# Roadmap

## Discovery and review

The shared scanner, CLI and desktop workbench support read-only discovery and dry-run review. The desktop adds local scan history, compatible storage comparisons, and JSON and CSV reporting. CI compiles the native application and runs workspace tests on Linux, macOS and Windows.

The next milestone is hands-on Windows evaluation, followed by qualification of a packaged read-only preview. Exercise native pickers, keyboard and screen-reader navigation, high-DPI displays, large inventories and cancellation on slow volumes. Record platform, filesystem, fixture size and observed gaps for each run.

### Further ecosystem coverage

Android modules with Gradle manifests use the existing Gradle rule. Android-specific intermediate layouts need independent fixtures. Flutter shares Dart project metadata and needs SDK-specific evidence. Godot imported assets and Unity library caches need fixtures that distinguish generated imports from editor state and local changes. Global Go build caches and arbitrary CMake binary directories remain outside the project-local rules.

## Safe cleaning

Start with one platform and a narrowly defined artifact class. Prove candidate identity and authorization across the entire operation. Add switched-ancestor, link, tracked and untracked content, concurrent rename, locked-file, crash and partial-operation fixtures before exposing an executor.

Design recoverable cleanup with its own operation log and verified recovery instructions. Review permanent removal separately. Keep each unsupported mode unavailable on each platform until its safety tests pass.

## Release candidate

Measure traversal performance and memory on documented fixtures. Exercise packaged applications on Windows, macOS and Linux, including keyboard navigation, high-DPI and slow or inaccessible volumes. Validate dependency licenses, packaging, installation and upgrade behavior. Stable publication requires a maintainer decision.
