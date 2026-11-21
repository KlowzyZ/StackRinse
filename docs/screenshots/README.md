# Interface screenshots

The browser interaction suite builds four temporary projects on disk, runs the StackRinse CLI against them, and passes its JSON report to the React application through a test-only desktop transport. The projects represent synthetic development workloads. Sizes shown in the interface are measured logical bytes. The suite exercises the rendered interface independently of native dialogs and platform webviews.

To capture the current interface, install the Rust and desktop dependencies from the main README, then run these commands from `apps/stackrinse-desktop`.

```sh
npx playwright install chromium
STACKRINSE_SCREENSHOTS="$PWD/../../docs/screenshots" npm run test:e2e
```

Set `CHROMIUM_PATH` to use an existing Chromium executable. The folder suffix and scan times vary by run.
