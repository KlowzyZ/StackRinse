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
impl Rule {
    pub fn matches_directory(&self, name: &str) -> bool {
        self.names.iter().any(|expected| {
            name == *expected || (cfg!(windows) && name.eq_ignore_ascii_case(expected))
        })
    }
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
    rule("Godot", &[".godot"], &[".gdignore"], "Editor caches", "Open the project in Godot to regenerate imports and cache data. Review editor-local state before rebuilding.", Cost::Medium),
    rule("Go", &["vendor"], &["modules.txt"], "Dependencies", "Regenerate vendored dependencies with the project’s Go toolchain and module sources. Keep local vendor patches before rebuilding.", Cost::High),
    rule("CMake", &["build", "cmake-build-debug", "cmake-build-release"], &["CMakeCache.txt"], "Build outputs", "Reconfigure with the original CMake generator, toolchain and options, then rebuild. Local cache settings and custom outputs may be unique.", Cost::High),
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
        "go.mod" => Some("Go"),
        "project.godot" => Some("Godot"),
        "CMakeLists.txt" => Some("CMake"),
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

#[derive(Default)]
pub struct ProjectEvidence {
    ecosystems: Vec<String>,
    latest: Option<u64>,
}

impl ProjectEvidence {
    pub fn record(&mut self, entry: &fs::DirEntry) -> io::Result<()> {
        let name = entry.file_name();
        let Some(eco) = name.to_str().and_then(manifest) else {
            return Ok(());
        };
        let meta = entry.metadata().map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", entry.path().display()))
        })?;
        if !linked(&meta) && meta.is_file() {
            if !self.ecosystems.iter().any(|s| s == eco) {
                self.ecosystems.push(eco.to_owned());
            }
            self.latest = self.latest.max(
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            );
        }
        Ok(())
    }

    pub fn finish(mut self) -> (Vec<String>, Option<u64>) {
        self.ecosystems.sort();
        (self.ecosystems, self.latest)
    }
}

fn metadata_if_present(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!("{}: {error}", path.display()),
        )),
    }
}

/// Directory markers carry a trailing slash in the rule table.
pub fn matched_marker(path: &Path, rule: &Rule) -> io::Result<Option<&'static str>> {
    for &marker in rule.evidence {
        let name = marker.trim_end_matches('/');
        let Some(meta) = metadata_if_present(&path.join(name))? else {
            continue;
        };
        if linked(&meta)
            || !(if marker.ends_with('/') {
                meta.is_dir()
            } else {
                meta.is_file()
            })
        {
            continue;
        }
        // A build directory can itself be a CMake source project, even in a mixed workspace.
        if rule.category == "Build outputs"
            && metadata_if_present(&path.join("CMakeLists.txt"))?.is_some()
        {
            return Ok(None);
        }
        let (directory, evidence) = match rule.ecosystem {
            "CMake" => ("CMakeFiles", "CMakeCache.txt and CMakeFiles/"),
            "Godot" => ("imported", ".gdignore and imported/"),
            _ => return Ok(Some(name)),
        };
        return Ok(metadata_if_present(&path.join(directory))?
            .filter(|m| !linked(m) && m.is_dir())
            .map(|_| evidence));
    }
    Ok(None)
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
