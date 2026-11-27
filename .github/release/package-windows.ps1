param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$RustNotices
)

$ErrorActionPreference = 'Stop'
$desktop = 'apps/stackrinse-desktop'
$target = 'x86_64-pc-windows-msvc'
$exe = "target/$target/release/stackrinse-desktop.exe"
$version = (Get-Content "$desktop/package.json" -Raw | ConvertFrom-Json).version
$tauri = Get-Content "$desktop/src-tauri/tauri.conf.json" -Raw | ConvertFrom-Json
$npmLock = Get-Content "$desktop/package-lock.json" -Raw | ConvertFrom-Json -AsHashtable
if ($tauri.version -cne $version -or $npmLock.version -cne $version -or
    $npmLock.packages[''].version -cne $version) {
    throw 'Desktop version metadata is inconsistent.'
}
$cargo = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or @($cargo.packages | Where-Object { $_.version -cne $version }).Count) {
    throw 'Cargo version metadata is inconsistent.'
}

$metadata = (Get-Item $exe).VersionInfo
if ($metadata.ProductName -cne 'StackRinse' -or $metadata.FileDescription -cne 'StackRinse' -or
    $metadata.CompanyName -cne 'KLOWZY' -or $metadata.ProductVersion -cne $version -or
    $metadata.FileVersion -cne $version) {
    throw "Unexpected executable metadata: $metadata"
}
if ((Get-AuthenticodeSignature $exe).Status -ne 'NotSigned') {
    throw 'Unexpected Authenticode status for the unsigned alpha.'
}

$commit = git rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $commit -cne $env:GITHUB_SHA) {
    throw 'The checked-out commit does not match the workflow run.'
}

$stage = Join-Path $env:RUNNER_TEMP 'stackrinse-stage'
$verify = Join-Path $env:RUNNER_TEMP 'stackrinse-unpacked'
New-Item -ItemType Directory -Path $stage, $OutputDirectory | Out-Null
Copy-Item $exe "$stage/StackRinse.exe"
Copy-Item LICENSE "$stage/LICENSE"
Copy-Item docs/releases/windows-launch.txt "$stage/START-HERE.txt"

$notices = [System.Collections.Generic.List[string]]::new()
$notices.Add((Get-Content $RustNotices -Raw))
$notices.Add("Bundled SQLite is in the public domain. https://www.sqlite.org/copyright.html`n")
foreach ($package in @('react', 'react-dom', 'scheduler', 'lucide-react', '@tauri-apps/api',
                       '@fontsource-variable/inter', '@fontsource/ibm-plex-mono', 'tailwindcss')) {
    $path = "$desktop/node_modules/$package"
    $manifest = Get-Content "$path/package.json" -Raw | ConvertFrom-Json
    $licenseFiles = @(Get-ChildItem $path -File -Filter 'LICENSE*')
    if ($licenseFiles.Count -eq 0) { throw "Missing license for $package" }
    $notices.Add("`n$package $($manifest.version) — $($manifest.license)`n")
    foreach ($license in $licenseFiles) {
        $notices.Add((Get-Content $license.FullName -Raw))
    }
}
$notices.Add((Get-Content .github/release/feather-license.txt -Raw))
$notices | Set-Content "$stage/THIRD-PARTY-NOTICES.txt"

$exeHash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
@(
    "StackRinse $version"
    "Source commit: $commit"
    "Target: $target (Windows x64)"
    "Authenticode: unsigned"
    "Executable SHA-256: $exeHash  StackRinse.exe"
) | Set-Content "$stage/BUILD.txt"

$zipName = "StackRinse-v$version-Windows-x64-Portable.zip"
$zip = Join-Path $OutputDirectory $zipName
Compress-Archive -Path "$stage/*" -DestinationPath $zip
Expand-Archive -LiteralPath $zip -DestinationPath $verify
$checks = foreach ($file in Get-ChildItem $stage -File) {
    $unpacked = Join-Path $verify $file.Name
    $originalHash = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
    $unpackedHash = (Get-FileHash $unpacked -Algorithm SHA256).Hash
    if ($originalHash -cne $unpackedHash) { throw "ZIP round-trip mismatch: $($file.Name)" }
    $file.Name
}
if ($checks.Count -ne @(Get-ChildItem $verify).Count) {
    throw 'Unexpected archive entries.'
}
$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
"$zipHash  $zipName" | Set-Content "$OutputDirectory/SHA256SUMS.txt" -Encoding ascii
@(
    "### Windows x64 release candidate"
    ""
    "- Source: $commit"
    "- Archive: $zipName ($((Get-Item $zip).Length) bytes)"
    "- Archive SHA-256: $zipHash"
    "- Executable SHA-256: $exeHash"
    "- Verified PE metadata, unsigned status and every extracted file's checksum."
) | Add-Content $env:GITHUB_STEP_SUMMARY
