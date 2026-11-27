$ErrorActionPreference = 'Stop'
if (-not [Environment]::UserInteractive -or (Get-Process -Id $PID).SessionId -eq 0) {
    'Windows launch smoke skipped because the runner has no interactive desktop session.' |
        Add-Content $env:GITHUB_STEP_SUMMARY
    Write-Warning 'No interactive Windows desktop session is available.'
    exit 0
}

function Get-WebView2Version {
    $client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($root in @('HKLM:\SOFTWARE\WOW6432Node', 'HKCU:\SOFTWARE')) {
        $key = "$root\Microsoft\EdgeUpdate\Clients\$client"
        $version = Get-ItemPropertyValue -LiteralPath $key -Name pv -ErrorAction SilentlyContinue
        if ($version -and [version]$version -gt [version]'0.0.0.0') { return $version }
    }
}

$version = Get-WebView2Version
if (-not $version) {
    $installer = Join-Path $env:RUNNER_TEMP 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
    Invoke-WebRequest 'https://go.microsoft.com/fwlink/p/?LinkId=2124701' -OutFile $installer
    $signature = Get-AuthenticodeSignature $installer
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=Microsoft Corporation(?:,|$)') {
        throw 'WebView2 Evergreen installer signature did not validate as Microsoft Corporation.'
    }
    $setup = Start-Process -FilePath $installer -ArgumentList '/silent', '/install' -Wait -PassThru
    $version = Get-WebView2Version
    if (-not $version) { throw "WebView2 installation did not register a runtime (exit $($setup.ExitCode))." }
}
Write-Host "WebView2 Evergreen $version is installed."

$webViewProfile = Join-Path $env:RUNNER_TEMP 'stackrinse-smoke-webview'
$env:WEBVIEW2_USER_DATA_FOLDER = $webViewProfile
$exe = Join-Path $env:RUNNER_TEMP 'stackrinse-unpacked/StackRinse.exe'
$app = Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $app.Refresh()
        if ($app.HasExited) { throw "The packaged process exited during startup (exit $($app.ExitCode))." }
        if ($app.MainWindowHandle -ne [IntPtr]::Zero -and $app.MainWindowTitle -eq 'StackRinse' -and $app.Responding) {
            $webView = Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
                Where-Object { $_.CommandLine -and $_.CommandLine.Contains($webViewProfile, [StringComparison]::OrdinalIgnoreCase) } |
                Select-Object -First 1
            if ($webView) {
                $message = "Automated Windows smoke passed. The extracted executable opened a responsive StackRinse window and initialized WebView2 Evergreen $version in its isolated profile."
                Write-Host $message
                $message | Add-Content $env:GITHUB_STEP_SUMMARY
                return
            }
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for the StackRinse window and packaged app WebView2 process (last window title: '$($app.MainWindowTitle)')."
} finally {
    if (-not $app.HasExited) {
        if (-not $app.CloseMainWindow() -or -not $app.WaitForExit(5000)) {
            Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
        }
    }
}
