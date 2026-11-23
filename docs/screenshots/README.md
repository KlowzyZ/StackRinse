# Interface screenshots

The browser interaction suite builds four temporary projects on disk, runs the StackRinse CLI against them, and passes its JSON report to the React application through a test-only desktop transport. The projects represent synthetic development workloads. Sizes shown in the interface are measured logical bytes. The suite exercises the rendered interface independently of native dialogs and platform webviews.

To capture the current interface, install the Rust and desktop dependencies from the main README, then run these commands from `apps/stackrinse-desktop`.

```sh
npx playwright install chromium
STACKRINSE_SCREENSHOTS="$PWD/../../docs/screenshots" npm run test:e2e
```

Set `CHROMIUM_PATH` to use an existing Chromium executable. The folder suffix and scan times vary by run.

[Linux native overview](overview-linux-native.png) shows the Tauri WebKitGTK window during a scan of three disposable projects. The native smoke test used WebKitWebDriver to run the scan from persisted fixture roots, open a project and preview a selected dependency directory through Rust IPC.

[Repository protection](repository-protection.png) shows a disposable project with a Git worktree boundary and a skipped directory inside its build output. The observed byte count and protection reason come from the shared scanner.

The Linux Actions job also drives the embedded application through WebKitWebDriver under Xvfb. It reads a disposable Cargo project through native IPC, checks the two-byte inventory and selected dry-run result, and uploads the measured overview as a short-lived workflow artifact.
