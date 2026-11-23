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
    assert_eq!(plan.logical_bytes(), 24);
    assert_eq!(plan.candidates().len(), 8);
    assert_eq!(
        dry_run_selected(&report, |p, _| p.name == "hybrid").logical_bytes(),
        6
    );
}

#[test]
fn names_or_ignored_paths_alone_are_insufficient() {
    let (_t, r) = root();
    file(&r, "unrelated/target/.rustc_info.json", "1");
    file(&r, "crate/Cargo.toml", "");
    file(&r, "crate/.gitignore", "target/");
    fs::create_dir_all(r.join("crate/target/.rustc_info.json")).unwrap();
    file(&r, "crate/pom.xml", "");
    file(
        &r,
        "crate/target/classes",
        "a user file, not a classes directory",
    );
    file(&r, "crate/target/photos.txt", "important");
    let report = inventory(&r, ScanOptions::default());
    assert_eq!(report.projects.len(), 1);
    assert!(dry_run(&report).candidates().is_empty());
}

#[test]
fn mixed_bytecode_and_user_files_are_protected() {
    let (_t, r) = root();
    file(&r, "p/pyproject.toml", "");
    file(&r, "p/foo.py", "source");
    file(&r, "p/__pycache__/foo.cpython-312.pyc", "batch");
    let report = inventory(&r, ScanOptions::default());
    assert_eq!(
        dry_run(&report).candidates()[0].classification,
        Classification::Rebuildable
    );
    file(&r, "p/__pycache__/notes", "changes since scan");
    let next = inventory(&r, ScanOptions::default());
    assert_eq!(
        next.projects[0].candidates[0].classification,
        Classification::Protected
    );
    assert!(dry_run(&next).candidates().is_empty());
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
    assert!(report.projects[0].boundary_reason.is_none());
    assert!(dry_run(&report).candidates().is_empty());
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
    assert!(!report.projects[0].candidates[0].size_complete);
    assert!(report.warnings.is_empty());
    assert!(dry_run(&report).candidates().is_empty());
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
    let overlapping = inventory(
        &r,
        ScanOptions {
            protected_projects: vec![r.join("worktree")],
            ..Default::default()
        },
    );
    let worktree = overlapping
        .projects
        .iter()
        .find(|p| p.name == "worktree")
        .unwrap();
    assert!(worktree
        .boundary_reason
        .as_deref()
        .unwrap()
        .contains("worktree"));
    assert!(dry_run(&report).candidates().is_empty());
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
        dry_run(&inventory(&r, Default::default()))
            .candidates()
            .len(),
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
    assert!(dry_run(&report).candidates().is_empty());
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
    assert!(dry_run(&report).candidates().is_empty());
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
    assert!(dry_run(&report).candidates().is_empty());
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
    assert_eq!(plan.candidates().len(), 1);
    assert_eq!(plan.candidates()[0].path, r.join("src/__pycache__"));
}

#[cfg(windows)]
#[test]
fn junction_inside_candidate_protects_it_and_skips_the_target() {
    let (_temp, dir) = root();
    file(&dir, "app/Cargo.toml", "");
    file(&dir, "app/target/.rustc_info.json", "x");
    file(&dir, "outside/private.txt", "must never count");
    // Use only fixed relative arguments. The fixture can have a Unicode or spaced path.
    let junction = Command::new("cmd.exe")
        .current_dir(dir.join("app/target"))
        .args(["/d", "/c", "mklink", "/J", "linked", r"..\..\outside"])
        .output()
        .unwrap();
    assert!(
        junction.status.success(),
        "{}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let report = inventory(&dir.join("app"), Default::default());
    assert_eq!(report.projects[0].logical_bytes, 1);
    assert!(dry_run(&report).candidates().is_empty());
    assert!(scan(
        &[dir.join("app/target/linked")],
        &Default::default(),
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn inaccessible_artifact_subdirectory_blocks_review() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, dir) = root();
    file(&dir, "Cargo.toml", "");
    file(&dir, "target/.rustc_info.json", "x");
    file(&dir, "target/locked/private.txt", "private");
    let locked = dir.join("target/locked");
    let original = fs::metadata(&locked).unwrap().permissions();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let can_read_without_permissions = fs::read_dir(&locked).is_ok();
    let report = inventory(&dir, Default::default());
    fs::set_permissions(&locked, original).unwrap();
    if !can_read_without_permissions {
        assert!(!report.projects[0].candidates[0].size_complete);
        assert!(!report.warnings.is_empty());
        assert_eq!(
            report.projects[0].candidates[0].classification,
            Classification::Protected
        );
        assert!(dry_run(&report).candidates().is_empty());
    }
}

#[test]
fn go_and_cmake_require_context_and_separate_build_outputs() {
    let (_temp, root) = root();
    file(&root, "go.mod", "module example.org/demo");
    file(&root, "CMakeLists.txt", "project(demo)");
    file(&root, "vendor/modules.txt", "# example.org/lib v1.0.0");
    file(&root, "cmake-build-debug/CMakeCache.txt", "generated cache");
    file(&root, "cmake-build-debug/CMakeFiles/cmake.check_cache", "");
    // A nested source project configured in place has both markers.
    file(&root, "build/CMakeLists.txt", "project(child)");
    file(&root, "build/CMakeCache.txt", "must not be planned");
    file(&root, "build/CMakeFiles/cmake.check_cache", "");
    file(&root, "build/vendor/modules.txt", "no go.mod here");
    file(
        &root,
        "cmake-build-release/CMakeCache.txt",
        "no CMakeFiles directory",
    );
    let report = scan(
        &[root.clone(), root.join("build")],
        &Default::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.projects.len(), 2);
    assert_eq!(report.projects[0].ecosystems, ["CMake", "Go"]);
    let plan = dry_run(&report);
    assert_eq!(plan.candidates().len(), 2);
    assert!(plan
        .candidates()
        .iter()
        .all(|c| c.classification == Classification::ReviewRequired));
    assert_eq!(plan.logical_bytes(), report.totals().reviewable_bytes);
    assert!(plan
        .candidates()
        .iter()
        .any(|c| c.path == root.join("vendor")));
    assert!(plan
        .candidates()
        .iter()
        .any(|c| c.path == root.join("cmake-build-debug")));
}

#[test]
fn incomplete_and_cancelled_inventory_never_inflates_reviewable_size() {
    let (_temp, root) = root();
    file(&root, "Cargo.toml", "");
    file(&root, "target/.rustc_info.json", "123");
    file(&root, "go.mod", "");
    file(&root, "vendor/modules.txt", "12345");
    file(&root, "vendor/local/private", "excluded");
    let mut report = inventory(
        &root,
        ScanOptions {
            exclusions: vec!["local".into()],
            ..Default::default()
        },
    );
    let totals = report.totals();
    assert_eq!(
        (
            totals.logical_bytes,
            totals.reviewable_bytes,
            totals.protected_bytes
        ),
        (8, 3, 5)
    );
    let snapshot = dry_run(&report);
    report.cancelled = true;
    assert_eq!(report.totals().reviewable_bytes, 0);
    assert_eq!(report.totals().protected_bytes, 5);
    assert!(dry_run(&report).candidates().is_empty());
    assert_eq!(snapshot.logical_bytes(), 3);
}

#[test]
fn a_cancelled_deep_scan_can_be_repeated_without_stale_inventory() {
    let (_temp, root) = root();
    let mut project = root.clone();
    for _ in 0..32 {
        project.push("層abc");
    }
    file(&project, "go.mod", "module demo");
    file(&project, "vendor/modules.txt", "module metadata");
    let cancel = AtomicBool::new(false);
    let roots = std::slice::from_ref(&root);
    let report = scan(roots, &ScanOptions::default(), &cancel, |p| {
        if p.directories > 0 {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    })
    .unwrap();
    assert!(report.cancelled && dry_run(&report).candidates().is_empty());
    let complete = inventory(&root, Default::default());
    assert!(!complete.cancelled);
    assert_eq!(
        dry_run(&complete).candidates()[0].path,
        project.join("vendor")
    );
}

#[test]
fn shared_gradle_and_cmake_output_is_measured_once() {
    let (_temp, root) = root();
    file(&root, "CMakeLists.txt", "");
    file(&root, "build.gradle", "");
    file(&root, "build/CMakeCache.txt", "ab");
    file(&root, "build/CMakeFiles/cmake.check_cache", "");
    file(&root, "build/generated/main", "cde");
    let report = inventory(&root, Default::default());
    assert_eq!(report.projects[0].candidates.len(), 1);
    assert_eq!(report.totals().logical_bytes, 5);
    assert_eq!(dry_run(&report).logical_bytes(), 5);
}

#[cfg(unix)]
#[test]
fn linked_secondary_cmake_marker_is_insufficient_evidence() {
    let (_temp, root) = root();
    file(&root, "CMakeLists.txt", "");
    file(&root, "build/CMakeCache.txt", "cache");
    fs::create_dir(root.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join("build/CMakeFiles")).unwrap();
    assert!(dry_run(&inventory(&root, Default::default()))
        .candidates()
        .is_empty());
}

#[test]
fn godot_requires_project_and_both_editor_cache_markers() {
    let (_temp, root) = root();
    for project in [
        "game",
        "empty",
        "unrelated",
        "no-imports",
        "wrong-marker-type",
    ] {
        if project != "unrelated" {
            file(
                &root,
                &format!("{project}/project.godot"),
                "config_version=5",
            );
        }
        if project != "empty" {
            file(&root, &format!("{project}/.godot/.gdignore"), "");
        }
        if ["game", "unrelated"].contains(&project) {
            file(
                &root,
                &format!("{project}/.godot/imported/icon.ctex"),
                "texture",
            );
        }
    }
    file(&root, "wrong-marker-type/.godot/imported", "a regular file");
    file(&root, "game/icon.svg.import", "source import settings");
    let report = inventory(&root, Default::default());
    assert_eq!(report.projects.len(), 4);
    assert!(report.projects.iter().all(|p| p.ecosystems == ["Godot"]));
    let plan = dry_run(&report);
    let [cache] = plan.candidates() else {
        panic!("expected just the editor cache from the configured Godot project");
    };
    assert_eq!(cache.path, root.join("game/.godot"));
    assert_eq!(cache.classification, Classification::ReviewRequired);
    assert_eq!(cache.logical_bytes, 7);
    assert!(cache.reason.contains(".gdignore and imported/"));

    file(
        &root,
        "game/.godot/imported/vendor/.git/config",
        "protected",
    );
    let protected = inventory(&root, Default::default());
    assert!(dry_run(&protected).candidates().is_empty());
    assert_eq!(protected.totals().protected_bytes, 7);
}

#[cfg(unix)]
#[test]
fn godot_linked_evidence_does_not_create_a_candidate() {
    let (_temp, root) = root();
    file(&root, "game/project.godot", "config_version=5");
    file(&root, "game/.godot/.gdignore", "");
    file(&root, "outside/icon.ctex", "not part of the project");
    std::os::unix::fs::symlink(root.join("outside"), root.join("game/.godot/imported")).unwrap();
    let report = inventory(&root, Default::default());
    assert_eq!(report.projects.len(), 1);
    assert!(report.projects[0].candidates.is_empty());
}

#[cfg(unix)]
#[test]
fn unreadable_directory_leaves_sibling_inventory_available() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, root) = root();
    file(&root, "locked/hidden/package.json", "{}");
    file(&root, "readable/Cargo.toml", "");
    file(&root, "readable/target/.rustc_info.json", "{}");
    let locked = root.join("locked");
    let original = fs::metadata(&locked).unwrap().permissions();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let denied = fs::read_dir(&locked).is_err();
    let report = inventory(&root, Default::default());
    fs::set_permissions(&locked, original).unwrap();

    if denied {
        assert!(report.warnings.iter().any(|w| w.contains("locked")));
        assert_eq!(report.projects.len(), 1);
    }
    assert_eq!(dry_run(&report).logical_bytes(), 2);
}

// macOS APFS rejects filenames with invalid UTF-8 before the scanner sees them.
#[cfg(target_os = "linux")]
#[test]
fn non_unicode_directory_is_skipped_with_a_warning() {
    use std::os::unix::ffi::OsStrExt;

    let (_temp, root) = root();
    let unusual = root.join(std::ffi::OsStr::from_bytes(b"non-unicode-\xff"));
    file(&unusual, "go.mod", "");
    file(&root, "readable/Cargo.toml", "");
    let report = inventory(&root, Default::default());

    assert!(report.warnings.iter().any(|w| w.contains("non-Unicode")));
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].path, root.join("readable"));
}

#[test]
fn cancellation_from_final_progress_disables_review() {
    use std::sync::atomic::Ordering;

    let (_temp, root) = root();
    file(&root, "Cargo.toml", "");
    file(&root, "target/.rustc_info.json", "metadata");
    let cancel = AtomicBool::new(false);
    let report = scan(
        std::slice::from_ref(&root),
        &Default::default(),
        &cancel,
        |p| {
            if p.projects > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.totals().logical_bytes, 8);
    assert_eq!(report.totals().reviewable_bytes, 0);
    assert!(dry_run(&report).candidates().is_empty());
}

#[test]
fn root_removed_during_scan_preserves_other_roots() {
    let (_temp, root) = root();
    let first = root.join("a");
    let missing = root.join("b");
    let last = root.join("c");
    file(&first, "go.mod", "");
    fs::create_dir(&missing).unwrap();
    file(&last, "Cargo.toml", "");
    file(&last, "target/.rustc_info.json", "{}");

    let report = scan(
        &[first, missing.clone(), last],
        &Default::default(),
        &AtomicBool::new(false),
        |p| {
            if p.directories == 1 {
                fs::remove_dir(&missing).unwrap();
            }
        },
    )
    .unwrap();
    assert_eq!(report.projects.len(), 2);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains(&*missing.to_string_lossy()));
    assert_eq!(dry_run(&report).logical_bytes(), 2);
}

#[cfg(windows)]
#[test]
fn differently_cased_artifact_keeps_actual_path_without_nested_accounting() {
    let (_temp, temp_path) = root();
    let root = temp_path.join("LOCAL/project");
    file(&root, "Cargo.toml", "");
    file(&root, "TARGET/.rustc_info.json", "{}");
    file(&root, "TARGET/dependency/Cargo.toml", "manifest");
    file(&root, "TARGET/dependency/target/.rustc_info.json", "{}");

    let report = inventory(&root, Default::default());
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].candidates[0].path, root.join("TARGET"));
    assert_eq!(report.totals().logical_bytes, 12);

    let excluded = inventory(
        &root,
        ScanOptions {
            exclusions: vec!["target".into(), "local".into()],
            ..Default::default()
        },
    );
    assert_eq!(excluded.projects.len(), 1);
    assert!(excluded.projects[0].candidates.is_empty());
    assert_eq!(excluded.totals().logical_bytes, 0);
}

#[test]
fn basename_exclusions_start_at_the_selected_root() {
    let (_tmp, root) = root();
    let workspace = root.join("local/workspace");
    file(&workspace, "Cargo.toml", "");
    file(&workspace, "local/nested/Cargo.toml", "");
    let report = inventory(
        &workspace,
        ScanOptions {
            exclusions: vec!["local".into()],
            ..Default::default()
        },
    );
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].path, workspace);
}

#[test]
fn manifest_context_requires_a_regular_entry() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink as link;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as link;
    let (_tmp, dir) = root();
    file(&dir, "real/package.json", "{}");
    for name in ["linked", "dangling", "directory"] {
        file(
            &dir,
            &format!("{name}/node_modules/.package-lock.json"),
            "{}",
        );
    }
    link(
        dir.join("real/package.json"),
        dir.join("linked/package.json"),
    )
    .unwrap();
    link(dir.join("missing"), dir.join("dangling/package.json")).unwrap();
    fs::create_dir(dir.join("directory/package.json")).unwrap();
    #[cfg(windows)]
    assert!(Command::new("cmd.exe")
        .current_dir(dir.join("directory"))
        .args(["/d", "/c", "mklink", "/J", "Cargo.toml", r"..\real"])
        .status()
        .unwrap()
        .success());
    let report = inventory(&dir, Default::default());
    assert!(report.warnings.is_empty());
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].path, dir.join("real"));
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn case_only_project_rename_preserves_index_protection() {
    let (_tmp, dir) = root();
    file(&dir, "project/Cargo.toml", "");
    file(&dir, "project/target/.rustc_info.json", "{}");
    for args in [vec!["init", "-q"], vec!["add", "-f", "project"]] {
        assert!(Command::new("git")
            .current_dir(&dir)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    fs::rename(dir.join("project"), dir.join("staging")).unwrap();
    fs::rename(dir.join("staging"), dir.join("PROJECT")).unwrap();
    let report = inventory(&dir, Default::default());
    assert!(report.projects[0].candidates[0].reason.contains("tracked"));
    assert!(dry_run(&report).candidates().is_empty());
}

#[cfg(windows)]
#[test]
fn mixed_case_nested_bytecode_keeps_its_project_owner() {
    let (_tmp, dir) = root();
    file(&dir, "pyproject.toml", "");
    file(&dir, "pkg/unit.py", "pass");
    file(&dir, "pkg/__PYCACHE__/unit.cpython-312.pyc", "abc");
    let report = inventory(&dir, Default::default());
    assert_eq!(dry_run(&report).logical_bytes(), 3);
}
