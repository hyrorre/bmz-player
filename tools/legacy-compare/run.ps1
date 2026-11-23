param(
    [string]$OpenLr2Directory = 'D:\OpenLR2',
    [string]$EnvironmentScript,
    [string]$BuildToolsDirectory,
    [string]$OutputDirectory,
    [switch]$InventoryOnly,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if ($EnvironmentScript) { . $EnvironmentScript }
if ($BuildToolsDirectory) {
    $env:VSCMD_SKIP_SENDTELEMETRY = '1'
    $env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
    $vcvars = Join-Path $BuildToolsDirectory 'vs\VC\Auxiliary\Build\vcvars64.bat'
    $vsEnv = & cmd.exe /d /s /c ('"' + $vcvars + '" >nul && set')
    if ($LASTEXITCODE -ne 0) { throw 'Visual C++ environment setup failed' }
    foreach ($entry in $vsEnv) {
        if ($entry -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
        }
    }
    $env:CARGO_HOME = Join-Path $BuildToolsDirectory 'cargo'
    $env:RUSTUP_HOME = Join-Path $BuildToolsDirectory 'rustup'
    $env:FFMPEG_DIR = Join-Path $BuildToolsDirectory 'ffmpeg\ffmpeg-master-latest-win64-lgpl-shared'
    $env:LIBCLANG_PATH = Join-Path $BuildToolsDirectory 'libclang\libclang-18.1.1.data\platlib\clang\native'
    $env:BINDGEN_EXTRA_CLANG_ARGS = '-DBMZ_FFMPEG_HEADERS_N9_0=1'
    $env:PATH = "$env:CARGO_HOME\bin;$env:FFMPEG_DIR\bin;$env:PATH"
}
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo '.local\legacy-compare' }
Push-Location $repo
try {
    if (-not $InventoryOnly -and -not $SkipBuild) {
        & cargo build -p bmz-gameplay --example legacy_compare --locked --offline
        if ($LASTEXITCODE -ne 0) { throw "Build failed: $LASTEXITCODE" }
    }
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
    $runner = Join-Path $target 'debug\examples\legacy_compare.exe'
    $compareArgs = @('tools/legacy-compare/compare.py', '--openlr2', $OpenLr2Directory,
        '--output', $OutputDirectory, '--runner', $runner)
    if ($InventoryOnly) { $compareArgs += '--inventory-only' }
    & python @compareArgs
    if ($LASTEXITCODE -ne 0) { throw "Comparison failed: $LASTEXITCODE" }
} finally { Pop-Location }
