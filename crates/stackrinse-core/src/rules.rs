use crate::{policy::linked, Cost};
use std::{ffi::OsStr, fs, io, path::Path, time::UNIX_EPOCH};

pub struct Rule {
    pub ecosystem: &'static str,
    pub names: &'static [&'static str],
    pub evidence: &'static [&'static str],
    pub category: &'static str,
    pub rebuild: &'static str,
    pub cost: Cost,
}
const fn rule(
    ecosystem: &'static str,
    names: &'static [&'static str],
    evidence: &'static [&'static str],
    category: &'static str,
    rebuild: &'static str,
    cost: Cost,
) -> Rule {
    Rule {
        ecosystem,
        names,
        evidence,
        category,
        rebuild,
        cost,
    }
}
pub const PYTHON_BYTECODE: Rule = rule(
    "Python",
    &["__pycache__"],
    &[],
    "Compiler caches",
    "Python recreates bytecode when the corresponding source modules are imported.",
    Cost::Low,
);

pub const RULES: &[Rule] = &[
    rule("Node.js", &["node_modules"], &[".package-lock.json", ".modules.yaml", ".yarn-state.yml"], "Dependencies", "Reinstall dependencies with the project’s package manager. Local package edits may be lost.", Cost::High),
    rule("Node.js", &[".next"], &["BUILD_ID", "build-manifest.json", "server/"], "Build outputs", "The next Next.js build recreates its output.", Cost::Medium),
    rule("Node.js", &[".nuxt"], &["tsconfig.json", "nuxt.d.ts"], "Build outputs", "Nuxt regenerates types and build metadata.", Cost::Medium),
    rule("Rust", &["target"], &[".rustc_info.json", "CACHEDIR.TAG"], "Build outputs", "Cargo recompiles dependencies and local crates. Custom artifacts may also be present.", Cost::High),
    rule("Python", &[".venv", "venv"], &["pyvenv.cfg"], "Virtual environments", "Recreate the environment and reinstall its packages. Manually installed packages may be difficult to reproduce.", Cost::High),
    rule("Python", &[".pytest_cache"], &["CACHEDIR.TAG"], "Test caches", "pytest recreates cache entries. Last-failed and local plugin state may be lost.", Cost::Low),
    PYTHON_BYTECODE,
    rule(".NET", &["obj"], &["project.assets.json"], "Build outputs", "Restore NuGet packages and rebuild intermediate files.", Cost::Medium),
    rule(".NET", &["bin"], &["Debug/", "Release/"], "Build outputs", "Rebuild the project. Unpublished binaries or manually copied files may be present.", Cost::Medium),
    rule("Maven", &["target"], &["classes/", "maven-status/"], "Build outputs", "Run the Maven build to recreate compiled classes and packages.", Cost::Medium),
    rule("Gradle", &["build"], &["classes/", "generated/"], "Build outputs", "Run the Gradle build to regenerate classes and outputs.", Cost::Medium),
];

fn manifest(name: &str) -> Option<&'static str> {
    match name {
        "package.json" => Some("Node.js"),
        "pyproject.toml" | "setup.py" | "setup.cfg" | "requirements.txt" => Some("Python"),
        "Cargo.toml" => Some("Rust"),
        "pom.xml" => Some("Maven"),
        "build.gradle" | "build.gradle.kts" => Some("Gradle"),
        _ if [".csproj", ".fsproj", ".vbproj"]
            .iter()
            .any(|ext| name.ends_with(ext)) =>
        {
            Some(".NET")
        }
        _ => None,
    }
}

pub fn manifests(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> io::Result<(Vec<String>, Option<u64>)> {
    let mut ecosystems = Vec::new();
    let mut latest = None;
    for item in fs::read_dir(path)? {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let item = item?;
        let name = item.file_name();
        if let Some(eco) = name.to_str().and_then(manifest) {
            let meta = fs::symlink_metadata(item.path())?;
            if linked(&meta) || !meta.is_file() {
                continue;
            }
            if !ecosystems.iter().any(|s| s == eco) {
                ecosystems.push(eco.to_owned());
            }
            latest = latest.max(
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            );
        }
    }
    ecosystems.sort();
    Ok((ecosystems, latest))
}

/// Directory markers carry a trailing slash in the rule table.
pub fn matched_marker(path: &Path, rule: &Rule) -> Option<&'static str> {
    rule.evidence.iter().find_map(|&marker| {
        let directory = marker.ends_with('/');
        let name = marker.trim_end_matches('/');
        let meta = fs::symlink_metadata(path.join(name)).ok()?;
        (!linked(&meta)
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file()
            })
        .then_some(name)
    })
}

pub fn bytecode_source(file: &Path, project: &Path) -> bool {
    let Some(name) = file.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let Some((module, suffix)) = name.split_once(".cpython-") else {
        return false;
    };
    let Some(version) = suffix.strip_suffix(".pyc") else {
        return false;
    };
    !module.is_empty()
        && !version.is_empty()
        && version.bytes().all(|b| b.is_ascii_digit())
        && fs::symlink_metadata(project.join(format!("{module}.py")))
            .is_ok_and(|m| !linked(&m) && m.is_file())
}
