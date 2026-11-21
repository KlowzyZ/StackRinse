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
    parser.add_argument("--projects", type=positive, default=240)
    parser.add_argument("--files", type=positive, default=50,
                        help="generated files per project")
    parser.add_argument("--runs", type=positive, default=5)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="stackrinse-bench-") as temp:
        root = Path(temp).resolve(strict=True)
        payload = b"x" * 1024
        for i in range(args.projects):
            project = root / f"project-{i:06}"
            package = project / "node_modules" / "sample"
            package.mkdir(parents=True)
            (project / "package.json").write_text('{"name":"app","version":"1.0.0"}')
            (package.parent / ".package-lock.json").write_text('{"lockfileVersion":3}')
            (package / "package.json").write_text('{"name":"sample","version":"1.0.0"}')
            for j in range(args.files):
                (package / f"file-{j:05}.js").write_bytes(payload)

        # Check inventory outside the timed section; this also warms the filesystem cache.
        report = json.loads(subprocess.check_output([binary, "scan", root, "--json"]))
        if len(report["projects"]) != args.projects:
            raise RuntimeError("fixture project count differs from scan results")
        elapsed = []
        for _ in range(args.runs):
            start = time.perf_counter()
            subprocess.run([binary, "scan", root, "--quiet"], check=True)
            elapsed.append(time.perf_counter() - start)
        print(json.dumps({
            "platform": platform.platform(),
            "stackrinse": subprocess.check_output([binary, "--version"], text=True).strip(),
            "projects": args.projects,
            "generated_files_per_project": args.files,
            "payload_bytes_per_file": len(payload),
            "warm_scan_seconds": elapsed,
            "median_seconds": statistics.median(elapsed),
        }, indent=2))


if __name__ == "__main__":
    main()
