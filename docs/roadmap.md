# Roadmap

## Discovery and review

The shared scanner, CLI and desktop workbench support read-only discovery and dry-run review. The desktop adds local scan history, compatible storage comparisons, and JSON and CSV reporting. CI compiles the native application and runs workspace tests on Linux, macOS and Windows.

The next milestone is a private Windows x64 preview for maintainer evaluation. Build the standalone desktop executable with embedded assets, then test it on a clean Windows account with WebView2. Exercise native pickers, keyboard and screen-reader navigation, high-DPI displays, large inventories and cancellation on slow volumes. Follow the [private preview workflow](windows-preview.md).

### Further ecosystem coverage

Android modules can use the Gradle build and project-local cache rules. Flutter packages can use Pub metadata discovery. Android-specific and Flutter-specific intermediates need their own native tool fixtures. Unity, Unreal, SwiftPM, Xcode and Bundler require project-local evidence and tests that distinguish generated state from sources and deliverables. The [ecosystem audit](ecosystems.md) records the evidence gaps.

## Safe cleaning

Start with one platform and a narrowly defined artifact class. Prove candidate identity and authorization across the entire operation. Add switched-ancestor, link, tracked and untracked content, concurrent rename, locked-file, crash and partial-operation fixtures before exposing an executor.

Design recoverable cleanup with its own operation log and verified recovery instructions. Review permanent removal separately. Keep each unsupported mode unavailable on each platform until its safety tests pass.

## Release candidate

Measure traversal performance and memory on documented fixtures. Exercise packaged applications on Windows, macOS and Linux, including keyboard navigation, high-DPI and slow or inaccessible volumes. Validate dependency licenses, packaging, installation and upgrade behavior. Stable publication requires a maintainer decision.
