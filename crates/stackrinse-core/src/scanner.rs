use crate::{policy, rules, Candidate, Classification, Progress, Project, Report, ScanOptions};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Select at least one directory to scan")]
    NoRoots,
    #[error("Cannot scan {path}: {reason}")]
    InvalidRoot { path: PathBuf, reason: String },
    #[error("Invalid scan preferences: {0}")]
    Preferences(String),
    #[error("Could not start filesystem workers: {0}")]
    Workers(String),
}

fn normalize_root(path: &Path) -> Result<PathBuf, ScanError> {
    let err = |reason: &str| ScanError::InvalidRoot {
        path: path.into(),
        reason: reason.into(),
    };
    if path.as_os_str().is_empty() || path.components().any(|c| c == Component::ParentDir) {
        return Err(err("empty or parent-relative path"));
    }
    let absolute = std::path::absolute(path).map_err(|e| err(&e.to_string()))?;
    for ancestor in absolute.ancestors() {
        let meta = fs::symlink_metadata(ancestor).map_err(|e| err(&e.to_string()))?;
        if !meta.is_dir() || policy::linked(&meta) {
            return Err(err(
                "root or ancestor is a link, reparse point, or non-directory",
            ));
        }
    }
    let canonical = fs::canonicalize(path).map_err(|e| err(&e.to_string()))?;
    if policy::system_path(&canonical)
        || policy::has_git_marker(&canonical)
        || canonical.to_str().is_none()
    {
        return Err(err("protected or unrepresentable location"));
    }
    Ok(canonical)
}

fn warning(warnings: &mut Vec<String>, message: String) {
    const LIMIT: usize = 200;
    if warnings.len() < LIMIT {
        warnings.push(message);
    } else if warnings.len() == LIMIT {
        warnings.push("Additional scan warnings suppressed.".into());
    }
}

fn stopped(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

#[cfg(unix)]
fn other_volume(root: &fs::Metadata, child: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    root.dev() != child.dev()
}
#[cfg(windows)]
fn other_volume(_: &fs::Metadata, _: &fs::Metadata) -> bool {
    false
}

fn measure(
    project: &Path,
    name: &str,
    rule: &rules::Rule,
    options: &ScanOptions,
    cancel: &AtomicBool,
    protection: Option<&str>,
) -> (Candidate, Option<String>) {
    let path = project.join(name);
    let mut blocked = protection.map(str::to_owned);
    let mut bytes = 0u64;
    let mut read_error = None;
    let mut size_complete = true;
    let mut files = 0u64;
    let bytecode = name == "__pycache__";
    let mut all_bytecode = true;
    let base_meta = fs::symlink_metadata(project);
    let mut walk = WalkDir::new(&path)
        .follow_root_links(false)
        .max_open(12)
        .into_iter();
    while !stopped(cancel) {
        let Some(entry) = walk.next() else {
            break;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                size_complete = false;
                read_error.get_or_insert_with(|| format!("{}: {e}", path.display()));
                blocked = Some(format!("Incomplete inventory: {e}"));
                continue;
            }
        };
        let entry_path = entry.path();
        if policy::has_git_marker(entry_path)
            || policy::excluded(entry_path, options)
            || options
                .protected_projects
                .iter()
                .any(|p| entry_path.starts_with(p))
            || entry_path.to_str().is_none()
        {
            size_complete = false;
            blocked = Some("Contains repository metadata, a protected path, an exclusion, or an unrepresentable name.".into());
            if entry.file_type().is_dir() {
                walk.skip_current_dir();
            }
            continue;
        }
        let meta = match fs::symlink_metadata(entry_path) {
            Ok(m) => m,
            Err(e) => {
                size_complete = false;
                read_error.get_or_insert_with(|| format!("{}: {e}", entry_path.display()));
                blocked = Some(format!("Metadata unavailable: {e}"));
                if entry.file_type().is_dir() {
                    walk.skip_current_dir();
                }
                continue;
            }
        };
        if policy::linked(&meta)
            || base_meta.as_ref().map_or(true, |m| other_volume(m, &meta))
            || !(meta.is_dir() || meta.is_file())
        {
            size_complete = false;
            blocked = Some("Contains a filesystem link, mount boundary, or special file.".into());
            if entry.file_type().is_dir() {
                walk.skip_current_dir();
            }
            continue;
        }
        if meta.is_file() {
            files += 1;
            bytes = bytes.saturating_add(meta.len());
            if bytecode && (entry.depth() != 1 || !rules::bytecode_source(entry_path, project)) {
                all_bytecode = false;
            }
        } else if bytecode && entry.depth() > 0 {
            all_bytecode = false;
        }
    }
    if blocked.is_none() && !stopped(cancel) {
        blocked = policy::tracked(project, name, cancel).map(str::to_owned);
    }
    if bytecode && (!all_bytecode || files == 0) {
        blocked.get_or_insert_with(|| "Bytecode could not be matched to adjacent source modules, or unexpected contents were found.".into());
    }
    let (classification, reason) = match blocked {
        Some(reason) => (Classification::Protected, reason),
        None if bytecode => (
            Classification::Rebuildable,
            "Only interpreter-named bytecode with corresponding source modules was observed."
                .into(),
        ),
        None => match rules::matched_marker(&path, rule) {
            Ok(Some(marker)) => (
                Classification::ReviewRequired,
                format!(
                    "{} project and {marker} marker found. Review the directory for local changes.",
                    rule.ecosystem
                ),
            ),
            Err(error) => {
                let message = format!("Cannot verify artifact evidence: {error}");
                read_error = Some(message.clone());
                (Classification::Protected, message)
            }
            Ok(None) => (
                Classification::Protected,
                "Artifact evidence changed during the scan.".into(),
            ),
        },
    };
    (
        Candidate {
            path,
            category: rule.category.into(),
            logical_bytes: bytes,
            size_complete,
            classification,
            reason,
            rebuild: rule.rebuild.into(),
            cost: rule.cost,
        },
        read_error,
    )
}

fn normalize_options(options: &ScanOptions) -> Result<ScanOptions, ScanError> {
    fn path(p: &Path, basename: bool) -> Result<PathBuf, ScanError> {
        if p.is_absolute() && !p.components().any(|c| c == Component::ParentDir) {
            // Match canonical scan paths (including verbatim Windows prefixes).
            return Ok(p.canonicalize().unwrap_or_else(|_| p.to_owned()));
        }
        if basename
            && p.components().count() == 1
            && matches!(p.components().next(), Some(Component::Normal(_)))
        {
            return Ok(p.to_owned());
        }
        Err(ScanError::Preferences(format!(
            "Invalid path rule {}",
            p.display()
        )))
    }
    Ok(ScanOptions {
        exclusions: options
            .exclusions
            .iter()
            .map(|p| path(p, true))
            .collect::<Result<_, _>>()?,
        protected_projects: options
            .protected_projects
            .iter()
            .map(|p| path(p, false))
            .collect::<Result<_, _>>()?,
    })
}

pub fn scan(
    roots: &[PathBuf],
    options: &ScanOptions,
    cancel: &AtomicBool,
    progress: impl Fn(Progress) + Sync,
) -> Result<Report, ScanError> {
    if roots.is_empty() {
        return Err(ScanError::NoRoots);
    }
    let options = normalize_options(options)?;
    let mut roots = roots
        .iter()
        .map(|r| normalize_root(r))
        .collect::<Result<Vec<_>, _>>()?;
    roots.sort();
    let mut unique: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !unique.iter().any(|r| root.starts_with(r)) {
            unique.push(root);
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(std::thread::available_parallelism().map_or(1, |n| n.get().min(4)))
        .build()
        .map_err(|e| ScanError::Workers(e.to_string()))?;
    let mut report = Report::default();
    let mut last = None::<Instant>;
    for root in unique {
        let volume = fs::symlink_metadata(&root).map_err(|e| ScanError::InvalidRoot {
            path: root.clone(),
            reason: e.to_string(),
        })?;
        let mut skip = HashSet::new();
        let mut owners: Vec<usize> = Vec::new();
        let mut walk = WalkDir::new(root)
            .follow_root_links(false)
            .max_open(16)
            .into_iter();
        while !stopped(cancel) {
            let Some(entry) = walk.next() else {
                break;
            };
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warning(&mut report.warnings, e.to_string());
                    continue;
                }
            };
            if !entry.file_type().is_dir() {
                continue;
            }
            let path = entry.path();
            if skip.remove(path)
                || policy::has_git_marker(path)
                || policy::system_path(path)
                || policy::excluded(path, &options)
            {
                walk.skip_current_dir();
                continue;
            }
            match fs::symlink_metadata(path) {
                Ok(meta)
                    if !policy::linked(&meta)
                        && !other_volume(&volume, &meta)
                        && path.to_str().is_some() => {}
                Ok(_) => {
                    walk.skip_current_dir();
                    continue;
                }
                Err(e) => {
                    warning(&mut report.warnings, format!("{}: {e}", path.display()));
                    walk.skip_current_dir();
                    continue;
                }
            }
            report.directories += 1;
            if last.is_none_or(|time| time.elapsed() >= Duration::from_millis(120)) {
                progress(Progress {
                    directories: report.directories,
                    projects: report.projects.len() as u64,
                });
                last = Some(Instant::now());
            }
            while owners
                .last()
                .is_some_and(|&i| !path.starts_with(&report.projects[i].path))
            {
                owners.pop();
            }
            if path.file_name().is_some_and(|name| name == "__pycache__") {
                if let Some(owner) = owners.last().map(|&i| &mut report.projects[i]) {
                    let parent = path.parent().expect("a named directory has a parent");
                    let crosses_repository = parent.ancestors().take_while(|p| *p != owner.path)
                        .any(|p| !matches!(p.join(".git").symlink_metadata(), Err(e) if e.kind() == io::ErrorKind::NotFound));
                    if !crosses_repository && owner.ecosystems.iter().any(|e| e == "Python") {
                        let protection = owner
                            .protected
                            .then_some("The owning project is protected.");
                        let (candidate, read_error) = measure(
                            parent,
                            "__pycache__",
                            &rules::PYTHON_BYTECODE,
                            &options,
                            cancel,
                            protection,
                        );
                        if !stopped(cancel) {
                            if let Some(message) = read_error {
                                warning(&mut report.warnings, message);
                            }
                            owner.logical_bytes =
                                owner.logical_bytes.saturating_add(candidate.logical_bytes);
                            owner.candidates.push(candidate);
                        }
                        walk.skip_current_dir();
                        continue;
                    }
                }
            }
            let (ecosystems, last_activity) = match rules::manifests(path, cancel) {
                Ok(data) => data,
                Err(e) => {
                    warning(&mut report.warnings, format!("{}: {e}", path.display()));
                    walk.skip_current_dir();
                    continue;
                }
            };
            if ecosystems.is_empty() {
                continue;
            }
            let boundary = policy::repository_boundary(path);
            let preference = options
                .protected_projects
                .iter()
                .any(|p| path.starts_with(p));
            let protection = boundary
                .or(preference.then_some("This project is protected by a user preference."));
            let mut jobs = Vec::new();
            for rule in rules::RULES
                .iter()
                .filter(|r| ecosystems.iter().any(|e| e == r.ecosystem))
            {
                for &name in rule.names {
                    let artifact = path.join(name);
                    if policy::excluded(&artifact, &options) {
                        continue;
                    }
                    match fs::symlink_metadata(&artifact) {
                        Ok(m)
                            if m.is_dir() && !policy::linked(&m) && !other_volume(&volume, &m) =>
                        {
                            if jobs.iter().any(|(n, _)| *n == name) {
                                continue;
                            }
                            let matched = match rules::matched_marker(&artifact, rule) {
                                Ok(marker) => rule.evidence.is_empty() || marker.is_some(),
                                Err(error) => {
                                    warning(&mut report.warnings, error.to_string());
                                    false
                                }
                            };
                            if matched {
                                skip.insert(artifact);
                                jobs.push((name, rule));
                            }
                        }
                        Err(e) if e.kind() != io::ErrorKind::NotFound => {
                            warning(&mut report.warnings, format!("{}: {e}", artifact.display()))
                        }
                        _ => {}
                    }
                }
            }
            let measurements: Vec<_> = pool.install(|| {
                jobs.par_iter()
                    .map(|(n, r)| measure(path, n, r, &options, cancel, protection))
                    .collect()
            });
            if stopped(cancel) {
                break;
            }
            let candidates: Vec<_> = measurements
                .into_iter()
                .map(|(candidate, read_error)| {
                    if let Some(message) = read_error {
                        warning(&mut report.warnings, message);
                    }
                    candidate
                })
                .collect();
            let logical_bytes = candidates
                .iter()
                .fold(0u64, |sum, c| sum.saturating_add(c.logical_bytes));
            owners.push(report.projects.len());
            report.projects.push(Project {
                path: path.into(),
                name: path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned(),
                ecosystems,
                logical_bytes,
                last_activity,
                protected: protection.is_some(),
                boundary_reason: boundary.map(str::to_owned),
                candidates,
            });
        }
        if stopped(cancel) {
            break;
        }
    }
    for project in &mut report.projects {
        project.candidates.sort_by(|a, b| a.path.cmp(&b.path));
    }
    report.projects.sort_by(|a, b| a.path.cmp(&b.path));
    report.cancelled = stopped(cancel);
    progress(Progress {
        directories: report.directories,
        projects: report.projects.len() as u64,
    });
    Ok(report)
}
