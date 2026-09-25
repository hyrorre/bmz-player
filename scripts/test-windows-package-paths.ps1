# Exercise actual packaged executables without GPU/audio or the user's AppData/config/DB.
param([string]$BinaryDirectory = "target/debug")
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$source = (Resolve-Path -LiteralPath $BinaryDirectory).Path
$testRoot = Join-Path $repo (".local\package-path-test-" + [guid]::NewGuid().ToString("N"))
$foreign = Join-Path $testRoot "foreign"
$songs = Join-Path $testRoot "empty-songs"
$poison = "This must never be parsed as TOML ["
New-Item -ItemType Directory -Force (Join-Path $foreign "data"), $songs | Out-Null
Set-Content -LiteralPath (Join-Path $foreign "data\config.toml") -Value $poison -Encoding utf8NoBOM -NoNewline
$version = [regex]::Match((Get-Content -Raw (Join-Path $repo "Cargo.toml")), '(?m)^version = "([^"]+)"').Groups[1].Value
$target = if ([Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -eq "Arm64") { "windows-arm64" } else { "windows-x64" }
$helperSource = Join-Path $source "updater\bmz-updater.exe"
if (!(Test-Path -LiteralPath $helperSource)) { $helperSource = Join-Path $source "bmz-updater.exe" }
$metadataScript = Join-Path $repo "scripts\generate-update-metadata.mjs"
$saved = @{}
$names = @("APPDATA", "LOCALAPPDATA", "BMZ_DATA_DIR", "BMZ_CACHE_DIR", "BMZ_LOGS_DIR", "BMZ_RESOURCE_DIR")
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process") }
$script:invocation = 0

function Assert-Path([string]$Path) {
    if (!(Test-Path -LiteralPath $Path)) { throw "Missing expected path: $Path" }
}

function Copy-TestBinary([string]$From, [string]$To) {
    # These fixtures never modify executable/DLL contents; avoid ten large debug binary copies.
    try { New-Item -ItemType HardLink -Path $To -Target $From -ErrorAction Stop | Out-Null }
    catch { Copy-Item -LiteralPath $From -Destination $To }
}

function Run-Player([string]$Root, [string[]]$PlayerArgs, [bool]$ExpectSuccess = $true) {
    $script:invocation++
    $out = Join-Path $testRoot "$($script:invocation).stdout.txt"
    $err = Join-Path $testRoot "$($script:invocation).stderr.txt"
    $arguments = ($PlayerArgs | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $process = Start-Process -FilePath (Join-Path $Root "bmz-player.exe") -ArgumentList $arguments `
        -WorkingDirectory $foreign -WindowStyle Hidden -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
    if (!$process.WaitForExit(60000)) {
        $process.Kill()
        throw "Test child timed out: $arguments"
    }
    if (($process.ExitCode -eq 0) -ne $ExpectSuccess) {
        throw "Unexpected exit $($process.ExitCode) for $arguments : $(Get-Content -Raw $err)"
    }
}

try {
    foreach ($layout in @("legacy", "grouped")) {
        foreach ($mode in @("portable", "portable-version-mismatch", "installer", "installer-adjacent", "override", "blocked")) {
            $case = Join-Path $testRoot "$layout-$mode"
            $package = Join-Path $case "日本語 BMZ Player"
            $kind = if ($mode.StartsWith("installer")) { "installer" } else { "portable" }
            foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $null, "Process") }
            $env:APPDATA = Join-Path $case "roaming"
            $env:LOCALAPPDATA = Join-Path $case "local"
            New-Item -ItemType Directory -Force $package | Out-Null
            Copy-TestBinary (Join-Path $source "bmz-player.exe") (Join-Path $package "bmz-player.exe")
            Get-ChildItem -LiteralPath $source -Filter *.dll | ForEach-Object { Copy-TestBinary $_.FullName (Join-Path $package $_.Name) }
            $helper = if ($layout -eq "legacy") { "bmz-updater.exe" } else { "updater\bmz-updater.exe" }
            New-Item -ItemType Directory -Force (Split-Path -Parent (Join-Path $package $helper)) | Out-Null
            Copy-TestBinary $helperSource (Join-Path $package $helper)
            $metadataVersion = if ($mode -eq "portable-version-mismatch") { "0.0.0" } else { $version }
            & node $metadataScript package $package $kind $target $metadataVersion $layout
            if ($LASTEXITCODE -ne 0) { throw "Package metadata generation failed" }

            $data = Join-Path $package "data"
            if ($mode -eq "installer") { $data = Join-Path $env:APPDATA "BMZ Player" }
            if ($mode -eq "installer-adjacent") { New-Item -ItemType Directory -Force $data | Out-Null }
            if ($mode -eq "override") {
                $data = Join-Path $case "override-data"
                $env:BMZ_DATA_DIR = $data
                $env:BMZ_CACHE_DIR = Join-Path $case "override-cache"
                $env:BMZ_LOGS_DIR = Join-Path $case "override-logs"
                $env:BMZ_RESOURCE_DIR = Join-Path $case "override-resources"
            }
            if ($kind -eq "portable") {
                $appDataConfig = Join-Path $env:APPDATA "BMZ Player\config.toml"
                New-Item -ItemType Directory -Force (Split-Path -Parent $appDataConfig) | Out-Null
                Set-Content -LiteralPath $appDataConfig -Value $poison -Encoding utf8NoBOM -NoNewline
            }
            if ($mode -eq "blocked") {
                Set-Content -LiteralPath $data -Value "not a directory" -Encoding utf8NoBOM
                Run-Player $package @("songs", "add", $songs) $false
            } else {
                Run-Player $package @("songs", "add", $songs)
                Run-Player $package @("songs", "load", "--no-everything")
                Assert-Path (Join-Path $data "config.toml")
                Assert-Path (Join-Path $data "library.db")
                Assert-Path (Join-Path $data "profiles")
                $auxiliary = if ($mode -eq "installer") { Join-Path $env:LOCALAPPDATA "BMZ Player" } else { $data }
                Assert-Path $(if ($env:BMZ_CACHE_DIR) { $env:BMZ_CACHE_DIR } else { Join-Path $auxiliary "cache" })
                Assert-Path $(if ($env:BMZ_LOGS_DIR) { $env:BMZ_LOGS_DIR } else { Join-Path $auxiliary "logs" })
                if ($mode -in @("installer", "override") -and (Test-Path -LiteralPath (Join-Path $package "data"))) {
                    throw "Unexpected adjacent data creation in $mode"
                }
            }
            if ($kind -eq "portable" -and [IO.File]::ReadAllText($appDataConfig) -cne $poison) {
                throw "Portable touched installer configuration"
            }
            if ($layout -eq "grouped") {
                foreach ($name in @("bmz-package.json", "bmz-updater.exe", ".bmz-instance.lock", ".bmz-updater.lock", ".bmz-update")) {
                    if (Test-Path -LiteralPath (Join-Path $package $name)) { throw "Unexpected root updater file: $name" }
                }
            }
        }
    }
    if ([IO.File]::ReadAllText((Join-Path $foreign "data\config.toml")) -cne $poison) { throw "Touched working-directory config" }
    if (Test-Path -LiteralPath (Join-Path $foreign "data\library.db")) { throw "Touched working-directory database" }
    Write-Host "PASS: package layouts, portable/installer paths, overrides and failure isolation. Logs: $testRoot"
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], "Process") }
}
