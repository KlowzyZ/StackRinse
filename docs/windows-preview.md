# Private Windows x64 preview

StackRinse currently builds a desktop executable named `stackrinse-desktop.exe`. The Tauri product name and window title are StackRinse. The Cargo crate, npm package and Tauri version are all 0.1.0. The Tauri bundle already references a multi-size Windows ICO. The app identifier is `dev.klowzy.stackrinse`. Preserve it across preview updates so Windows keeps resolving the same settings and history directories.

The release build with the Tauri custom protocol embeds the Vite production assets and suppresses the console window. SQLite is bundled. The build machine needs Node, Rust and the MSVC C++ toolchain. The preview machine needs an x64 Windows environment and a functioning Microsoft Edge WebView2 Evergreen Runtime. The archive carries the app only. Settings and scan history use the OS per-user folders, typically `%APPDATA%\dev.klowzy.stackrinse\settings.json` and `%LOCALAPPDATA%\dev.klowzy.stackrinse\scan-history.sqlite3`. Extracting the app to a different folder still uses those locations. The preview is unsigned until a signing and distribution process is approved.

## Next-milestone build workflow

Add the following manual workflow during private preview packaging. Run it from an approved, verified main revision. The source repository is public. GitHub Actions artifact access follows repository permissions, so upload only the encrypted archive. Set `WINDOWS_PREVIEW_ARCHIVE_PASSWORD` to a high-entropy secret before running it and convey the password separately to the maintainer. Run the workflow only from trusted code with review over the workflow and its dependencies.

```yaml
name: Private Windows x64 preview
on:
  workflow_dispatch:
permissions:
  contents: read
jobs:
  preview:
    if: github.ref == 'refs/heads/main'
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: npm
          cache-dependency-path: apps/stackrinse-desktop/package-lock.json
      - name: Build frontend
        working-directory: apps/stackrinse-desktop
        run: |
          npm ci
          npm run build
      - name: Build x64 desktop executable
        run: |
          rustup update stable --no-self-update
          rustup target add --toolchain stable x86_64-pc-windows-msvc
          cargo +stable build --locked --release -p stackrinse-desktop --target x86_64-pc-windows-msvc --features custom-protocol
      - name: Stage and encrypt preview
        shell: pwsh
        env:
          PREVIEW_PASSWORD: ${{ secrets.WINDOWS_PREVIEW_ARCHIVE_PASSWORD }}
        run: |
          $ErrorActionPreference = 'Stop'
          if (-not $env:PREVIEW_PASSWORD) { throw 'Configure the preview archive secret.' }
          New-Item -ItemType Directory -Path preview | Out-Null
          Copy-Item target/x86_64-pc-windows-msvc/release/stackrinse-desktop.exe preview/StackRinse.exe
          Copy-Item LICENSE,docs/windows-preview.md preview/
          $hash = (Get-FileHash preview/StackRinse.exe -Algorithm SHA256).Hash.ToLowerInvariant()
          @("Commit $env:GITHUB_SHA", "$hash  StackRinse.exe") | Set-Content preview/BUILD.txt
          & "$env:ProgramFiles\7-Zip\7z.exe" a -t7z -mhe=on "-p$env:PREVIEW_PASSWORD" StackRinse-Windows-x64.7z ./preview/*
          if ($LASTEXITCODE -ne 0) { throw 'Creating the encrypted archive failed.' }
      - uses: actions/upload-artifact@v4
        with:
          name: StackRinse-Windows-x64-encrypted
          path: StackRinse-Windows-x64.7z
          if-no-files-found: error
          retention-days: 7
```

## Maintainer validation

Download the workflow artifact, extract the outer ZIP and open the encrypted 7z archive with 7-Zip. Verify the executable checksum against `BUILD.txt` and attest the commit against the approved run. On a clean x64 Windows account or VM, confirm WebView2 Evergreen is installed and launch `StackRinse.exe` without developer tools or a dev server. Microsoft provides the [Evergreen Runtime installer](https://developer.microsoft.com/microsoft-edge/webview2/). If SmartScreen flags the unsigned preview, follow the organization's trust policy for the verified binary.

Exercise the folder picker and Save dialog with synthetic workspaces, light and dark themes, 100% and higher display scaling, keyboard navigation, cancellation, exports, and history persistence after restart. Test a host without WebView2 to document that launch dependency. Recording outcomes on a native Windows machine is a preview acceptance step.

Future Windows installer evaluation can use Tauri NSIS or MSI with an explicit WebView2 install policy. Compare Evergreen bootstrapper versus offline installer behavior and signing before distributing any installer. The executable archive is suitable for testing on hosts with WebView2 available. A self-contained distribution for offline hosts would need separately qualified runtime packaging. [Tauri Windows distribution options](https://v2.tauri.app/distribute/windows-installer/) · [WebView2 distribution](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution)
