"""Time read-only scans over a repeatable, freshly generated temporary tree."""
import argparse
import json
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def positive(value):
    value = int(value)
    if value < 1:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return value


MIXED_ARTIFACTS = {
    "dart/.dart_tool",
    "composer/vendor",
    "gradle/.gradle",
    "gradle/build",
    "python/.ruff_cache",
    "python/.mypy_cache",
}


def mixed_workspace(root, fixtures, copies, source_dirs):
    for i in range(copies):
        packages = root / f"workspace-{i:06}" / "packages"
        packages.mkdir(parents=True)
        for name in ["dart", "composer", "gradle", "python", "bun"]:
            shutil.copytree(fixtures / name, packages / name, symlinks=True)
        for j in range(source_dirs):
            (packages / "src" / f"feature-{j:03}").mkdir(parents=True)
        # The real generated cache next door offers a useful decoy comparison.
        decoy = packages / "decoy"
        (decoy / ".dart_tool").mkdir(parents=True)
        (decoy / "pubspec.yaml").write_text("name: decoy\n")
        (decoy / ".dart_tool" / "package_config.json").write_text("{}\n")


def candidate_paths(report, root):
    return {
        Path(candidate["path"]).resolve(strict=True).relative_to(root).as_posix(): candidate
        for project in report["projects"]
        for candidate in project["candidates"]
    }


def without_rule_provenance(report):
    # Sprint 4 JSON predates these additive fields. Retain all inventory values.
    report = dict(report)
    report.pop("discovery_rule_version", None)
    report["projects"] = [dict(project, candidates=[
        {key: value for key, value in candidate.items() if key not in ("ecosystem", "evidence")}
        for candidate in project["candidates"]
    ]) for project in report["projects"]]
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="compiled stackrinse executable")
    parser.add_argument("--baseline", type=Path, help="optional baseline executable")
    parser.add_argument("--source-dirs", type=int, choices=range(0, 101), default=0,
                        metavar="0-100", help="non-project source directories per project")
    parser.add_argument("--projects", type=positive, default=240)
    parser.add_argument("--files", type=positive, default=50,
                        help="generated files per project")
    parser.add_argument("--runs", type=positive, default=5)
    parser.add_argument("--mixed-fixtures", type=Path,
                        help="directory containing tool-generated dart, composer, gradle, python and bun fixtures")
    parser.add_argument("--copies", type=positive, default=32,
                        help="number of nested workspaces in mixed mode")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    binaries = {"current": binary}
    if args.baseline:
        binaries["baseline"] = args.baseline.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="stackrinse-bench-") as temp:
        root = Path(temp).resolve(strict=True)
        payload = b"x" * 1024
        if args.mixed_fixtures:
            mixed_workspace(root, args.mixed_fixtures, args.copies, args.source_dirs)
        else:
            for i in range(args.projects):
                project = root / f"project-{i:06}"
                package = project / "node_modules" / "sample"
                package.mkdir(parents=True)
                (project / "package.json").write_text('{"name":"app","version":"1.0.0"}')
                for j in range(args.source_dirs):
                    source = project / "src" / f"feature-{j:03}"
                    source.mkdir(parents=True)
                    (source / "index.js").write_text("export const sample = true;")
                (package.parent / ".package-lock.json").write_text('{"lockfileVersion":3}')
                (package / "package.json").write_text('{"name":"sample","version":"1.0.0"}')
                for j in range(args.files):
                    (package / f"file-{j:05}.js").write_bytes(payload)

        # Validate before timing and warm the same fixture for both binaries.
        reports = {}
        samples = {label: [] for label in binaries}
        for label, executable in binaries.items():
            report = json.loads(subprocess.check_output([executable, "scan", root, "--json"]))
            if args.mixed_fixtures:
                paths = candidate_paths(report, root)
                expected = {"gradle/build"} if report.get("discovery_rule_version", 1) == 1 else MIXED_ARTIFACTS
                expected_paths = {f"workspace-{i:06}/packages/{path}"
                                  for i in range(args.copies) for path in expected}
                if paths.keys() != expected_paths:
                    raise RuntimeError(f"{label}: missing {expected_paths - paths.keys()}, unexpected {paths.keys() - expected_paths}")
                if not all(item["size_complete"] and item["classification"] == "review_required" for item in paths.values()):
                    raise RuntimeError(f"{label}: a generated fixture was not fully measured for review")
            elif len(report["projects"]) != args.projects or any(
                len(project["candidates"]) != 1 for project in report["projects"]
            ):
                raise RuntimeError(f"{label}: fixture inventory differs from scan results")
            reports[label] = report
        if args.baseline and not args.mixed_fixtures:
            baseline, current = reports["baseline"], reports["current"]
            if baseline.get("discovery_rule_version", 1) != current.get("discovery_rule_version", 1):
                baseline, current = without_rule_provenance(baseline), without_rule_provenance(current)
            if baseline != current:
                raise RuntimeError("baseline and current inventories differ")

        order = list(binaries)
        for run in range(args.runs):
            for label in order if run % 2 == 0 else reversed(order):
                start = time.perf_counter()
                subprocess.run([binaries[label], "scan", root, "--quiet"], check=True)
                samples[label].append(time.perf_counter() - start)
        print(json.dumps({
            "platform": platform.platform(),
            "fixture": "mixed" if args.mixed_fixtures else "node",
            "projects": len(reports["current"]["projects"]),
            **({"workspace_copies": args.copies, "source_dirs_per_workspace": args.source_dirs}
               if args.mixed_fixtures else {
                   "generated_files_per_project": args.files,
                   "source_dirs_per_project": args.source_dirs,
                   "payload_bytes_per_file": len(payload),
               }),
            "directories_visited": reports["current"]["directories"],
            "logical_bytes": reports["current"]["totals"]["logical_bytes"],
            "results": {
                label: {
                    "stackrinse": subprocess.check_output([exe, "--version"], text=True).strip(),
                    "candidate_count": sum(len(project["candidates"]) for project in reports[label]["projects"]),
                    "directories_visited": reports[label]["directories"],
                    "logical_bytes": reports[label]["totals"]["logical_bytes"],
                    "warm_scan_seconds": samples[label],
                    "median_seconds": statistics.median(samples[label]),
                } for label, exe in binaries.items()
            },
        }, indent=2))



if __name__ == "__main__":
    main()
