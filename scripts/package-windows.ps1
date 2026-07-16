param(
    [ValidateSet("Debug", "Release")]
    [string]$Profile = "Release",
    [string]$Target = "",
    [switch]$SkipBuild,
    [string]$OutDir = "",
    [string]$StageName = "BMZ Player",
    [string[]]$DllDir = @(),
    [switch]$CopySiblingDlls,
    [switch]$Installer,
    [string]$IsccPath = "",
    [string]$GameInputPackageDir = "",
    [switch]$NoDefaultFeatures,
    [string]$Features = "",
    [switch]$SkipRustLicenseReport,
    [switch]$Smoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Resolve-FullPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $Path))
}

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @()
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function Require-Command {
    param([Parameter(Mandatory = $true)][string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "required command not found: $Name"
    }
}

function Require-File {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "missing file: $Path"
    }
}

function Require-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "missing directory: $Path"
    }
}

function Get-CargoVersion {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    $cargoToml = Join-Path $RepoRoot "Cargo.toml"
    foreach ($line in Get-Content -LiteralPath $cargoToml) {
        if ($line -match '^version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }
    throw "failed to read workspace version from $cargoToml"
}

function New-RustLicenseReport {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Output,
        [string]$Target = "",
        [switch]$NoDefaultFeatures,
        [string]$Features = ""
    )

    Require-Command "cargo-about"

    $arguments = @("generate", "--workspace", "--locked", "--fail", "--output-file", $Output)
    if ($Target) {
        $arguments += @("--target", $Target)
    }
    if ($NoDefaultFeatures) {
        $arguments += "--no-default-features"
    }
    if ($Features) {
        $arguments += @("--features", $Features)
    }
    $arguments += "about.hbs"

    Push-Location $RepoRoot
    try {
        Invoke-Native "cargo-about" $arguments
    } finally {
        Pop-Location
    }
}

function Sync-InnoAppVersion {
    param(
        [Parameter(Mandatory = $true)][string]$IssPath,
        [Parameter(Mandatory = $true)][string]$Version
    )

    Require-File $IssPath
    if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z.+_-]*$') {
        throw "invalid package version: $Version"
    }

    $content = Get-Content -LiteralPath $IssPath -Raw
    $pattern = '(?m)^#define\s+AppVersion\s+"[^"]+"'
    if ($content -notmatch $pattern) {
        throw "failed to find AppVersion define in $IssPath"
    }
    $updated = [regex]::Replace($content, $pattern, "#define AppVersion `"$Version`"", 1)
    if ($updated -ne $content) {
        Set-Content -LiteralPath $IssPath -Value $updated -NoNewline
    }
}

function Copy-DirectoryMirror {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Require-Directory $Source
    if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null

    & robocopy $Source $Destination /MIR /XD .git /XF .git .DS_Store
    if ($LASTEXITCODE -gt 7) {
        throw "robocopy failed with exit code $LASTEXITCODE"
    }
}

function Copy-RequiredFile {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Require-File $Source
    $parent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function Copy-DllDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Require-Directory $Source
    $dlls = Get-ChildItem -LiteralPath $Source -Filter "*.dll" -File
    foreach ($dll in $dlls) {
        Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $Destination $dll.Name) -Force
    }
}

function Add-UniquePath {
    param(
        [Parameter(Mandatory = $true)]$Paths,
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    foreach ($existing in $Paths) {
        if ([StringComparer]::OrdinalIgnoreCase.Equals($existing, $fullPath)) {
            return
        }
    }
    $Paths.Add($fullPath) | Out-Null
}

function Add-VcpkgRootBinCandidate {
    param(
        [Parameter(Mandatory = $true)]$Paths,
        [string]$Root,
        [Parameter(Mandatory = $true)][string]$Triplet
    )

    if ([string]::IsNullOrWhiteSpace($Root)) {
        return
    }

    Add-UniquePath $Paths (Join-Path (Join-Path (Join-Path $Root "installed") $Triplet) "bin")
}

function Add-VcpkgInstalledBinCandidate {
    param(
        [Parameter(Mandatory = $true)]$Paths,
        [string]$InstalledRoot,
        [Parameter(Mandatory = $true)][string]$Triplet
    )

    if ([string]::IsNullOrWhiteSpace($InstalledRoot)) {
        return
    }

    Add-UniquePath $Paths (Join-Path (Join-Path $InstalledRoot $Triplet) "bin")
}

function Find-Iscc {
    param([string]$ExplicitPath)

    if ($ExplicitPath) {
        Require-File $ExplicitPath
        return $ExplicitPath
    }

    $command = Get-Command "ISCC.exe" -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $candidates = @()
    $programFilesX86 = [Environment]::GetEnvironmentVariable("ProgramFiles(x86)")
    $programFiles = [Environment]::GetEnvironmentVariable("ProgramFiles")
    if ($programFilesX86) {
        $candidates += (Join-Path $programFilesX86 "Inno Setup 6\ISCC.exe")
    }
    if ($programFiles) {
        $candidates += (Join-Path $programFiles "Inno Setup 6\ISCC.exe")
    }

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    throw "ISCC.exe was not found. Install Inno Setup 6 or pass -IsccPath."
}

function Resolve-InstallerArch {
    param([string]$Target)

    if ($Target -match "i686|i586|x86-pc-windows") {
        return "x86"
    }
    if ($Target -match "aarch64|arm64") {
        return "arm64"
    }
    return "x64"
}

function Resolve-VcpkgTriplet {
    param([string]$Target)

    if ($Target -match "i686|i586|x86-pc-windows") {
        return "x86-windows"
    }
    if ($Target -match "aarch64|arm64") {
        return "arm64-windows"
    }
    return "x64-windows"
}

function Find-DefaultVcpkgDllDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$Target
    )

    $triplet = Resolve-VcpkgTriplet $Target
    $candidates = [System.Collections.Generic.List[string]]::new()

    Add-VcpkgInstalledBinCandidate $candidates (Join-Path $RepoRoot "vcpkg_installed") $triplet
    Add-VcpkgRootBinCandidate $candidates $env:VCPKG_ROOT $triplet

    $command = Get-Command "vcpkg" -ErrorAction SilentlyContinue
    if ($command -and $command.Source) {
        $source = [System.IO.Path]::GetFullPath($command.Source)
        $commandRoot = Split-Path -Parent $source
        Add-VcpkgRootBinCandidate $candidates $commandRoot $triplet

        if ($source -match "\\scoop\\shims\\vcpkg(?:\.exe)?$") {
            if ($env:USERPROFILE) {
                Add-VcpkgRootBinCandidate $candidates (Join-Path $env:USERPROFILE "scoop\apps\vcpkg\current") $triplet
            }
        } elseif ($source -match "\\scoop\\apps\\vcpkg\\[^\\]+\\vcpkg(?:\.exe)?$") {
            Add-VcpkgRootBinCandidate $candidates $commandRoot $triplet
        }
    }

    if ($env:USERPROFILE) {
        Add-VcpkgRootBinCandidate $candidates (Join-Path $env:USERPROFILE "scoop\apps\vcpkg\current") $triplet
    }
    if ($env:SCOOP) {
        Add-VcpkgRootBinCandidate $candidates (Join-Path $env:SCOOP "apps\vcpkg\current") $triplet
    }
    if ($env:SCOOP_GLOBAL) {
        Add-VcpkgRootBinCandidate $candidates (Join-Path $env:SCOOP_GLOBAL "apps\vcpkg\current") $triplet
    }
    Add-VcpkgRootBinCandidate $candidates "C:\vcpkg" $triplet

    foreach ($candidate in $candidates) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Container)) {
            continue
        }
        $dll = Get-ChildItem -LiteralPath $candidate -Filter "*.dll" -File -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($dll) {
            return $candidate
        }
    }

    return $null
}

$repoRoot = Resolve-RepoRoot
Set-Location $repoRoot

if (-not $OutDir) {
    if ($env:BMZ_WINDOWS_OUT_DIR) {
        $OutDir = $env:BMZ_WINDOWS_OUT_DIR
    } else {
        $OutDir = Join-Path $repoRoot "dist\windows"
    }
}
$OutDir = Resolve-FullPath $OutDir

if (-not $GameInputPackageDir -and $env:BMZ_GAMEINPUT_PACKAGE_DIR) {
    $GameInputPackageDir = $env:BMZ_GAMEINPUT_PACKAGE_DIR
}
if ($GameInputPackageDir) {
    $GameInputPackageDir = Resolve-FullPath $GameInputPackageDir
    Require-Directory $GameInputPackageDir
    Require-File (Join-Path $GameInputPackageDir "redist\GameInputRedist.msi")
    Require-File (Join-Path $GameInputPackageDir "LICENSE.txt")
    Require-File (Join-Path $GameInputPackageDir "NOTICE.txt")
}

Require-Command "cargo"
Require-Command "robocopy"

$profileDir = $Profile.ToLowerInvariant()
$cargoArgs = @("build", "-p", "bmz-player")
if ($Profile -eq "Release") {
    $cargoArgs += "--release"
}
if ($Target) {
    $cargoArgs += @("--target", $Target)
}
if ($NoDefaultFeatures) {
    $cargoArgs += "--no-default-features"
}
if ($Features) {
    $cargoArgs += @("--features", $Features)
}

if (-not $SkipBuild) {
    Write-Host "==> Building bmz-player ($profileDir)"
    Invoke-Native "cargo" $cargoArgs
}

$targetBase = Join-Path $repoRoot "target"
if ($Target) {
    $targetBase = Join-Path $targetBase $Target
}
$binary = Join-Path (Join-Path $targetBase $profileDir) "bmz-player.exe"
Require-File $binary

$version = Get-CargoVersion $repoRoot
$issPath = Join-Path $repoRoot "installer\inno\bmz-player.iss"
Sync-InnoAppVersion $issPath $version
$defaultSkin = Join-Path $repoRoot "data\skins\default\select.json"
$rmzSkin = Join-Path $repoRoot "data\skins\Rmz-skin\play7main.luaskin"
$mzSelectSkin = Join-Path $repoRoot "data\skins\mz-select\music_select.luaskin"
$luxezFlatSkin = Join-Path $repoRoot "data\skins\Luxez-Flat\music_select.luaskin"
$sampleSong = Join-Path $repoRoot "data\songs\sample-playable\sample-playable.bms"
$appIcon = Join-Path $repoRoot "assets\app-icon\bmz-player.ico"
Require-File $defaultSkin
Require-File $rmzSkin
Require-File $mzSelectSkin
Require-File $luxezFlatSkin
Require-File $sampleSong
Require-File $appIcon

$stageDir = Join-Path $OutDir $StageName
$resourcesDir = Join-Path $stageDir "resources"
$licensesDir = Join-Path $resourcesDir "licenses"

Write-Host "==> Creating $stageDir"
if (Test-Path -LiteralPath $stageDir) {
    Remove-Item -LiteralPath $stageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $resourcesDir "skins") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $resourcesDir "songs") | Out-Null
New-Item -ItemType Directory -Force -Path $licensesDir | Out-Null

Copy-RequiredFile $binary (Join-Path $stageDir "bmz-player.exe")
Copy-DirectoryMirror (Join-Path $repoRoot "data\skins\default") (Join-Path $resourcesDir "skins\default")
Copy-DirectoryMirror (Join-Path $repoRoot "data\skins\Rmz-skin") (Join-Path $resourcesDir "skins\Rmz-skin")
Copy-DirectoryMirror (Join-Path $repoRoot "data\skins\mz-select") (Join-Path $resourcesDir "skins\mz-select")
Copy-DirectoryMirror (Join-Path $repoRoot "data\skins\Luxez-Flat") (Join-Path $resourcesDir "skins\Luxez-Flat")
Copy-DirectoryMirror (Join-Path $repoRoot "data\songs\sample-playable") (Join-Path $resourcesDir "songs\sample-playable")
Copy-RequiredFile (Join-Path $repoRoot "LICENSE") (Join-Path $licensesDir "BMZ-GPL-3.0-only.txt")
Copy-RequiredFile (Join-Path $repoRoot "docs\licenses.md") (Join-Path $licensesDir "license-notes.md")
Copy-RequiredFile (Join-Path $repoRoot "THIRD-PARTY-NOTICES.txt") (Join-Path $licensesDir "third-party-notices.txt")
if ($GameInputPackageDir) {
    $redistDir = Join-Path $resourcesDir "redist"
    New-Item -ItemType Directory -Force -Path $redistDir | Out-Null
    Copy-RequiredFile `
        (Join-Path $GameInputPackageDir "redist\GameInputRedist.msi") `
        (Join-Path $redistDir "GameInputRedist.msi")
    Copy-RequiredFile `
        (Join-Path $GameInputPackageDir "LICENSE.txt") `
        (Join-Path $licensesDir "GameInput-LICENSE.txt")
    Copy-RequiredFile `
        (Join-Path $GameInputPackageDir "NOTICE.txt") `
        (Join-Path $licensesDir "GameInput-NOTICE.txt")
}
if (-not $SkipRustLicenseReport -and $env:BMZ_SKIP_RUST_LICENSE_REPORT -ne "1") {
    Write-Host "==> Generating Rust dependency license report"
    New-RustLicenseReport `
        -RepoRoot $repoRoot `
        -Output (Join-Path $licensesDir "rust-dependency-licenses.txt") `
        -Target $Target `
        -NoDefaultFeatures:$NoDefaultFeatures `
        -Features $Features
} else {
    Write-Host "==> Skipping Rust dependency license report"
}
Copy-RequiredFile $appIcon (Join-Path $resourcesDir "bmz-player.ico")

if ($CopySiblingDlls) {
    Write-Host "==> Copying DLLs from binary directory"
    Copy-DllDirectory (Split-Path -Parent $binary) $stageDir
}

$envDllDirs = @()
if ($env:BMZ_WINDOWS_DLL_DIRS) {
    $envDllDirs = $env:BMZ_WINDOWS_DLL_DIRS -split ";"
}
$dllDirs = $DllDir + $envDllDirs
if ($dllDirs.Count -eq 0) {
    $defaultDllDir = Find-DefaultVcpkgDllDirectory $repoRoot $Target
    if ($defaultDllDir) {
        Write-Host "==> Auto-detected vcpkg DLL directory: $defaultDllDir"
        $dllDirs = @($defaultDllDir)
    }
}
foreach ($dir in $dllDirs) {
    if (-not [string]::IsNullOrWhiteSpace($dir)) {
        Write-Host "==> Copying DLLs from $dir"
        Copy-DllDirectory (Resolve-FullPath $dir) $stageDir
    }
}

if ($Smoke) {
    Write-Host "==> Running packaged smoke test"
    $oldDataDir = $env:BMZ_DATA_DIR
    $smokeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("bmz-player-windows-smoke-" + [Guid]::NewGuid().ToString("N"))
    try {
        $env:BMZ_DATA_DIR = Join-Path $smokeRoot "data"
        Invoke-Native (Join-Path $stageDir "bmz-player.exe") @("--boot-play-sample", "--smoke-exit-after-frames", "3")
    } finally {
        $env:BMZ_DATA_DIR = $oldDataDir
    }
}

if ($Installer) {
    $iscc = Find-Iscc $IsccPath
    $installerOutDir = Join-Path $OutDir "installer"
    New-Item -ItemType Directory -Force -Path $installerOutDir | Out-Null
    Require-File $issPath
    $arch = Resolve-InstallerArch $Target

    Write-Host "==> Building Inno Setup installer"
    Invoke-Native $iscc @(
        "/DAppVersion=$version",
        "/DSourceDir=$stageDir",
        "/DOutputDir=$installerOutDir",
        "/DIconFile=$appIcon",
        "/DAppArch=$arch",
        $issPath
    )
}

Write-Host "==> Done"
Write-Host $stageDir
