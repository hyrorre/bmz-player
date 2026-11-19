# Exercises production Setup and its cleanup helper with isolated fixtures.
param([string]$Iscc = "iscc.exe", [string]$UpdaterPath)
$ErrorActionPreference = "Stop"
if (!(Get-Command $Iscc -ErrorAction SilentlyContinue)) {
    $Iscc = @(${env:ProgramFiles(x86)}, $env:ProgramFiles) |
        Where-Object { $_ } |
        ForEach-Object { Join-Path $_ "Inno Setup 6\ISCC.exe" } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (!$Iscc) { throw "Install Inno Setup 6 or pass -Iscc <path>." }
}
$repo = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repo (".local\installer-test-" + [guid]::NewGuid().ToString("N"))
$source = Join-Path $testRoot "source"
$target = Join-Path $testRoot "installed 日本語"
$output = Join-Path $testRoot "output"
$testAppId = [guid]::NewGuid().ToString()
New-Item -ItemType Directory -Force $source, $output | Out-Null
if (!$UpdaterPath) {
    $helperBuild = Join-Path $testRoot 'helper-build'
    cargo build --manifest-path (Join-Path $repo 'Cargo.toml') --locked -p bmz-updater --target-dir $helperBuild
    if ($LASTEXITCODE -ne 0) { throw "Could not build the Setup cleanup helper" }
    $UpdaterPath = Join-Path $helperBuild 'debug\bmz-updater.exe'
}
$UpdaterPath = (Resolve-Path -LiteralPath $UpdaterPath).Path

function Write-Fixture([string]$Root, [string]$Relative, [string]$Content) {
    $path = Join-Path $Root $Relative
    New-Item -ItemType Directory -Force (Split-Path -Parent $path) | Out-Null
    Set-Content -LiteralPath $path -Value $Content -Encoding utf8NoBOM -NoNewline
}
function Assert-Content([string]$Relative, [string]$Expected) {
    $path = Join-Path $target $Relative
    if (!(Test-Path -LiteralPath $path -PathType Leaf) -or
        [IO.File]::ReadAllText($path) -cne $Expected) {
        throw "File changed or missing: $Relative"
    }
}
function Write-PackageManifest([string]$Version, [string]$Layout, [string]$DllLayout) {
    & node (Join-Path $repo 'scripts\generate-update-metadata.mjs') package `
        $source installer windows-x64 $Version $Layout $DllLayout
    if ($LASTEXITCODE -ne 0) { throw "Could not write installer package manifest" }
}
function Build-Setup([string]$Version, [switch]$Sxs) {
    $arguments = @('/DAppName=BMZ Installer Preservation Test', "/DAppId=$testAppId",
        "/DAppVersion=$Version", "/DSourceDir=$source", "/DOutputDir=$output",
        "/DIconFile=$(Join-Path $repo 'assets\app-icon\bmz-player.ico')")
    if ($Sxs) { $arguments += '/DSxsDllLayout=1' }
    & $Iscc @arguments (Join-Path $repo "installer\inno\bmz-player.iss")
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed: $LASTEXITCODE" }
    return Join-Path $output "bmz-player-$Version-windows-x64-setup.exe"
}
function Run-Installer([string]$Executable, [string]$Label) {
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART',
        "/DIR=`"$target`"", "/LOG=`"$(Join-Path $testRoot "$Label.log")`"")
    $process = Start-Process -FilePath $Executable -ArgumentList $arguments `
        -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$Label failed: $($process.ExitCode)" }
}

Write-Fixture $source "bmz-player.exe" "fixture only - never execute"
Write-Fixture $source "bmz-updater.exe" "updater fixture only"
Write-Fixture $source ".bmz-instance.lock" ""
Write-Fixture $source "vcruntime140.dll" "old VC runtime"
Write-Fixture $source "locked.dll" "old locked DLL"
Write-Fixture $source "modified.dll" "old packaged DLL"
Write-Fixture $source "resources\skins\default\play7.json" "old packaged skin"
Write-Fixture $source "resources\obsolete.txt" "old package only"
Write-PackageManifest '0.0.1' legacy flat
# Capture only the returned path; compiler output goes to the host.
$oldSetup = Build-Setup "0.0.1" | Select-Object -Last 1
Run-Installer $oldSetup "fresh"
Assert-Content "resources\skins\default\play7.json" "old packaged skin"

$preserved = @{
    "resources\skins\custom\日本語 skin.lua" = "custom skin"
    "resources\skins\default\custom.wav" = "added inside bundled skin"
    "resources\fonts\custom.ttf" = "custom font"
    "data\skins\custom\play7.json" = "user skin"
    "data\config.toml" = "user configuration"
    "data\profiles\default\score.db" = "score fixture"
    "unknown.txt" = "unknown root file"
    "unknown.dll" = "unknown root DLL"
    "updater\job-old\backup\0" = "recovery backup"
    "updater\update.lock" = ""
}
foreach ($entry in $preserved.GetEnumerator()) { Write-Fixture $target $entry.Key $entry.Value }
Write-Fixture $target "resources\skins\default\play7.json" "edited packaged file"
Write-Fixture $target "modified.dll" "new patched DLL"
Write-Fixture $source "resources\skins\default\play7.json" "new packaged skin"
# Remove one exact test-source file to model a file discontinued by the new package.
Remove-Item -LiteralPath (Join-Path $source "resources\obsolete.txt")
$oldLayout = 'bmz-updater.exe', 'bmz-package.json', '.bmz-instance.lock', 'vcruntime140.dll', 'locked.dll', 'modified.dll'
foreach ($path in $oldLayout) { Remove-Item -LiteralPath (Join-Path $source $path) }
Write-Fixture $source "updater\instance.lock" ""
Copy-Item -LiteralPath $UpdaterPath -Destination (Join-Path $source 'updater\bmz-updater.exe')
Write-Fixture $source "dll\dll.manifest" "assembly manifest fixture only"
Write-Fixture $source "dll\vcruntime140.dll" "new VC runtime"
Write-Fixture $source "dll\ucrtbase.dll" "UCRT fixture"
Write-Fixture $source "dll\api-ms-win-crt-runtime-l1-1-0.dll" "API forwarder fixture"
Write-PackageManifest '0.0.2' grouped sxs

# Setup can complete after an invalid loader file was written. Keep the old DLLs in that case.
Write-Fixture $source "dll\ucrtbase.dll" "unexpected UCRT"
$brokenSetup = Build-Setup "0.0.2" -Sxs | Select-Object -Last 1
Run-Installer $brokenSetup 'unverified-runtime'
Assert-Content 'vcruntime140.dll' 'old VC runtime'
Assert-Content 'locked.dll' 'old locked DLL'
if (!(Select-String -LiteralPath (Join-Path $testRoot 'unverified-runtime.log') -Pattern 'installed SxS file did not verify: dll/ucrtbase.dll' -SimpleMatch -Quiet)) {
    throw 'Setup did not report why the old DLLs were kept'
}

Write-Fixture $source "dll\ucrtbase.dll" "UCRT fixture"
$newSetup = Build-Setup "0.0.2" -Sxs | Select-Object -Last 1
$lock = [IO.File]::Open((Join-Path $target 'locked.dll'), 'Open', 'Read', 'Read')
try {
    Run-Installer $newSetup 'locked-upgrade'
    Assert-Content 'locked.dll' 'old locked DLL'
    if (Test-Path -LiteralPath (Join-Path $target 'vcruntime140.dll')) {
        throw 'A locked DLL blocked retirement of other managed DLLs'
    }
} finally {
    $lock.Dispose()
}
foreach ($label in @("upgrade", "reinstall")) {
    Run-Installer $newSetup $label
    Assert-Content 'dll\vcruntime140.dll' 'new VC runtime'
    Assert-Content 'modified.dll' 'new patched DLL'
    Assert-Content "resources\skins\default\play7.json" "new packaged skin"
    Assert-Content "resources\obsolete.txt" "old package only"
    foreach ($entry in $preserved.GetEnumerator()) { Assert-Content $entry.Key $entry.Value }
    $backups = Join-Path $target 'updater\installer-dll-backup'
    foreach ($dll in @(@{name='vcruntime140.dll'; content='old VC runtime'}, @{name='locked.dll'; content='old locked DLL'})) {
        $backup = @(Get-ChildItem -LiteralPath $backups -Recurse -Filter $dll.name -File)
        if ((Test-Path -LiteralPath (Join-Path $target $dll.name)) -or $backup.Count -ne 1 -or
            [IO.File]::ReadAllText($backup[0].FullName) -cne $dll.content) {
            throw "Managed DLL was not retired exactly once: $($dll.name)"
        }
    }
}
Run-Installer (Join-Path $target "unins000.exe") "uninstall"
foreach ($entry in $preserved.GetEnumerator()) { Assert-Content $entry.Key $entry.Value }
if (@(Get-ChildItem -LiteralPath $backups -Recurse -Filter '*.dll' -File).Count -ne 2) {
    throw 'Uninstaller removed the retired DLL backups'
}
if (Test-Path -LiteralPath (Join-Path $target "bmz-player.exe")) {
    throw "Uninstaller did not remove the packaged executable"
}
Write-Host "PASS: Setup safely retires known root DLLs, retries locked files, and preserves user files. Logs: $testRoot"
