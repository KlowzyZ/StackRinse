use crate::ScanOptions;
use std::{
    fs::{self, Metadata},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn linked(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

pub fn system_path(path: &Path) -> bool {
    #[cfg(unix)]
    {
        [
            "/bin",
            "/boot",
            "/dev",
            "/etc",
            "/lib",
            "/lib64",
            "/proc",
            "/root",
            "/run",
            "/sbin",
            "/sys",
            "/usr",
            "/var/lib",
            "/var/log",
            "/System",
            "/Library",
            "/Applications",
            "/private/etc",
            "/private/var/db",
        ]
        .iter()
        .any(|p| path.starts_with(p))
    }
    #[cfg(windows)]
    {
        // Canonical paths can use a verbatim drive prefix. Compare components.
        let first = path.components().find_map(|c| match c {
            std::path::Component::Normal(v) => Some(v.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        });
        matches!(
            first.as_deref(),
            Some(
                "windows"
                    | "program files"
                    | "program files (x86)"
                    | "programdata"
                    | "system volume information"
                    | "$recycle.bin"
            )
        )
    }
}

pub fn excluded(path: &Path, opts: &ScanOptions) -> bool {
    opts.exclusions.iter().any(|e| {
        if e.is_absolute() {
            path.starts_with(e)
        } else {
            // Traversal checks every visited directory. Keep case folding on
            // the current entry so an outside ancestor (such as AppData/Local)
            // cannot silently exclude the selected workspace.
            path.file_name().is_some_and(|name| {
                name == e.as_os_str() || (cfg!(windows) && name.eq_ignore_ascii_case(e.as_os_str()))
            })
        }
    })
}

pub fn has_git_marker(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"))
}

pub fn repository_boundary(path: &Path) -> Option<&'static str> {
    let mut seen = false;
    for dir in path.ancestors() {
        match fs::symlink_metadata(dir.join(".git")) {
            Ok(m) if linked(&m) || !m.is_dir() => {
                return Some("Linked Git worktree or submodule boundary.")
            }
            Ok(_) if seen => return Some("Nested Git repository boundary."),
            Ok(_) => seen = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Some("Repository boundary could not be inspected."),
        }
    }
    None
}

fn wait_for_index(mut child: Child, cancel: &AtomicBool, timeout: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    let mut delay = Duration::from_millis(1);
    while !cancel.load(Ordering::Relaxed) && started.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => std::thread::sleep(delay),
            Err(_) => break,
        }
        delay = (delay * 2).min(Duration::from_millis(20));
    }
    // Reap only the Git process started for this check.
    let _ = child.kill();
    let _ = child.wait();
    None
}

pub fn tracked(project: &Path, relative: &str, cancel: &AtomicBool) -> Option<&'static str> {
    let mut repository = None;
    for dir in project.ancestors() {
        match fs::symlink_metadata(dir.join(".git")) {
            Ok(_) => {
                repository = Some(dir);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Some("Repository boundary could not be inspected."),
        }
    }
    let repository = repository?;
    // ls-files reads the index. Disable hooks, external fsmonitor and optional
    // index updates; never invoke project commands or filters. Git must be on PATH.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let executable = std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|file| file.is_file());
    let Some(executable) = executable else {
        return Some("Git executable unavailable for index verification.");
    };
    let mut git = Command::new(executable);
    // Ignore ambient repository/index overrides and injected command config.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().get(..4).is_some_and(|prefix| {
            prefix == "GIT_" || (cfg!(windows) && prefix.eq_ignore_ascii_case("GIT_"))
        }) {
            git.env_remove(key);
        }
    }
    let artifact = project
        .strip_prefix(repository)
        .expect("repository is an ancestor")
        .join(relative);
    let mut pathspec = std::ffi::OsString::from(if cfg!(any(windows, target_os = "macos")) {
        ":(icase,literal)"
    } else {
        ":(literal)"
    });
    #[cfg(windows)]
    let artifact = artifact.to_string_lossy().replace('\\', "/");
    pathspec.push(artifact);
    let child = git
        .arg("-C")
        .arg(repository)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args([
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--cached",
            "--error-unmatch",
            "--",
        ])
        .arg(pathspec)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let status = child
        .ok()
        .and_then(|c| wait_for_index(c, cancel, Duration::from_secs(15)));
    match status.and_then(|s| s.code()) {
        Some(0) => Some("The Git index contains tracked paths in this artifact."),
        Some(1) => None,
        _ => Some("The Git index could not be verified."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_and_timed_out_git_checks_cannot_approve_a_candidate() {
        for (cancel, timeout) in [
            (true, Duration::from_secs(15)),
            (false, Duration::from_millis(10)),
        ] {
            // hash-object waits on our open stdin pipe and does not write an object.
            let child = Command::new("git")
                .args(["hash-object", "--stdin"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            assert!(wait_for_index(child, &AtomicBool::new(cancel), timeout).is_none());
        }
    }
}
