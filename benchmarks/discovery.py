"""Time read-only scans over a repeatable, freshly generated temporary tree."""
import argparse
import json
import platform
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
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    binaries = {"current": binary}
    if args.baseline:
        binaries["baseline"] = args.baseline.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="stackrinse-bench-") as temp:
        root = Path(temp).resolve(strict=True)
        payload = b"x" * 1024
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
            if len(report["projects"]) != args.projects or any(
                len(project["candidates"]) != 1 for project in report["projects"]
            ):
                raise RuntimeError(f"{label}: fixture inventory differs from scan results")
            reports[label] = report
        if args.baseline and reports["baseline"] != reports["current"]:
            raise RuntimeError("baseline and current inventories differ")

        order = list(binaries)
        for run in range(args.runs):
            for label in order if run % 2 == 0 else reversed(order):
                start = time.perf_counter()
                subprocess.run([binaries[label], "scan", root, "--quiet"], check=True)
                samples[label].append(time.perf_counter() - start)
        print(json.dumps({
            "platform": platform.platform(),
            "projects": args.projects,
            "generated_files_per_project": args.files,
            "source_dirs_per_project": args.source_dirs,
            "payload_bytes_per_file": len(payload),
            "directories_visited": reports["current"]["directories"],
            "logical_bytes": reports["current"]["totals"]["logical_bytes"],
            "results": {
                label: {
                    "stackrinse": subprocess.check_output([exe, "--version"], text=True).strip(),
                    "warm_scan_seconds": samples[label],
                    "median_seconds": statistics.median(samples[label]),
                } for label, exe in binaries.items()
            },
        }, indent=2))



if __name__ == "__main__":
    main()
