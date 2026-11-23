use crate::{policy, rules, Candidate, Classification, Progress, Project, Report, ScanOptions};
use rayon::prelude::*;
use std::{
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
    path: &Path,
    rule: &rules::Rule,
    options: &ScanOptions,
    cancel: &AtomicBool,
    protection: Option<&str>,
) -> (Candidate, Option<String>) {
    let project = path.parent().expect("candidate has an owning directory");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("only Unicode candidates are measured");
    let mut blocked = protection.map(str::to_owned);
    let mut bytes = 0u64;
    let mut read_error = None;
    let mut size_complete = true;
    let mut files = 0u64;
    let bytecode = rule.names == ["__pycache__"];
    let mut all_bytecode = true;
    let base_meta = fs::symlink_metadata(project);
    let mut walk = WalkDir::new(path)
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
    if stopped(cancel) {
        size_complete = false;
        blocked.get_or_insert_with(|| "Measurement interrupted.".into());
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
        None => match rules::matched_marker(path, rule) {
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
            path: path.into(),
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
        if stopped(cancel) {
            break;
        }
        let volume = match fs::symlink_metadata(&root) {
            Ok(meta) if meta.is_dir() && !policy::linked(&meta) => meta,
            Ok(_) => {
                warning(
                    &mut report.warnings,
                    format!("{}: scan root changed", root.display()),
                );
                continue;
            }
            Err(error) => {
                warning(&mut report.warnings, format!("{}: {error}", root.display()));
                continue;
            }
        };
        let mut owners: Vec<usize> = Vec::new();
        let mut pending = vec![root];
        while !stopped(cancel) {
            let Some(path) = pending.pop() else {
                break;
            };
            let path = path.as_path();
            if policy::has_git_marker(path)
                || policy::system_path(path)
                || policy::excluded(path, &options)
            {
                continue;
            }
            if path.to_str().is_none() {
                warning(
                    &mut report.warnings,
                    format!(
                        "{}: skipped directory with a non-Unicode path",
                        path.display()
                    ),
                );
                continue;
            }
            match fs::symlink_metadata(path) {
                Ok(meta)
                    if meta.is_dir() && !policy::linked(&meta) && !other_volume(&volume, &meta) => {
                }
                Ok(_) => continue,
                Err(e) => {
                    warning(&mut report.warnings, format!("{}: {e}", path.display()));
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
                if stopped(cancel) {
                    break;
                }
            }
            while owners
                .last()
                .is_some_and(|&i| !path.starts_with(&report.projects[i].path))
            {
                owners.pop();
            }
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| rules::PYTHON_BYTECODE.matches_directory(name))
            {
                if let Some(owner) = owners.last().map(|&i| &mut report.projects[i]) {
                    let parent = path.parent().expect("a named directory has a parent");
                    let crosses_repository = parent.ancestors().take_while(|p| *p != owner.path)
                        .any(|p| !matches!(p.join(".git").symlink_metadata(), Err(e) if e.kind() == io::ErrorKind::NotFound));
                    if !crosses_repository && owner.ecosystems.iter().any(|e| e == "Python") {
                        let protection = owner
                            .protected
                            .then_some("The owning project is protected.");
                        let (candidate, read_error) =
                            measure(path, &rules::PYTHON_BYTECODE, &options, cancel, protection);
                        if !stopped(cancel) {
                            if let Some(message) = read_error {
                                warning(&mut report.warnings, message);
                            }
                            owner.logical_bytes =
                                owner.logical_bytes.saturating_add(candidate.logical_bytes);
                            owner.candidates.push(candidate);
                        }
                        continue;
                    }
                }
            }
            let mut entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(e) => {
                    warning(&mut report.warnings, format!("{}: {e}", path.display()));
                    continue;
                }
            };
            let first_child = pending.len();
            let mut evidence = rules::ProjectEvidence::default();
            while !stopped(cancel) {
                let Some(entry) = entries.next() else {
                    break;
                };
                let result = entry.and_then(|entry| {
                    if entry.file_type()?.is_dir() {
                        pending.push(entry.path());
                    } else {
                        evidence.record(&entry)?;
                    }
                    Ok(())
                });
                if let Err(e) = result {
                    warning(&mut report.warnings, format!("{}: {e}", path.display()));
                }
            }
            if stopped(cancel) {
                break;
            }
            pending[first_child..].reverse();
            let (ecosystems, last_activity) = evidence.finish();
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
            let applicable: Vec<_> = rules::RULES
                .iter()
                .filter(|rule| ecosystems.iter().any(|e| e == rule.ecosystem))
                .collect();
            let mut jobs = Vec::new();
            // Select from the observed children, so a candidate is removed from
            // discovery by its actual spelling and is measured only once.
            pending
                .extract_if(first_child.., |artifact| {
                    if stopped(cancel) {
                        return false;
                    }
                    let Some(name) = artifact.file_name().and_then(|n| n.to_str()) else {
                        return false;
                    };
                    let mut matching = applicable
                        .iter()
                        .copied()
                        .filter(|rule| rule.matches_directory(name))
                        .peekable();
                    if matching.peek().is_none() || policy::excluded(artifact, &options) {
                        return false;
                    }
                    match fs::symlink_metadata(&*artifact) {
                        Ok(meta)
                            if meta.is_dir()
                                && !policy::linked(&meta)
                                && !other_volume(&volume, &meta) => {}
                        Ok(_) => return false,
                        Err(error) => {
                            warning(
                                &mut report.warnings,
                                format!("{}: {error}", artifact.display()),
                            );
                            return false;
                        }
                    }
                    for rule in matching {
                        match rules::matched_marker(artifact, rule) {
                            Ok(marker) if rule.evidence.is_empty() || marker.is_some() => {
                                jobs.push((artifact.clone(), rule));
                                return true;
                            }
                            Ok(_) => {}
                            Err(error) => warning(&mut report.warnings, error.to_string()),
                        }
                    }
                    false
                })
                .for_each(drop);
            if stopped(cancel) {
                break;
            }
            let measurements: Vec<_> = pool.install(|| {
                jobs.par_iter()
                    .map(|(artifact, rule)| measure(artifact, rule, &options, cancel, protection))
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
    progress(Progress {
        directories: report.directories,
        projects: report.projects.len() as u64,
    });
    report.cancelled = stopped(cancel);
    Ok(report)
}
