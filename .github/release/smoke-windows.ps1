$ErrorActionPreference = 'Stop'
if (-not [Environment]::UserInteractive -or (Get-Process -Id $PID).SessionId -eq 0) {
    'Windows launch smoke skipped because the runner has no interactive desktop session.' |
        Add-Content $env:GITHUB_STEP_SUMMARY
    Write-Warning 'No interactive Windows desktop session is available.'
    exit 0
}

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port"
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:RUNNER_TEMP 'stackrinse-smoke-webview'
$exe = Join-Path $env:RUNNER_TEMP 'stackrinse-unpacked/StackRinse.exe'
$app = Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) -PassThru
try {
    node .github/release/check-webview.mjs $port
    if ($LASTEXITCODE -ne 0) { throw 'Packaged WebView2 smoke test failed.' }
    $app.Refresh()
    if ($app.HasExited -or $app.MainWindowHandle -eq [IntPtr]::Zero) {
        throw 'The packaged process did not create a native app window.'
    }
    'Automated Windows launch passed. The extracted EXE opened a window, rendered the embedded Overview and answered settings IPC. Hands-on interaction remains pending.' |
        Add-Content $env:GITHUB_STEP_SUMMARY
} finally {
    if (-not $app.HasExited) {
        if (-not $app.CloseMainWindow() -or -not $app.WaitForExit(5000)) {
            Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
        }
    }
}
