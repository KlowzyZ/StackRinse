use crate::{policy, Cost, ScanOptions};
use std::{
    borrow::Cow,
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::Path,
    time::UNIX_EPOCH,
};

pub enum Evidence {
    Any(&'static [&'static str]),
    All(&'static [&'static str]),
    TaggedCache {
        tool: &'static str,
        creator_line: &'static [u8],
    },
    PythonBytecode,
}

pub struct Rule {
    pub ecosystem: &'static str,
    pub names: &'static [&'static str],
    pub evidence: Evidence,
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
    evidence: Evidence,
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
    Evidence::PythonBytecode,
    "Compiler caches",
    "Python recreates bytecode when the corresponding source modules are imported.",
    Cost::Low,
);

pub const RULES: &[Rule] = &[
    rule("Godot", &[".godot"], Evidence::All(&[".gdignore", "imported/"]), "Editor caches", "Open the project in Godot to regenerate imports and cache data. Review editor-local state before rebuilding.", Cost::Medium),
    rule("Go", &["vendor"], Evidence::Any(&["modules.txt"]), "Dependencies", "Regenerate vendored dependencies with the project’s Go toolchain and module sources. Keep local vendor patches before rebuilding.", Cost::High),
    rule("CMake", &["build", "cmake-build-debug", "cmake-build-release"], Evidence::All(&["CMakeCache.txt", "CMakeFiles/"]), "Build outputs", "Reconfigure with the original CMake generator, toolchain and options, then rebuild. Local cache settings and custom outputs may be unique.", Cost::High),
    rule("Node.js", &["node_modules"], Evidence::Any(&[".package-lock.json", ".modules.yaml", ".yarn-state.yml"]), "Dependencies", "Reinstall dependencies with the project’s package manager. Local package edits may be lost.", Cost::High),
    rule("Node.js", &[".next"], Evidence::Any(&["BUILD_ID", "build-manifest.json", "server/"]), "Build outputs", "The next Next.js build recreates its output.", Cost::Medium),
    rule("Node.js", &[".nuxt"], Evidence::Any(&["tsconfig.json", "nuxt.d.ts"]), "Build outputs", "Nuxt regenerates types and build metadata.", Cost::Medium),
    rule("Rust", &["target"], Evidence::Any(&[".rustc_info.json", "CACHEDIR.TAG"]), "Build outputs", "Cargo recompiles dependencies and local crates. Custom artifacts may also be present.", Cost::High),
    rule("Python", &[".venv", "venv"], Evidence::Any(&["pyvenv.cfg"]), "Virtual environments", "Recreate the environment and reinstall its packages. Manually installed packages may be difficult to reproduce.", Cost::High),
    rule("Python", &[".pytest_cache"], Evidence::Any(&["CACHEDIR.TAG"]), "Test caches", "pytest recreates cache entries. Last-failed and local plugin state may be lost.", Cost::Low),
    rule("Dart", &[".dart_tool"], Evidence::All(&["package_config.json", "package_graph.json"]), "Tooling metadata", "Run dart pub get (or flutter pub get for a Flutter project) with the original SDK and dependencies. Review other tools’ local state in .dart_tool.", Cost::Medium),
    rule("PHP", &["vendor"], Evidence::All(&["autoload.php", "composer/installed.json", "composer/autoload_real.php"]), "Dependencies", "Run composer install with the original lockfile, PHP version, extensions and available package sources. Preserve local dependency edits and patches.", Cost::High),
    rule("Python", &[".ruff_cache"], Evidence::TaggedCache { tool: "Ruff", creator_line: b"# Automatically created by ruff." }, "Analysis caches", "Run Ruff again with the original version and configuration. Review any files added to the cache by hand.", Cost::Low),
    rule("Python", &[".mypy_cache"], Evidence::TaggedCache { tool: "mypy", creator_line: b"# Automatically created by mypy" }, "Analysis caches", "Run mypy with the original Python environment, plugins and configuration. Incremental type-checking results are recreated.", Cost::Low),
    PYTHON_BYTECODE,
    rule(".NET", &["obj"], Evidence::Any(&["project.assets.json"]), "Build outputs", "Restore NuGet packages and rebuild intermediate files.", Cost::Medium),
    rule(".NET", &["bin"], Evidence::Any(&["Debug/", "Release/"]), "Build outputs", "Rebuild the project. Unpublished binaries or manually copied files may be present.", Cost::Medium),
    rule("Maven", &["target"], Evidence::Any(&["classes/", "maven-status/"]), "Build outputs", "Run the Maven build to recreate compiled classes and packages.", Cost::Medium),
    rule("Gradle", &["build"], Evidence::Any(&["classes/", "generated/"]), "Build outputs", "Run the Gradle build to regenerate classes and outputs.", Cost::Medium),
    rule("Gradle", &[".gradle"], Evidence::All(&["buildOutputCleanup/cache.properties", "buildOutputCleanup/outputFiles.bin"]), "Build caches", "Run the same Gradle tasks with the original wrapper, JDK and dependencies. Stop active builds before inspecting local cache state.", Cost::Medium),
];

fn manifest(name: &str) -> Option<&'static str> {
    match name {
        "package.json" => Some("Node.js"),
        "pyproject.toml" | "setup.py" | "setup.cfg" | "requirements.txt" => Some("Python"),
        "go.mod" => Some("Go"),
        "pubspec.yaml" => Some("Dart"),
        "composer.json" => Some("PHP"),
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
        // DirEntry::metadata inspects the entry without following a symbolic link.
        let meta = entry.metadata().map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", entry.path().display()))
        })?;
        if !policy::linked(&meta) && meta.is_file() {
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

/// Directory markers carry a trailing slash. Check each nested component
/// before probing its child so linked, excluded and mounted paths provide no evidence.
fn marker_present(
    artifact: &Path,
    marker: &str,
    options: &ScanOptions,
    volume: &fs::Metadata,
) -> io::Result<bool> {
    let mut path = artifact.to_path_buf();
    let mut parts = marker.trim_end_matches('/').split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        if policy::excluded(&path, options) {
            return Ok(false);
        }
        let Some(meta) = metadata_if_present(&path)? else {
            return Ok(false);
        };
        let expect_directory = parts.peek().is_some() || marker.ends_with('/');
        if policy::linked(&meta)
            || policy::other_volume(volume, &meta)
            || if expect_directory {
                !meta.is_dir()
            } else {
                !meta.is_file()
            }
        {
            return Ok(false);
        }
    }
    Ok(true)
}

const CACHE_TAG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";

fn first_line_matches(path: &Path, signature: &[u8], volume: &fs::Metadata) -> io::Result<bool> {
    let read = || {
        let mut open = fs::OpenOptions::new();
        open.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            open.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = open.open(path)?;
        let meta = file.metadata()?;
        if policy::linked(&meta) || !meta.is_file() || policy::other_volume(volume, &meta) {
            return Ok(false);
        }
        // A small fixed read is enough to prove the tool signature.
        let limit = signature.len() + 2;
        let mut header = Vec::with_capacity(limit);
        file.take(limit as u64).read_to_end(&mut header)?;
        let line = header
            .split(|&byte| byte == b'\n')
            .next()
            .unwrap_or_default();
        Ok(line.strip_suffix(b"\r").unwrap_or(line) == signature)
    };
    read().map_err(|error: io::Error| {
        io::Error::new(error.kind(), format!("{}: {error}", path.display()))
    })
}

pub fn matched_marker(
    artifact: &Path,
    rule: &Rule,
    options: &ScanOptions,
    volume: &fs::Metadata,
) -> io::Result<Option<Cow<'static, str>>> {
    let present = |marker| marker_present(artifact, marker, options, volume);
    let matched = match rule.evidence {
        Evidence::Any(markers) => {
            let mut matched = None;
            for &marker in markers {
                if present(marker)? {
                    matched = Some(Cow::Borrowed(marker));
                    break;
                }
            }
            matched
        }
        Evidence::All(markers) => {
            for marker in markers {
                if !present(marker)? {
                    return Ok(None);
                }
            }
            Some(Cow::Owned(markers.join(" and ")))
        }
        Evidence::TaggedCache { tool, creator_line } => {
            if !present("CACHEDIR.TAG")?
                || !present(".gitignore")?
                || !first_line_matches(&artifact.join("CACHEDIR.TAG"), CACHE_TAG, volume)?
                || !first_line_matches(&artifact.join(".gitignore"), creator_line, volume)?
            {
                return Ok(None);
            }
            Some(Cow::Owned(format!(
                "cache tag and {tool}-generated .gitignore"
            )))
        }
        Evidence::PythonBytecode => None,
    };
    // A build output may instead be a CMake source project configured in place.
    if matched.is_some()
        && rule.category == "Build outputs"
        && metadata_if_present(&artifact.join("CMakeLists.txt"))?.is_some()
    {
        return Ok(None);
    }
    Ok(matched)
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
            .is_ok_and(|m| !policy::linked(&m) && m.is_file())
}
