# Discovery benchmark

Build the optimized CLI, then use Python 3 to generate and measure a temporary workspace.

```sh
cargo build --release -p stackrinse-cli
python3 benchmarks/discovery.py target/release/stackrinse
```

On Windows, use `py benchmarks/discovery.py target/release/stackrinse.exe`. Set `--projects`, `--files` and `--runs` to vary the shape or sample count. The default tree contains 240 Node.js projects with 50 generated 1 KiB files each plus package metadata.

The script checks project discovery, then measures complete warm-cache CLI processes with normal output suppressed. JSON output records each elapsed time in seconds, the median, fixture parameters, platform and CLI version. Fixture creation and validation are outside the timed region. Use the same host, filesystem, build profile and parameters for comparisons. The script removes its own temporary tree when it finishes.

To compare two revisions, build both with the same Rust toolchain and save the baseline binary outside the build directory. The comparison verifies equal JSON inventories on the same fixture, then alternates which binary runs first in each measured pair. For source-heavy discovery, add `--source-dirs 40` to place ordinary source folders alongside generated outputs.

```sh
python3 benchmarks/discovery.py target/release/stackrinse --baseline /path/to/baseline --runs 15
python3 benchmarks/discovery.py target/release/stackrinse --baseline /path/to/baseline --projects 360 --files 20 --source-dirs 40 --runs 15
```

## Mixed ecosystems

The mixed workload copies actual Dart Pub, Composer, Gradle, Ruff, mypy and Bun outputs into nested workspaces. It adds ordinary source directories and an incomplete Dart lookalike. Bun is a negative fixture. The script checks the observed paths, classification and completeness before timing. Sprint 4 and Sprint 5 intentionally discover different inventories on this workload.

Generate the disposable outputs in a development environment with the fixture tools, then benchmark both release binaries against that same tree. The scanner tests use small committed excerpts and run without these additional toolchains.

```sh
python3 benchmarks/generate_tool_fixtures.py /tmp/stackrinse-tool-fixtures
python3 benchmarks/discovery.py target/release/stackrinse \
  --baseline /path/to/sprint-4/stackrinse \
  --mixed-fixtures /tmp/stackrinse-tool-fixtures/fixtures \
  --copies 64 --source-dirs 24 --runs 21
```

Sprint 5 used Dart 3.14.1, Composer 2.10.3, Gradle 9.3.1 with JDK 21, Ruff 0.17.6, mypy 2.4.1 and Bun 1.4.5. Tool versions can change which files they produce. The generator creates a new directory, uses local or empty dependency graphs, and saves tool state outside the scanned fixtures.
