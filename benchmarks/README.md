# Discovery benchmark

Build the optimized CLI, then use Python 3 to generate and measure a temporary workspace.

```sh
cargo build --release -p stackrinse-cli
python3 benchmarks/discovery.py target/release/stackrinse
```

On Windows, use `py benchmarks/discovery.py target/release/stackrinse.exe`. Set `--projects`, `--files` and `--runs` to vary the shape or sample count. The default tree contains 240 Node.js projects with 50 generated 1 KiB files each plus package metadata.

The script checks project discovery, then measures complete warm-cache CLI processes with normal output suppressed. JSON output records each elapsed time in seconds, the median, fixture parameters, platform and CLI version. Fixture creation and validation are outside the timed region. Use the same host, filesystem, build profile and parameters for comparisons. The script removes its own temporary tree when it finishes.
