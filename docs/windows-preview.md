# Windows x64 release candidate

The Windows portable ZIP bundles `StackRinse.exe`, launch instructions, `LICENSE`, third-party notices and `BUILD.txt`. The executable uses Tauri's custom protocol to serve the embedded production frontend and links bundled SQLite. The app identifier remains `dev.klowzy.stackrinse`. Settings typically live in `%APPDATA%\dev.klowzy.stackrinse\settings.json` and scan history in `%LOCALAPPDATA%\dev.klowzy.stackrinse\scan-history.sqlite3`.

## Build and deliver privately

Run the [Windows x64 release candidate workflow](https://github.com/KlowzyZ/StackRinse/actions/workflows/windows-release.yml) from a reviewed main commit with passing cross-platform checks. The workflow builds the production frontend, compiles the locked Windows MSVC release and checks the executable, ZIP and dependency notices. It attempts a packaged launch and checks for a responsive native window and an app-owned WebView2 process where a Windows desktop session is available. The runner installs Microsoft WebView2 Evergreen if needed. The run summary records compilation, verification and launch results. The encrypted candidate is retained if a hosted launch check fails, so the maintainer can investigate the same bytes.

The public repository's Actions upload contains only a CMS-encrypted delivery archive. Provide a Base64 DER public RSA certificate (3072 bits or larger) as the workflow input. Keep the private key outside GitHub. This example uses OpenSSL on the trusted machine that decrypts the delivery.

```sh
umask 077
openssl req -x509 -newkey rsa:3072 -nodes -days 30 -subj '/CN=StackRinse RC' \
  -addext 'keyUsage=keyEncipherment' -keyout rc-key.pem -out rc-cert.pem
openssl x509 -in rc-cert.pem -outform DER | openssl base64 -A
```

Download and extract the Actions artifact. On the trusted machine, decrypt the enclosed CMS file and extract the delivery ZIP.

```sh
openssl cms -decrypt -binary -inform DER -in StackRinse-Windows-x64.cms \
  -recip rc-cert.pem -inkey rc-key.pem -out delivery.zip
unzip delivery.zip
sha256sum -c SHA256SUMS.txt
```

Keep the verified portable ZIP and checksum manifest together. Put the same two files on the unpublished GitHub release draft or use a private transfer to the maintainer. Preserve those bytes through publication.

## Windows maintainer acceptance

Use Windows 10 or 11 x64 with [WebView2 Evergreen](https://developer.microsoft.com/microsoft-edge/webview2/). A standard account or disposable VM without developer tools is suitable.

- Compare the ZIP hash with `SHA256SUMS.txt`. Extract it and compare `Get-FileHash .\StackRinse.exe -Algorithm SHA256` with `BUILD.txt`. Verify the source commit matches the candidate.
- Open `StackRinse.exe`, confirm the icon, window and About version, and record any SmartScreen or runtime prompts. Follow local policy for unsigned apps.
- Use the native folder picker to scan a synthetic workspace with generated artifacts. Inspect evidence, protect a project, exercise scan cancellation, and verify dry-run review leaves the workspace contents intact.
- Use the native Save dialog to export and inspect JSON and CSV. Run two comparable scans, restart, and check settings, history and comparison.
- Check keyboard navigation, light and dark themes, and 100% and higher Windows display scaling. Record the OS build, WebView2 version and any failures against the executable checksum.

After the maintainer explicitly approves publication, verify the draft's tag target and uploaded checksums. Publish the draft as a pre-release with the accepted files.
