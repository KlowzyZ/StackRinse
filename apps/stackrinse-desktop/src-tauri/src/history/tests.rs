use super::*;
use stackrinse_core::{scan, ScanOptions};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Barrier},
    time::Instant,
};

fn timing(completed_at_ms: i64) -> ScanTiming {
    ScanTiming {
        completed_at_ms,
        duration_ms: 420,
    }
}

fn captured(root: &Path, options: &ScanOptions, time: i64) -> Snapshot {
    let report = scan(&[root.into()], options, &AtomicBool::new(false), |_| {}).unwrap();
    Snapshot::capture(&report, timing(time)).unwrap()
}

fn project(root: &Path, name: &str, bytes: usize) -> PathBuf {
    let directory = root.join(name);
    let artifact = directory.join("target");
    fs::create_dir_all(&artifact).unwrap();
    fs::write(directory.join("Cargo.toml"), "[package]\nname = 'example'").unwrap();
    fs::write(artifact.join(".rustc_info.json"), b"{}").unwrap();
    fs::write(artifact.join("compiled"), vec![1; bytes]).unwrap();
    directory
}

fn empty_snapshot(time: i64) -> Snapshot {
    Snapshot::capture(&Report::default(), timing(time)).unwrap()
}

#[test]
fn initializes_schema_transactionally_and_preserves_future_versions() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().into());
    fs::write(store.file(), b"").unwrap(); // Interrupted creation before the migration.
    let db = store.connect().unwrap();
    let pragma = |name| {
        db.pragma_query_value(None, name, |r| r.get::<_, i64>(0))
            .unwrap()
    };
    assert_eq!(pragma("user_version"), SCHEMA_VERSION);
    assert_eq!(pragma("auto_vacuum"), 1);
    assert_eq!(pragma("secure_delete"), 1);
    assert_eq!(pragma("max_page_count"), (MAX_DATABASE_BYTES / 4096) as i64);
    assert_eq!(store.list().unwrap().retention, DEFAULT_RETENTION);
    db.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    drop(db);
    let original = fs::read(store.file()).unwrap();
    assert!(store.clear().unwrap_err().contains("schema is unsupported"));
    assert_eq!(fs::read(store.file()).unwrap(), original);
}

#[test]
fn unreadable_database_and_malformed_records_are_preserved_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().into());
    let corrupt = b"truncated or invalid database";
    fs::write(store.file(), corrupt).unwrap();
    assert!(store
        .list()
        .err()
        .unwrap()
        .contains("Scan history is unavailable"));
    assert!(store.record(empty_snapshot(1)).is_err());
    assert_eq!(fs::read(store.file()).unwrap(), corrupt);

    fs::remove_file(store.file()).unwrap();
    store.record(empty_snapshot(1)).unwrap();
    let db = store.connect().unwrap();
    db.execute("UPDATE scans SET inventory = x'7b' WHERE id = 1", [])
        .unwrap();
    assert_eq!(store.list().unwrap().entries.len(), 1);
    assert!(store.get(1).err().unwrap().contains("invalid record"));
    store.remove(1).unwrap();
    assert!(store.list().unwrap().entries.is_empty());
}

#[cfg(unix)]
#[test]
fn database_is_private_and_existing_links_are_refused() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().join("app"));
    store.record(empty_snapshot(1)).unwrap();
    let path = store.file();
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
    let original = dir.path().join("original");
    fs::rename(&path, &original).unwrap();
    symlink(&original, &path).unwrap();
    assert!(store.clear().is_err());
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
}

#[test]
fn restart_ordering_retention_and_removal_use_persistent_ids() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().into());
    store.set_retention(2).unwrap();
    store.record(empty_snapshot(3000)).unwrap();
    store.record(empty_snapshot(1000)).unwrap(); // Another instance completes its write later.
    store.record(empty_snapshot(2000)).unwrap();
    drop(store);

    let store = HistoryStore(dir.path().into());
    let index = store.list().unwrap();
    assert_eq!(index.retention, 2);
    assert_eq!(
        index
            .entries
            .iter()
            .map(|entry| (entry.id, entry.summary.completed_at_ms))
            .collect::<Vec<_>>(),
        [(1, 3000), (3, 2000)]
    );
    assert_eq!(store.get(1).unwrap().summary.duration_ms, 420);
    assert!(store.compare(1, 3).is_err());
    assert_eq!(store.compare(3, 1).unwrap().logical_bytes_delta, 0);
    store.remove(1).unwrap();
    assert!(store.get(1).is_err());
    assert_eq!(store.list().unwrap().entries[0].id, 3);
    store.clear().unwrap();
    store.record(empty_snapshot(4000)).unwrap();
    assert_eq!(store.list().unwrap().entries[0].id, 4);
    assert_eq!(store.list().unwrap().retention, 2);
    assert!(store.set_retention(0).is_err());
}

#[test]
fn concurrent_creation_and_records_do_not_lose_updates() {
    let dir = tempfile::tempdir().unwrap();
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for i in 0..4 {
            let store = HistoryStore(dir.path().into());
            let barrier = &barrier;
            workers.push(scope.spawn(move || {
                barrier.wait();
                store.record(empty_snapshot(i)).unwrap();
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let store = HistoryStore(dir.path().into());
    assert_eq!(store.list().unwrap().entries.len(), 4);
}

#[test]
fn compares_real_inventory_changes_by_path_and_artifact_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let same = project(&root, "café-engine", 12);
    let gone = project(&root, "old-build", 50);
    fs::write(same.join("package.json"), "{}").unwrap(); // Same project, multiple ecosystems.
    let before = captured(&root, &ScanOptions::default(), 1);
    assert_eq!(before.inventory.ecosystems.get("Rust"), Some(&66));
    assert!(!before.inventory.ecosystems.contains_key("Node.js"));
    let node = same.join("node_modules");
    fs::create_dir(&node).unwrap();
    fs::write(node.join(".package-lock.json"), b"{}").unwrap();
    fs::write(same.join("target/compiled"), [0; 17]).unwrap();
    fs::remove_dir_all(&gone).unwrap();
    let new = project(&root, "new-build", 7);
    let after = captured(&root, &ScanOptions::default(), 2);
    let result = compare(&before, &after).unwrap();
    assert_eq!(result.logical_bytes_delta, -36);
    assert_eq!(result.growth.unwrap().projects[0].path, same);
    assert_eq!(result.discovered.unwrap().projects[0].path, new);
    assert_eq!(result.no_longer_observed.unwrap().projects[0].path, gone);
    assert!(result
        .ecosystems
        .iter()
        .any(|bucket| bucket.name == "Node.js" && bucket.before == 0 && bucket.after == 2));
    assert!(result
        .categories
        .iter()
        .any(|bucket| bucket.name == "Build outputs" && bucket.before == 66 && bucket.after == 28));
}

#[test]
fn rejects_changed_folders_exclusions_protection_and_partial_inventories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let project = project(&root, "project", 10);
    let before = captured(&root, &ScanOptions::default(), 1);
    let other = captured(&project, &ScanOptions::default(), 2);
    assert!(compare(&before, &other)
        .err()
        .unwrap()
        .contains("different folders"));
    let protected = captured(
        &root,
        &ScanOptions {
            protected_projects: vec![project.clone()],
            ..Default::default()
        },
        2,
    );
    assert!(compare(&before, &protected).is_err());
    let excluded = captured(
        &root,
        &ScanOptions {
            exclusions: vec!["ignored".into()],
            ..Default::default()
        },
        2,
    );
    assert!(compare(&before, &excluded).is_err());
    let redundant_roots = scan(
        &[project, root.clone(), root.clone()],
        &ScanOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(redundant_roots.scope.roots, std::slice::from_ref(&root));
    let mut partial = redundant_roots.clone();
    partial.projects[0].candidates[0].size_complete = false;
    let saved = Snapshot::capture(&partial, timing(2)).unwrap();
    assert!(!saved.summary.complete);
    assert_eq!(saved.summary.incomplete_candidate_count, 1);
    assert!(compare(&before, &saved)
        .err()
        .unwrap()
        .contains("complete inventories"));
    partial.projects[0].candidates[0].size_complete = true;
    partial.warnings.push("Unreadable directory".into());
    assert!(
        !Snapshot::capture(&partial, timing(2))
            .unwrap()
            .summary
            .complete
    );
    partial.cancelled = true;
    assert!(Snapshot::capture(&partial, timing(2)).is_err());
    let opts = |exclusions| ScanOptions {
        exclusions,
        ..Default::default()
    };
    let first = captured(&root, &opts(vec!["temp".into(), "cache".into()]), 1);
    let second = captured(
        &root,
        &opts(vec!["cache".into(), "temp".into(), "cache".into()]),
        2,
    );
    assert!(compare(&first, &second).is_ok());
}

#[test]
fn large_histories_enforce_both_limits_and_keep_summary_reads_small() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().into());
    store.set_retention(MAX_RETENTION).unwrap();
    for time in 0..(i64::from(MAX_RETENTION) + 3) {
        store.record(empty_snapshot(time)).unwrap();
    }
    assert_eq!(store.list().unwrap().entries.len(), MAX_RETENTION as usize);

    let started = Instant::now();
    for time in 1000..1011 {
        let mut snapshot = empty_snapshot(time);
        snapshot
            .inventory
            .warnings
            .push("w".repeat(3 * 1024 * 1024));
        store.record(snapshot).unwrap();
    }
    let recording = started.elapsed();
    let started = Instant::now();
    let index = store.list().unwrap();
    let list_time = started.elapsed();
    let size = fs::metadata(store.file()).unwrap().len();
    assert_eq!(index.entries.len(), 7);
    assert!(size <= MAX_DATABASE_BYTES);
    let db = store.connect().unwrap();
    let payload: i64 = db
        .query_row(
            "SELECT sum(length(summary) + length(inventory)) FROM scans",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(payload <= MAX_PAYLOAD_BYTES as i64);
    assert!(
        size < MAX_PAYLOAD_BYTES as u64 + 1024 * 1024,
        "expected auto-vacuum to reclaim unused database pages"
    );
    println!("11 x 3 MiB writes: {recording:?}; retained {} rows, {size} database bytes; list: {list_time:?}", index.entries.len());
}

#[test]
fn oversized_project_details_keep_totals_and_explain_unavailable_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore(dir.path().into());
    let mut snapshot = empty_snapshot(1);
    snapshot.inventory.projects = Some(vec![Project {
        path: PathBuf::from("/").join("x".repeat(MAX_INVENTORY_BYTES)),
        ecosystems: vec!["Rust".into()],
        candidate_count: 1,
        logical_bytes: 1,
    }]);
    snapshot.summary.totals.logical_bytes = 1;
    store.record(snapshot).unwrap();
    store.record(empty_snapshot(2)).unwrap();
    let detail = store.detail(1).unwrap();
    assert!(!detail.project_details_available);
    assert!(detail.largest_projects.is_empty());
    assert_eq!(detail.entry.summary.totals.logical_bytes, 1);
    let comparison = store.compare(1, 2).unwrap();
    assert!(comparison.growth.is_none());
    assert_eq!(comparison.logical_bytes_delta, -1);
}

#[test]
fn exports_unicode_json_and_safe_csv_without_overwriting() {
    use export::{Format, Source};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let project_path = project(&root, "東京-café", 30);
    let mut snapshot = captured(&root, &ScanOptions::default(), 1000);
    snapshot
        .inventory
        .warnings
        .push(" \u{feff}=HYPERLINK(\"example\")\r\n,ñ".into());
    let json = root.join("inventário.json");
    let csv = root.join("inventário.csv");
    export::save(&json, Format::Json, Source::History { id: 5 }, &snapshot).unwrap();
    export::save(&csv, Format::Csv, Source::Fresh, &snapshot).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&json).unwrap()).unwrap();
    assert_eq!(value["format_version"], 1);
    assert_eq!(value["source"]["kind"], "history");
    assert_eq!(value["summary"]["totals"]["logical_bytes"], 32);
    assert_eq!(
        value["inventory"]["projects"][0]["path"],
        project_path.to_str().unwrap()
    );
    assert!(value["summary"].get("scope_key").is_none());
    let content = fs::read_to_string(&csv).unwrap();
    assert!(content.starts_with('\u{feff}'));
    assert!(content.contains("東京-café"));
    assert!(content.contains("\"' \u{feff}=HYPERLINK(\"\"example\"\")\r\n,ñ\","));
    assert!(content.contains("\"fresh_scan\""));
    assert!(export::save(&csv, Format::Csv, Source::Fresh, &snapshot)
        .unwrap_err()
        .contains("already exists"));
    assert_eq!(fs::read_to_string(&csv).unwrap(), content);
    assert!(export::save(
        &root.join("incorrect.txt"),
        Format::Csv,
        Source::Fresh,
        &snapshot
    )
    .is_err());
    let count = fs::read_dir(&root).unwrap().count();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        snapshot
            .inventory
            .roots
            .push(PathBuf::from(std::ffi::OsString::from_vec(vec![0xff])));
        let invalid = root.join("invalid.json");
        assert!(export::save(&invalid, Format::Json, Source::Fresh, &snapshot).is_err());
        assert!(!invalid.exists());
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), count);
}
