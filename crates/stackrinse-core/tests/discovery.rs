use stackrinse_core::{dry_run, dry_run_selected, scan, Classification, Report, ScanOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};
use tempfile::{tempdir, TempDir};

fn root() -> (TempDir, PathBuf) {
    let temp = tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap();
    (temp, path)
}
fn file(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn inventory(dir: &Path, opts: ScanOptions) -> Report {
    scan(&[dir.to_owned()], &opts, &AtomicBool::new(false), |_| {}).unwrap()
}

#[test]
fn polysystem_context_and_totals() {
    let (_t, r) = root();
    for (manifest, marker) in [
        ("node/package.json", "node/node_modules/.package-lock.json"),
        ("python/pyproject.toml", "python/.venv/pyvenv.cfg"),
        ("rust/Cargo.toml", "rust/target/.rustc_info.json"),
        ("jvm/pom.xml", "jvm/target/classes/Hello.class"),
        ("kotlin/build.gradle.kts", "kotlin/build/classes/App.class"),
        ("dotnet/App.csproj", "dotnet/obj/project.assets.json"),
        ("hybrid/package.json", "hybrid/.next/BUILD_ID"),
        ("hybrid/pyproject.toml", "hybrid/.pytest_cache/CACHEDIR.TAG"),
    ] {
        file(&r, manifest, "");
        file(&r, marker, "123");
    }
    let report = inventory(&r, ScanOptions::default());
    assert_eq!(report.projects.len(), 7);
    assert!(report.projects.iter().all(|p| p.last_activity.is_some()));
    let plan = dry_run(&report);
    assert_eq!(plan.logical_bytes, 24);
    assert_eq!(plan.candidates.len(), 8);
    assert_eq!(
        dry_run_selected(&report, |p, _| p.name == "hybrid").logical_bytes,
        6
    );
}

#[test]
fn names_or_ignored_paths_alone_are_insufficient() {
    let (_t, r) = root();
    file(&r, "unrelated/target/.rustc_info.json", "1");
    file(&r, "crate/Cargo.toml", "");
    file(&r, "crate/.gitignore", "target/");
    file(&r, "crate/target/photos.txt", "important");
    let report = inventory(&r, ScanOptions::default());
    assert_eq!(report.projects.len(), 1);
    assert!(dry_run(&report).candidates.is_empty());
}

#[test]
fn mixed_bytecode_and_user_files_are_protected() {
    let (_t, r) = root();
    file(&r, "p/pyproject.toml", "");
    file(&r, "p/foo.py", "source");
    file(&r, "p/__pycache__/foo.cpython-312.pyc", "batch");
    let report = inventory(&r, ScanOptions::default());
    assert_eq!(
        dry_run(&report).candidates[0].classification,
        Classification::Rebuildable
    );
    file(&r, "p/__pycache__/notes", "changes since scan");
    let next = inventory(&r, ScanOptions::default());
    assert_eq!(
        next.projects[0].candidates[0].classification,
        Classification::Protected
    );
    assert!(dry_run(&next).candidates.is_empty());
    assert_eq!(
        fs::read_to_string(r.join("p/__pycache__/notes")).unwrap(),
        "changes since scan"
    );
}
#[test]
fn deduplicates_overlapping_unicode_roots_and_preserves_protection() {
    let (_t, r) = root();
    let project = r.join("édition 一");
    file(&project, "Cargo.toml", "");
    file(&project, "target/.rustc_info.json", "{}");
    let options = ScanOptions {
        protected_projects: vec![project.clone()],
        ..Default::default()
    };
    let report = scan(
        &[project.clone(), r.clone(), r],
        &options,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.projects.len(), 1);
    assert!(report.projects[0].protected);
    assert!(dry_run(&report).candidates.is_empty());
}
#[test]
fn exclusions_apply_inside_artifacts_and_discovery() {
    let (_t, r) = root();
    file(&r, "app/Cargo.toml", "");
    file(&r, "app/target/.rustc_info.json", "{}");
    file(&r, "app/target/secrets/private", "1234");
    file(&r, "secrets/package.json", "");
    let options = ScanOptions {
        exclusions: vec!["secrets".into()],
        ..Default::default()
    };
    let report = inventory(&r, options);
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].logical_bytes, 2);
    assert!(dry_run(&report).candidates.is_empty());
}

#[test]
fn git_markers_in_artifacts_and_worktrees_block_planning() {
    let (_t, r) = root();
    file(&r, "worktree/.git", "gitdir: /never/follow");
    for p in ["worktree", "artifact", "outer/nested"] {
        file(&r, &format!("{p}/Cargo.toml"), "");
        file(&r, &format!("{p}/target/.rustc_info.json"), "1");
    }
    file(&r, "artifact/target/vendor/.git/config", "secret");
    fs::create_dir_all(r.join("outer/.git")).unwrap();
    fs::create_dir_all(r.join("outer/nested/.git")).unwrap();
    let report = inventory(&r, Default::default());
    assert_eq!(report.projects.len(), 3);
    assert!(dry_run(&report).candidates.is_empty());
}
#[test]
fn git_index_marks_tracked_content_protected() {
    let (_t, r) = root();
    assert!(Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&r)
        .status()
        .unwrap()
        .success());
    file(&r, "package.json", "");
    file(&r, "node_modules/.package-lock.json", "{}");
    assert_eq!(
        dry_run(&inventory(&r, Default::default())).candidates.len(),
        1
    );
    assert!(Command::new("git")
        .current_dir(&r)
        .args(["add", "-f", "node_modules"])
        .status()
        .unwrap()
        .success());
    let report = inventory(&r, Default::default());
    assert!(report.projects[0].candidates[0].reason.contains("tracked"));
    assert!(dry_run(&report).candidates.is_empty());
}
#[cfg(unix)]
#[test]
fn symlinks_in_roots_manifests_and_artifacts_are_never_followed() {
    use std::os::unix::fs::symlink;
    let (_t, r) = root();
    let (_u, outside) = root();
    file(&outside, "private", "secret");
    file(&r, "app/Cargo.toml", "");
    file(&r, "app/target/.rustc_info.json", "x");
    symlink(&outside, r.join("app/target/elsewhere")).unwrap();
    symlink(&outside, r.join("other")).unwrap();
    fs::create_dir_all(r.join("bad")).unwrap();
    symlink(outside.join("private"), r.join("bad/package.json")).unwrap();
    let report = inventory(&r, Default::default());
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].logical_bytes, 1);
    assert!(dry_run(&report).candidates.is_empty());
    assert!(scan(
        &[r.join("other")],
        &Default::default(),
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
}
#[test]
fn cancellation_and_invalid_paths_fail_closed() {
    let (_t, r) = root();
    let cancel = AtomicBool::new(true);
    let report = scan(
        std::slice::from_ref(&r),
        &Default::default(),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(report.cancelled);
    assert!(dry_run(&report).candidates.is_empty());
    let options = ScanOptions {
        exclusions: vec!["../elsewhere".into()],
        ..Default::default()
    };
    assert!(scan(std::slice::from_ref(&r), &options, &cancel, |_| {}).is_err());
    assert!(scan(&[r.join("../escape")], &Default::default(), &cancel, |_| {}).is_err());
    assert!(scan(&[], &Default::default(), &cancel, |_| {}).is_err());
}

#[test]
fn nested_python_bytecode_belongs_to_nearest_project() {
    let (_t, r) = root();
    file(&r, "pyproject.toml", "");
    file(&r, "src/lib.py", "");
    file(&r, "src/__pycache__/lib.cpython-312.pyc", "a");
    file(&r, "child/package.json", "");
    file(&r, "child/src/lib.py", "");
    file(&r, "child/src/__pycache__/lib.cpython-312.pyc", "b");
    file(&r, "nested/.git", "gitdir: elsewhere");
    file(&r, "nested/src/lib.py", "");
    file(&r, "nested/src/__pycache__/lib.cpython-312.pyc", "c");
    let report = inventory(&r, Default::default());
    let plan = dry_run(&report);
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].path, r.join("src/__pycache__"));
}
