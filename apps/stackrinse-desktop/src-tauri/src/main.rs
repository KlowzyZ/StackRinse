#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod settings;

use history::{export, Comparison, Detail, HistoryIndex, HistoryStore, ScanTiming, Snapshot};
use settings::{Appearance, Result, Settings, Store};
use stackrinse_core::{Plan, Report, ScanOptions};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

struct ActiveScan {
    cancelled: Arc<AtomicBool>,
    accepting_cancel: bool,
}

type Slot = Arc<Mutex<Option<ActiveScan>>>;

#[derive(Clone)]
struct FreshScan {
    settings: Settings,
    report: Arc<Report>,
    timing: ScanTiming,
}

#[derive(Default)]
struct Session {
    scan: Slot,
    last_scan: Mutex<Option<FreshScan>>,
}

fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| "Application state unavailable. Restart StackRinse.".into())
}

struct ScanLease(Slot);
impl Drop for ScanLease {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
        }
    }
}

fn store(app: &AppHandle) -> Result<Store> {
    app.path()
        .app_config_dir()
        .map(Store)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<Settings> {
    store(&app)?.read()
}

#[tauri::command]
async fn choose_root(app: AppHandle) -> Result<Settings> {
    let picker_app = app.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        picker_app
            .dialog()
            .file()
            .set_title("Choose a StackRinse scan folder")
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(selected) = selected else {
        return get_settings(app);
    };
    let root = selected.into_path().map_err(|e| e.to_string())?;
    // Preserve the selected spelling so core can inspect it for links at scan time.
    if !root.is_absolute() || !root.is_dir() || root.to_str().is_none() {
        return Err("Choose an accessible folder with a Unicode path.".into());
    }
    store(&app)?.update(|s| {
        if !s.roots.contains(&root) {
            s.roots.push(root);
        }
    })
}

#[tauri::command]
fn remove_root(app: AppHandle, root: PathBuf) -> Result<Settings> {
    store(&app)?.update(|s| s.roots.retain(|p| p != &root))
}

#[tauri::command]
fn set_preferences(
    app: AppHandle,
    appearance: Appearance,
    excluded_names: Vec<String>,
) -> Result<Settings> {
    store(&app)?.update(|s| {
        s.appearance = appearance;
        s.excluded_names = excluded_names;
    })
}

#[tauri::command]
fn set_project_protection(
    app: AppHandle,
    session: State<'_, Session>,
    path: PathBuf,
    protected: bool,
) -> Result<Settings> {
    if protected
        && !lock(&session.last_scan)?
            .as_ref()
            .is_some_and(|scan| scan.report.projects.iter().any(|p| p.path == path))
    {
        return Err("Select a project from the latest scan.".into());
    }
    store(&app)?.update(|s| {
        s.protected_projects.retain(|p| p != &path);
        if protected {
            s.protected_projects.push(path);
        }
    })
}

#[derive(serde::Serialize, Clone)]
struct ScanProgress {
    visited: u64,
}

#[derive(serde::Serialize)]
struct ScanOutcome {
    #[serde(flatten)]
    report: Arc<Report>,
    #[serde(rename = "historyNotice")]
    history_notice: Option<String>,
    #[serde(rename = "completedAtMs")]
    completed_at_ms: i64,
}

#[tauri::command]
async fn scan(app: AppHandle, session: State<'_, Session>) -> Result<ScanOutcome> {
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut slot = lock(&session.scan)?;
        if slot.is_some() {
            return Err("A scan is already running.".into());
        }
        // A replacement scan invalidates the previous preview, including on failure.
        *lock(&session.last_scan)? = None;
        *slot = Some(ActiveScan {
            cancelled: cancelled.clone(),
            accepting_cancel: true,
        });
    }
    let lease = ScanLease(session.scan.clone());
    let settings = get_settings(app.clone())?;
    if settings.roots.is_empty() {
        return Err("Choose a scan folder first.".into());
    }
    let scan_settings = settings.clone();
    let scan_slot = session.scan.clone();
    let (result, timing, history_notice) = tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let options = ScanOptions {
            exclusions: settings
                .excluded_names
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            protected_projects: settings.protected_projects,
        };
        let last_event = Mutex::new(None::<Instant>);
        let mut report = stackrinse_core::scan(&settings.roots, &options, &cancelled, |progress| {
            let Ok(mut last) = last_event.lock() else {
                return;
            };
            let now = Instant::now();
            if last.is_none_or(|t| now.duration_since(t) >= Duration::from_millis(120)) {
                *last = Some(now);
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        visited: progress.directories,
                    },
                );
            }
        })
        .map_err(|e| e.to_string())?;
        {
            let mut slot = lock(&scan_slot)?;
            if let Some(active) = slot.as_mut() {
                active.accepting_cancel = false;
            }
            report.cancelled |= cancelled.load(Ordering::Relaxed);
        }
        let timing = ScanTiming {
            completed_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        let history_notice = if report.cancelled {
            None
        } else {
            Snapshot::capture(&report, timing)
                .and_then(|snapshot| history_store(&app)?.record(snapshot))
                .err()
                .map(|error| format!("This scan could not be saved to history. {error}"))
        };
        Ok::<_, String>((report, timing, history_notice))
    })
    .await
    .map_err(|e| e.to_string())??;
    let result = Arc::new(result);
    if !result.cancelled {
        *lock(&session.last_scan)? = Some(FreshScan {
            settings: scan_settings,
            report: result.clone(),
            timing,
        });
    }
    drop(lease);
    Ok(ScanOutcome {
        report: result,
        history_notice,
        completed_at_ms: timing.completed_at_ms,
    })
}

#[tauri::command]
fn cancel_scan(session: State<'_, Session>) -> Result<bool> {
    if let Some(active) = lock(&session.scan)?
        .as_ref()
        .filter(|scan| scan.accepting_cancel)
    {
        active.cancelled.store(true, Ordering::Relaxed);
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
async fn dry_run(app: AppHandle, session: State<'_, Session>, paths: Vec<PathBuf>) -> Result<Plan> {
    let current = get_settings(app)?;
    let scan = lock(&session.last_scan)?
        .clone()
        .ok_or("Scan a folder first.")?;
    selected_plan(&scan.settings, &current, &scan.report, paths)
}

fn history_store(app: &AppHandle) -> Result<HistoryStore> {
    app.path()
        .app_local_data_dir()
        .map(HistoryStore)
        .map_err(|error| error.to_string())
}

async fn history_call<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(HistoryStore) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || action(history_store(&app)?))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn history_list(app: AppHandle) -> Result<HistoryIndex> {
    history_call(app, |store| store.list()).await
}

#[tauri::command]
async fn history_detail(app: AppHandle, id: i64) -> Result<Detail> {
    history_call(app, move |store| store.detail(id)).await
}

#[tauri::command]
async fn history_compare(app: AppHandle, before_id: i64, after_id: i64) -> Result<Comparison> {
    history_call(app, move |store| store.compare(before_id, after_id)).await
}

#[tauri::command]
async fn history_remove(app: AppHandle, id: i64) -> Result<()> {
    history_call(app, move |store| store.remove(id)).await
}

#[tauri::command]
async fn history_clear(app: AppHandle) -> Result<()> {
    history_call(app, |store| store.clear()).await
}

#[tauri::command]
async fn history_set_retention(app: AppHandle, count: u32) -> Result<()> {
    history_call(app, move |store| store.set_retention(count)).await
}

#[tauri::command]
async fn export_inventory(
    app: AppHandle,
    session: State<'_, Session>,
    source: export::Source,
    format: export::Format,
) -> Result<bool> {
    let fresh = if let export::Source::Fresh = source {
        Some(
            lock(&session.last_scan)?
                .clone()
                .ok_or("Run a fresh scan to export its inventory.")?,
        )
    } else {
        None
    };
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = match source {
            export::Source::Fresh => {
                let scan = fresh.ok_or("Run a fresh scan to export its inventory.")?;
                Snapshot::capture(&scan.report, scan.timing)?
            }
            export::Source::History { id } => history_store(&app)?.get(id)?,
        };
        let extension = format.extension();
        let Some(selected) = app
            .dialog()
            .file()
            .set_title("Export inventory · May contain local paths")
            .set_file_name(format!("stackrinse-inventory.{extension}"))
            .add_filter(format!("{} report", extension.to_uppercase()), &[extension])
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = selected.into_path().map_err(|error| error.to_string())?;
        export::save(&path, format, source, &snapshot)?;
        Ok(true)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn selected_plan(
    previous: &Settings,
    current: &Settings,
    report: &Report,
    paths: Vec<PathBuf>,
) -> Result<Plan> {
    let selected: HashSet<_> = paths.into_iter().collect();
    if previous.roots != current.roots || previous.excluded_names != current.excluded_names {
        return Err("Scan again to use the updated locations and exclusions.".into());
    }
    Ok(stackrinse_core::dry_run_selected(report, |_, candidate| {
        selected.contains(&candidate.path)
            && !current
                .protected_projects
                .iter()
                .any(|p| candidate.path.starts_with(p) || p.starts_with(&candidate.path))
    }))
}

fn main() {
    tauri::Builder::default()
        .manage(Session::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            choose_root,
            remove_root,
            set_preferences,
            set_project_protection,
            scan,
            cancel_scan,
            dry_run,
            history_list,
            history_detail,
            history_compare,
            history_remove,
            history_clear,
            history_set_retention,
            export_inventory
        ])
        .run(tauri::generate_context!())
        .expect("StackRinse could not start its desktop window");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn dry_run_intersects_selection_with_inventory_and_current_protection() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let package = root.join("package");
        let artifact = package.join("__pycache__");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(root.join("pyproject.toml"), "[project]\nname='sample'").unwrap();
        fs::write(package.join("sample.py"), "pass").unwrap();
        fs::write(artifact.join("sample.cpython-311.pyc"), "bytecode").unwrap();
        let settings = Settings {
            roots: vec![root],
            ..Settings::default()
        };
        let report = stackrinse_core::scan(
            &settings.roots,
            &ScanOptions::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let plan = selected_plan(
            &settings,
            &settings,
            &report,
            vec![
                artifact.clone(),
                package.join("sample.py"),
                artifact.clone(),
            ],
        )
        .unwrap();
        assert_eq!(plan.candidates().len(), 1);
        assert_eq!(plan.candidates()[0].path, artifact);
        assert_eq!(plan.logical_bytes(), 8);
        let protected = Settings {
            protected_projects: vec![package],
            ..settings.clone()
        };
        assert!(
            selected_plan(&settings, &protected, &report, vec![artifact.clone()])
                .unwrap()
                .candidates()
                .is_empty()
        );
        let changed = Settings {
            excluded_names: vec!["cache".into()],
            ..settings.clone()
        };
        assert!(selected_plan(&settings, &changed, &report, vec![artifact]).is_err());
    }
}
