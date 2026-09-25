# Builds a tiny isolated package with the production installer script. Never uses
# the real installation, profile, or application executable. Keeps logs for review.
param([string]$Iscc = "iscc.exe")
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
$target = Join-Path $testRoot "installed"
$output = Join-Path $testRoot "output"
$testAppId = [guid]::NewGuid().ToString()
New-Item -ItemType Directory -Force $source, $output | Out-Null

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
function Build-Setup([string]$Version) {
    & $Iscc "/DAppName=BMZ Installer Preservation Test" "/DAppId=$testAppId" `
        "/DAppVersion=$Version" "/DSourceDir=$source" "/DOutputDir=$output" `
        "/DIconFile=$(Join-Path $repo 'assets\app-icon\bmz-player.ico')" `
        (Join-Path $repo "installer\inno\bmz-player.iss")
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
Write-Fixture $source "updater\bmz-updater.exe" "updater fixture only"
Write-Fixture $source "updater\bmz-package.json" "metadata fixture only"
Write-Fixture $source "updater\instance.lock" ""
Write-Fixture $source "resources\skins\default\play7.json" "old packaged skin"
Write-Fixture $source "resources\obsolete.txt" "old package only"
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
    "updater\job-old\backup\0" = "recovery backup"
    "updater\update.lock" = ""
}
foreach ($entry in $preserved.GetEnumerator()) { Write-Fixture $target $entry.Key $entry.Value }
Write-Fixture $target "resources\skins\default\play7.json" "edited packaged file"
Write-Fixture $source "resources\skins\default\play7.json" "new packaged skin"
# Remove one exact test-source file to model a file discontinued by the new package.
Remove-Item -LiteralPath (Join-Path $source "resources\obsolete.txt")
$newSetup = Build-Setup "0.0.2" | Select-Object -Last 1
foreach ($label in @("upgrade", "reinstall")) {
    Run-Installer $newSetup $label
    Assert-Content "resources\skins\default\play7.json" "new packaged skin"
    Assert-Content "resources\obsolete.txt" "old package only"
    foreach ($entry in $preserved.GetEnumerator()) { Assert-Content $entry.Key $entry.Value }
}
Run-Installer (Join-Path $target "unins000.exe") "uninstall"
foreach ($entry in $preserved.GetEnumerator()) { Assert-Content $entry.Key $entry.Value }
if (Test-Path -LiteralPath (Join-Path $target "bmz-player.exe")) {
    throw "Uninstaller did not remove the packaged executable"
}
Write-Host "PASS: fresh install, upgrade, reinstall and uninstall preserve user files. Logs: $testRoot"
