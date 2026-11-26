use stackrinse_core::{dry_run, scan, Classification, Report, ScanOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

struct Sample {
    label: &'static str,
    manifest: (&'static str, &'static str),
    artifact: &'static str,
    ecosystem: &'static str,
    category: &'static str,
    markers: &'static [(&'static str, &'static [u8])],
}

const DART: Sample = Sample {
    label: "dart",
    manifest: (
        "pubspec.yaml",
        "name: storage_sample\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
    ),
    artifact: ".dart_tool",
    ecosystem: "Dart",
    category: "Tooling metadata",
    markers: &[
        (
            "package_config.json",
            include_bytes!("fixtures/ecosystem-markers/package_config.json"),
        ),
        (
            "package_graph.json",
            include_bytes!("fixtures/ecosystem-markers/package_graph.json"),
        ),
    ],
};
const COMPOSER: Sample = Sample {
    label: "composer",
    manifest: ("composer.json", "{\"name\":\"stackrinse/storage-sample\"}"),
    artifact: "vendor",
    ecosystem: "PHP",
    category: "Dependencies",
    markers: &[
        (
            "autoload.php",
            include_bytes!("fixtures/ecosystem-markers/composer-autoload.txt"),
        ),
        (
            "composer/installed.json",
            include_bytes!("fixtures/ecosystem-markers/composer-installed.json"),
        ),
        (
            "composer/autoload_real.php",
            include_bytes!("fixtures/ecosystem-markers/composer-autoload-real.txt"),
        ),
    ],
};
const GRADLE: Sample = Sample {
    label: "gradle",
    manifest: ("build.gradle", "plugins { id 'java' }"),
    artifact: ".gradle",
    ecosystem: "Gradle",
    category: "Build caches",
    markers: &[
        (
            "buildOutputCleanup/cache.properties",
            include_bytes!("fixtures/ecosystem-markers/gradle-cache.properties"),
        ),
        (
            "buildOutputCleanup/outputFiles.bin",
            include_bytes!("fixtures/ecosystem-markers/gradle-outputFiles.bin"),
        ),
    ],
};
const RUFF: Sample = Sample {
    label: "ruff",
    manifest: ("pyproject.toml", "[project]\nname = 'storage_sample'"),
    artifact: ".ruff_cache",
    ecosystem: "Python",
    category: "Analysis caches",
    markers: &[
        (
            "CACHEDIR.TAG",
            include_bytes!("fixtures/ecosystem-markers/ruff-tag.txt"),
        ),
        (
            ".gitignore",
            include_bytes!("fixtures/ecosystem-markers/ruff-ignore.txt"),
        ),
    ],
};
const MYPY: Sample = Sample {
    label: "mypy",
    artifact: ".mypy_cache",
    markers: &[
        (
            "CACHEDIR.TAG",
            include_bytes!("fixtures/ecosystem-markers/mypy-tag.txt"),
        ),
        (
            ".gitignore",
            include_bytes!("fixtures/ecosystem-markers/mypy-ignore.txt"),
        ),
    ],
    ..RUFF
};
const SAMPLES: [Sample; 5] = [DART, COMPOSER, GRADLE, RUFF, MYPY];

fn root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap();
    (temp, path)
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn place_markers(project: &Path, sample: &Sample, except: Option<&str>) {
    for &(name, bytes) in sample.markers {
        if Some(name) != except {
            write(&project.join(sample.artifact).join(name), bytes);
        }
    }
}

fn place_project(project: &Path, sample: &Sample) {
    write(&project.join(sample.manifest.0), sample.manifest.1);
    place_markers(project, sample, None);
}

fn inventory(root: &Path, options: ScanOptions) -> Report {
    scan(&[root.into()], &options, &AtomicBool::new(false), |_| {}).unwrap()
}

#[test]
fn positive_markers_attribute_nested_workspace_artifacts_once() {
    let (_temp, root) = root();
    let mut measured = 0;
    for sample in &SAMPLES {
        let project = root.join("packages/例").join(sample.label);
        place_project(&project, sample);
        // A local edit can live beside valid generated output.
        write(
            &project.join(sample.artifact).join("手書き.txt"),
            "local edit",
        );
        measured += sample
            .markers
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum::<u64>()
            + "local edit".len() as u64;
    }
    let report = inventory(&root, ScanOptions::default());
    assert!(report.inventory_complete());
    assert_eq!(report.projects.len(), SAMPLES.len());
    assert_eq!(report.totals().logical_bytes, measured);
    for sample in &SAMPLES {
        let project = report
            .projects
            .iter()
            .find(|project| project.name == sample.label)
            .unwrap();
        assert_eq!(project.ecosystems, [sample.ecosystem]);
        assert_eq!(project.candidates.len(), 1);
        let candidate = &project.candidates[0];
        assert_eq!(candidate.path, project.path.join(sample.artifact));
        assert_eq!(candidate.ecosystem, sample.ecosystem);
        assert_eq!(candidate.category, sample.category);
        assert_eq!(candidate.classification, Classification::ReviewRequired);
        assert!(candidate
            .evidence
            .as_deref()
            .is_some_and(|value| !value.is_empty()));
        assert!(candidate.reason.contains("local changes"));
        assert!(!candidate.rebuild.is_empty());
    }
}

#[test]
fn every_new_rule_requires_its_manifest_and_each_regular_marker() {
    let (_temp, root) = root();
    for sample in &SAMPLES {
        let family = root.join(sample.label);
        place_markers(&family.join("contextless"), sample, None);
        let no_evidence = family.join("name-only");
        fs::create_dir_all(no_evidence.join(sample.artifact)).unwrap();
        write(&no_evidence.join(sample.manifest.0), sample.manifest.1);
        for (index, &(missing, _)) in sample.markers.iter().enumerate() {
            let incomplete = family.join(format!("missing-{index}"));
            write(&incomplete.join(sample.manifest.0), sample.manifest.1);
            place_markers(&incomplete, sample, Some(missing));

            let wrong_kind = family.join(format!("wrong-kind-{index}"));
            write(&wrong_kind.join(sample.manifest.0), sample.manifest.1);
            place_markers(&wrong_kind, sample, Some(missing));
            fs::create_dir_all(wrong_kind.join(sample.artifact).join(missing)).unwrap();
        }
    }
    let report = inventory(&root, ScanOptions::default());
    assert_eq!(report.projects.len(), 27);
    assert_eq!(report.totals().logical_bytes, 0);
    assert!(report
        .projects
        .iter()
        .all(|project| project.candidates.is_empty()));
}

#[test]
fn common_cache_tags_and_lookalike_creator_comments_are_insufficient() {
    let (_temp, root) = root();
    for sample in [&RUFF, &MYPY] {
        let foreign = root.join(sample.label).join("foreign");
        place_project(&foreign, sample);
        write(
            &foreign.join(sample.artifact).join(".gitignore"),
            "# Automatically created by ruff. and mypy\n*\n",
        );
        let malformed = root.join(sample.label).join("malformed");
        place_project(&malformed, sample);
        write(
            &malformed.join(sample.artifact).join("CACHEDIR.TAG"),
            "Signature: type-checker-cache\n",
        );
    }
    assert!(inventory(&root, ScanOptions::default())
        .projects
        .iter()
        .all(|p| p.candidates.is_empty()));
}

#[test]
fn shared_go_and_composer_vendor_is_protected_without_double_counting() {
    let (_temp, root) = root();
    place_project(&root, &COMPOSER);
    write(&root.join("go.mod"), "module example.org/demo\n");
    write(
        &root.join("vendor/modules.txt"),
        "# example.org/library v1.0.0\n",
    );
    let report = inventory(&root, ScanOptions::default());
    assert_eq!(report.projects[0].ecosystems, ["Go", "PHP"]);
    assert_eq!(report.projects[0].candidates.len(), 1);
    let candidate = &report.projects[0].candidates[0];
    assert_eq!(candidate.ecosystem, "Mixed evidence");
    assert_eq!(candidate.classification, Classification::Protected);
    assert!(candidate
        .evidence
        .as_deref()
        .unwrap()
        .contains("Go: modules.txt"));
    assert!(candidate
        .evidence
        .as_deref()
        .unwrap()
        .contains("PHP: autoload.php"));
    assert_eq!(report.totals().logical_bytes, candidate.logical_bytes);
    assert_eq!(report.totals().protected_bytes, candidate.logical_bytes);
    assert!(dry_run(&report).candidates().is_empty());
}

#[test]
fn every_new_rule_respects_git_index_project_protection_and_exclusions() {
    let (_temp, root) = root();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success());
    let mut preferences = ScanOptions::default();
    for sample in &SAMPLES {
        let project = root.join(sample.label);
        place_project(&project, sample);
        let path = project.join(sample.artifact).join("tracked.txt");
        write(&path, "locally tracked evidence");
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "--force", "--"])
            .arg(path.strip_prefix(&root).unwrap())
            .status()
            .unwrap()
            .success());
        preferences.protected_projects.push(project.clone());
        preferences.exclusions.push(project.join(sample.artifact));
    }
    let tracked = inventory(&root, ScanOptions::default());
    assert!(tracked.inventory_complete());
    assert_eq!(tracked.projects.len(), SAMPLES.len());
    for project in &tracked.projects {
        assert_eq!(project.candidates.len(), 1);
        assert_eq!(
            project.candidates[0].classification,
            Classification::Protected
        );
        assert!(project.candidates[0].reason.contains("Git index"));
    }
    assert!(dry_run(&tracked).candidates().is_empty());
    let excluded = inventory(&root, preferences.clone());
    assert!(excluded.projects.iter().all(|p| p.candidates.is_empty()));
    preferences.exclusions.clear();
    let protected = inventory(&root, preferences);
    assert!(protected.projects.iter().all(|p| p.protected));
    assert!(dry_run(&protected).candidates().is_empty());
}

#[test]
fn excluded_intermediate_directory_provides_no_marker_evidence() {
    let (_temp, root) = root();
    place_project(&root.join("php"), &COMPOSER);
    place_project(&root.join("gradle"), &GRADLE);
    let report = inventory(
        &root,
        ScanOptions {
            exclusions: vec!["composer".into(), "buildOutputCleanup".into()],
            ..Default::default()
        },
    );
    assert_eq!(report.projects.len(), 2);
    assert!(report
        .projects
        .iter()
        .all(|project| project.candidates.is_empty()));
}

#[cfg(unix)]
#[test]
fn linked_project_manifests_and_artifact_markers_provide_no_evidence() {
    use std::os::unix::fs::symlink;
    let (_temp, root) = root();
    for sample in &SAMPLES {
        let project = root.join(sample.label);
        place_markers(&project, sample, None);
        place_project(&project.join("marker-by-link"), sample);
        let external_manifest = root.join(format!("{}-manifest.txt", sample.label));
        write(&external_manifest, sample.manifest.1);
        symlink(external_manifest, project.join(sample.manifest.0)).unwrap();
        let marker = project
            .join("marker-by-link")
            .join(sample.artifact)
            .join(sample.markers[0].0);
        let external_file = root.join(format!("{}-marker.txt", sample.label));
        write(&external_file, sample.markers[0].1);
        fs::remove_file(&marker).unwrap();
        symlink(external_file, &marker).unwrap();
    }
    let result = inventory(&root, ScanOptions::default());
    assert_eq!(result.projects.len(), SAMPLES.len());
    assert!(result.projects.iter().all(|p| p.candidates.is_empty()));
}

#[test]
fn evidence_behind_a_linked_ancestor_is_refused() {
    let (_temp, root) = root();
    for (sample, nested) in [(&COMPOSER, "composer"), (&GRADLE, "buildOutputCleanup")] {
        let project = root.join(sample.label);
        place_project(&project, sample);
        let link = project.join(sample.artifact).join(nested);
        let outside = root.join(format!("{}-elsewhere", sample.label));
        fs::rename(&link, &outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        assert!(Command::new("cmd.exe")
            .current_dir(link.parent().unwrap())
            .args(["/d", "/c", "mklink", "/J", nested])
            .arg(Path::new(r"..\..").join(outside.file_name().unwrap()))
            .status()
            .unwrap()
            .success());
    }
    let report = inventory(&root, ScanOptions::default());
    assert!(report
        .projects
        .iter()
        .all(|project| project.candidates.is_empty()));
    assert!(dry_run(&report).candidates().is_empty());
}
