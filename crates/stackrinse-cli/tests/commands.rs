use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_stackrinse"))
}

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("sample app");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join("package.json"),
        r#"{"name":"sample","version":"1.0.0"}"#,
    )
    .unwrap();
    fs::create_dir(project.join("node_modules")).unwrap();
    fs::write(
        project.join("node_modules/.package-lock.json"),
        r#"{"lockfileVersion":3}"#,
    )
    .unwrap();
    root
}

fn run(cmd: &str, root: &Path, flags: &[&str]) -> Output {
    command()
        .arg(cmd)
        .arg(root.canonicalize().unwrap())
        .args(flags)
        .output()
        .unwrap()
}

#[test]
fn inventory_and_size_filter_use_real_projects() {
    let fixture = fixture();
    let output = run("scan", fixture.path(), &["--json"]);
    assert!(output.status.success(), "{:?}", output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["projects"][0]["name"], "sample app");
    assert_eq!(
        report["totals"]["reviewable_bytes"],
        report["projects"][0]["logical_bytes"]
    );
    let filtered = run("scan", fixture.path(), &["--json", "--min-size", "1TB"]);
    let filtered: Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert_eq!(filtered["totals"]["logical_bytes"], 0);
    assert_eq!(filtered["projects"], serde_json::json!([]));
}

#[test]
fn inspect_and_preview_preserve_original_files() {
    let fixture = fixture();
    let inspect = run("inspect", fixture.path(), &[]);
    assert!(inspect.status.success());
    assert!(String::from_utf8(inspect.stdout)
        .unwrap()
        .contains("sample app"));
    for flags in [&["--json"][..], &["--dry-run", "--json"]] {
        let preview = run("clean", fixture.path(), flags);
        assert!(preview.status.success());
        let plan: Value = serde_json::from_slice(&preview.stdout).unwrap();
        assert_eq!(plan["mode"], "dry_run");
        assert!(plan["plan"]["candidates"].is_array());
    }
    assert_eq!(
        fs::read_to_string(
            fixture
                .path()
                .join("sample app/node_modules/.package-lock.json")
        )
        .unwrap(),
        r#"{"lockfileVersion":3}"#
    );
}

#[test]
fn quiet_and_invalid_arguments() {
    let fixture = fixture();
    let quiet = run("scan", fixture.path(), &["--quiet"]);
    assert!(quiet.status.success() && quiet.stdout.is_empty());
    assert_eq!(
        command().arg("scan").output().unwrap().status.code(),
        Some(2)
    );
    assert_eq!(
        run("clean", fixture.path(), &["--apply"]).status.code(),
        Some(2)
    );
    assert_eq!(
        run("scan", fixture.path(), &["--min-size", "lots"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn relative_protection_and_exclusion_resolve_from_current_directory() {
    let fixture = fixture();
    let output = command()
        .current_dir(fixture.path())
        .args(["clean", ".", "--json", "--protect", "sample app"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["plan"]["candidates"], serde_json::json!([]));
    let excluded = command()
        .current_dir(fixture.path())
        .args(["scan", ".", "--json", "--exclude", "sample app"])
        .output()
        .unwrap();
    assert!(excluded.status.success(), "{excluded:?}");
    let report: Value = serde_json::from_slice(&excluded.stdout).unwrap();
    assert_eq!(report["projects"], serde_json::json!([]));
}

#[test]
fn recovery_is_unavailable_at_production_entry_points() {
    let root = fixture();
    for verb in ["quarantine", "restore", "purge"] {
        assert_eq!(run(verb, root.path(), &[]).status.code(), Some(2));
    }
    let host = include_str!("../../../apps/stackrinse-desktop/src-tauri/src/main.rs");
    let (_, registration) = host.split_once("tauri::generate_handler![").unwrap();
    let names: Vec<_> = registration
        .split_once(']')
        .unwrap()
        .0
        .split(',')
        .map(str::trim)
        .collect();
    assert_eq!(
        names,
        [
            "get_settings",
            "choose_root",
            "remove_root",
            "set_preferences",
            "set_project_protection",
            "scan",
            "cancel_scan",
            "dry_run",
            "history_list",
            "history_detail",
            "history_compare",
            "history_remove",
            "history_clear",
            "history_set_retention",
            "export_inventory"
        ]
    );
}
