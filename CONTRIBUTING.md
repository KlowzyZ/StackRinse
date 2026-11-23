# Contributing to StackRinse

Start with the [development instructions](README.md) and [safety model](docs/safety.md). For a new ecosystem rule, describe the manifest evidence, artifact evidence, plausible user-owned content and rebuild consequences in an issue before proposing broad matches.

Use a topic branch. Keep filesystem policy in the shared Rust core so both entry points produce consistent results. Add positive and negative fixtures inside temporary directories, including a plausible source directory with the same name as the proposed artifact. Keep tests independent of your home directory, network and installed project toolchains.

For a core or CLI change, start with `cargo test --locked`. Full workspace checks require the desktop prerequisites and built frontend assets listed in the README.

Run `cargo fmt --all -- --check`, `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. For desktop changes, run the frontend build and interaction tests. Include a redacted screenshot when the interface changes. In your pull request, list the platforms and paths you exercised and identify any that still need testing.

Contributions are distributed under the MIT license.
