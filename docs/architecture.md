# Architecture

The Cargo workspace contains a headless discovery library, a terminal client and the Tauri host. Cargo's default members are the core and CLI so terminal development can run independently of WebKit or WebView2. The desktop renderer is a strict TypeScript and React Vite project.

| Component | Responsibility |
| --- | --- |
| `crates/stackrinse-core` | Project and artifact evidence, traversal policy, classifications, logical size inventory and a dry-run plan |
| `crates/stackrinse-cli` | Explicit roots, cancellation and terminal or JSON presentation |
| `apps/stackrinse-desktop/src-tauri` | Native folder selection, settings persistence, scan lifetime and renderer IPC |
| `apps/stackrinse-desktop/src` | Interactive storage map, project navigation, candidate inspection, review and preferences |
| `apps/stackrinse-desktop/e2e` | Browser interaction checks using temporary inventories |
| `benchmarks` | Reproducible, disposable workspaces for traversal measurements |

## Data flow

A scan receives explicit roots, exclusions and protected project paths. Manifest discovery identifies a project and its ecosystems. Rules associate an ecosystem with candidate directory names, additional local evidence, expected rebuild consequences and qualitative cost. The engine reports absolute paths, classification explanations, file lengths, manifest modification times and warnings.

The CLI formats the report directly. The native desktop host runs traversal outside the UI thread and forwards throttled progress events. The renderer uses the report for the category ring, sorting and detailed inspection. Only the native side touches the filesystem. Selecting a candidate for review does not grant a capability to delete it.

## Local state

The desktop uses Tauri's per-user application configuration directory for `dev.klowzy.stackrinse`. Its settings JSON contains absolute paths, exclusions, protected project paths and appearance preferences. It is sensitive local metadata. Updates validate the configuration and replace it via a sibling temporary file under a lock. OS user permissions and a trustworthy user-owned configuration directory are assumed. Scan results live in memory. CLI JSON is written to stdout and can contain full paths.

## Extending discovery

Add an ecosystem marker and evidence rules in the core. Include a valid fixture and lookalikes with absent evidence, tracked content, links, ambiguous files and nested projects. Decide how to report partial traversal. Each consumer needs the same explanation and rebuild implications. Keep executable project commands outside discovery.
