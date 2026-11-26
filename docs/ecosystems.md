# Ecosystem coverage and evidence

## Discovery before Sprint 5

The scanner already recognized the following project-local layouts.

| Ecosystem | Project context | Previously recognized artifacts |
| --- | --- | --- |
| Node.js | `package.json` | `node_modules` with npm, pnpm or Yarn node-modules markers, plus marked Next.js and Nuxt build output |
| Python | `pyproject.toml`, `setup.py`, `setup.cfg`, or `requirements.txt` | Marked virtual environments, pytest cache and CPython bytecode paired with nearby source |
| Go | `go.mod` | `vendor` with `modules.txt` |
| Rust | `Cargo.toml` | `target` with compiler metadata or a cache tag |
| CMake | `CMakeLists.txt` | Conventional build directories with `CMakeCache.txt` and `CMakeFiles/` |
| Godot | `project.godot` | `.godot` with `.gdignore` and `imported/` |
| .NET | C#, F# or VB project file | Marked `obj` and `bin` output |
| Maven | `pom.xml` | Marked `target` output |
| Gradle | `build.gradle` or `build.gradle.kts` | `build` with classes or generated outputs |

The scanner works from regular project manifests and tool evidence. It reads only selected roots and skips filesystem links, reparse points, exclusions, special files and protected system paths. On Unix it also checks volume boundaries. The Git index, repository boundaries, protection preferences and incomplete measurements can block dry-run review.

## Sprint 5 additions

Every rule below starts at **Review required**. File lengths include local edits observed in a matched directory. Repository and filesystem protection take precedence.

| Artifact | Required project context and evidence | Regeneration and scope |
| --- | --- | --- |
| Dart Pub `.dart_tool` | Regular `pubspec.yaml`. Both `package_config.json` and `package_graph.json` must be regular files. | Run `dart pub get` or `flutter pub get` with the original SDK and dependencies. Pub metadata is reported under Dart. Review other state stored in the same directory. |
| Composer `vendor` | Regular `composer.json`. Requires `vendor/autoload.php`, `vendor/composer/installed.json` and `vendor/composer/autoload_real.php`. | Run `composer install` with the original lockfile, PHP extensions and package sources. Preserve local dependency changes. The rule covers the default vendor location. |
| Gradle `.gradle` | Regular `build.gradle` or `build.gradle.kts`. Requires `buildOutputCleanup/cache.properties` and `buildOutputCleanup/outputFiles.bin`. | Rerun the matching tasks with the original wrapper, JDK and dependencies. Stop active builds before inspecting the cache. The rule covers project-local state from a build that wrote its output index. |
| Ruff `.ruff_cache` | A supported Python manifest, a standard `CACHEDIR.TAG` signature and Ruff's generated `.gitignore` header. | Run Ruff with the original version and configuration. Review any local additions. |
| mypy `.mypy_cache` | A supported Python manifest, a standard `CACHEDIR.TAG` signature and mypy's generated `.gitignore` header. | Run mypy with the original Python environment, plugins and configuration. The rule uses the cache tags across internal storage layouts. |

Dart Pub, Composer, Gradle, Ruff and mypy generated the evidence used in the tests. The scanner checks each component of a nested marker path and reads only a bounded first line for Python cache signatures. If separate rules match a single directory, such as Go and PHP sharing `vendor`, the scanner measures it once, attributes its bytes to **Mixed evidence**, and protects it from review planning. See [fixture provenance](../crates/stackrinse-core/tests/fixtures/ecosystem-markers/README.md) and the [mixed benchmark](../benchmarks/README.md).

## Investigated and deferred

| Area | Scope and evidence gap |
| --- | --- |
| Flutter-specific output | Pub metadata qualifies through the Dart rule. Flutter build, plugin and platform output need fixtures from a full Flutter SDK and rules that distinguish generated platform files from editable application sources. [Flutter CLI](https://docs.flutter.dev/reference/flutter-cli) |
| Android-specific output | Gradle output and local caches can qualify through the Gradle rules. SDK-specific caches and Android module outputs need fixtures across Android Gradle Plugin versions. Packaged APKs and AABs can be important deliverables. [Android build structure](https://developer.android.com/build/android-build-structure) |
| Unity | The Asset Database distinguishes source assets from imported state. Reliable `Library` coverage needs editor-generated fixtures and evidence for import databases across versions. [Unity Asset Database](https://docs.unity.com/en-us/engine/6000.6/manual/assets-and-media/asset-database) |
| Unreal Engine | `Saved` includes local state and potentially valuable work. Intermediate and derived data need project-local fixtures with reliable ownership evidence. [Unreal directory structure](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-directory-structure) |
| Swift and Xcode | SwiftPM build output needs native fixtures covering workspace state and custom build paths. Xcode DerivedData can live outside the project, while archives can be release artifacts. [SwiftPM](https://www.swift.org/getting-started/cli-swiftpm/) · [Xcode distribution](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases) |
| Ruby / Bundler | Bundler install paths are configurable and can be shared. A future rule needs current project-local bundle evidence across Ruby versions and configurations. [Bundler configuration](https://bundler.io/man/bundle-config.1.html) |
| Bun and Yarn PnP | The tested isolated Bun install produced links and no stable install marker suitable for this rule set. A lockfile records dependency resolution, while Yarn PnP archives can be intentionally checked in for Zero-Installs. [Bun isolated installs](https://bun.com/docs/pm/isolated-installs) · [Yarn cache](https://yarnpkg.com/features/caching) |
| Other build systems | In-place Ninja builds can share source and output directories. Dedicated Meson or other rules need validated generated-tree evidence and clear scope ownership before expanding generic build-directory matches. |

## Recoverable cleanup

Start with a separately reviewed authorization design for one platform and one narrowly defined artifact class. Use fresh evidence, stable file and volume identities, and operation-time checks of ancestors, repository state, mixed or locally modified content, and exclusions. Record intent before any mutation and verify recovery through interruption and restart. Exercise replacement races, locked files, links, partial failures, and recovery on disposable volumes. Keep logical inventory and confirmed physical recovery as separate measurements.
