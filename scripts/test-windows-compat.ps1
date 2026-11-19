param(
    [Parameter(Mandatory = $true)][string]$PackageDirectory,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
if (Test-Path -LiteralPath (Join-Path $package 'data')) { throw 'Use pristine package staging without user data' }
& (Join-Path $PSScriptRoot 'verify-windows-compat-imports.ps1') -PackageDirectory $package
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw "Use a fresh output directory: $output" }
if ($output.StartsWith($package + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or $output -eq $package) {
    throw "Smoke output must be outside the release package"
}
New-Item -ItemType Directory -Path $output | Out-Null
$copy = Join-Path $output "package"
# Hard links keep the full release package test affordable without mutating its files.
foreach ($file in (Get-ChildItem -LiteralPath $package -Recurse -File)) {
    $relative = [IO.Path]::GetRelativePath($package, $file.FullName)
    if ($relative -in @('updater\instance.lock', '.bmz-instance.lock')) { continue }
    $target = Join-Path $copy $relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
    try { New-Item -ItemType HardLink -Path $target -Target $file.FullName -ErrorAction Stop | Out-Null }
    catch { Copy-Item -LiteralPath $file.FullName -Destination $target }
}
$foreign = Join-Path $output "working-directory"
New-Item -ItemType Directory -Path $foreign | Out-Null
$names = @('PATH','BMZ_RESOURCE_DIR','BMZ_DATA_DIR','BMZ_CACHE_DIR','BMZ_LOGS_DIR','RUST_LOG')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
function Run-Player([string[]]$PlayerArgs, [string]$Label) {
    $process = Start-Process -FilePath (Join-Path $copy 'bmz-player.exe') -ArgumentList $PlayerArgs -WorkingDirectory $foreign -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output "$Label.stdout.txt") -RedirectStandardError (Join-Path $output "$Label.stderr.txt")
    if (-not $process.WaitForExit(60000)) {
        $process.Kill()
        throw "Packaged startup timed out: $Label"
    }
    if ($process.ExitCode -ne 0) { throw "Packaged startup failed: $Label, exit $($process.ExitCode)" }
}
try {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
    $env:PATH = "$env:WINDIR\System32;$env:WINDIR;$env:WINDIR\System32\Wbem"
    $env:RUST_LOG = 'info'
    Run-Player @('--help') 'help'
    if ((Get-Content -LiteralPath (Join-Path $output 'help.stdout.txt') -Raw) -notmatch 'bmz-player') { throw 'Missing CLI help' }
    $updater = Join-Path $copy 'updater\bmz-updater.exe'
    if (-not (Test-Path -LiteralPath $updater)) { $updater = Join-Path $copy 'bmz-updater.exe' }
    $process = Start-Process -FilePath $updater -ArgumentList '--help' -WorkingDirectory $foreign -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'updater.stdout.txt')
    if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Updater startup timed out' }
    if ($process.ExitCode -ne 0) { throw "Updater startup failed: $($process.ExitCode)" }
    Run-Player @('profile','create','ci-smoke','--activate') 'bootstrap'
    $config = Join-Path $copy 'data\config.toml'
    $text = Get-Content -LiteralPath $config -Raw
    if ($text -notmatch 'gamepad_backend = "(?:Auto|Gilrs|RawInput)"') { throw 'Unexpected fresh gamepad configuration' }
    $text = $text -replace 'gamepad_backend = "(?:Auto|Gilrs|RawInput)"', 'gamepad_backend = "Auto"'
    $text = $text.Replace('check_on_startup = true', 'check_on_startup = false')
    $text | Set-Content -LiteralPath $config -Encoding utf8NoBOM
    Run-Player @('--boot-play-sample','--autoplay-on-start','--smoke-exit-after-play-frames','120') 'auto'
    $text.Replace('gamepad_backend = "Auto"', 'gamepad_backend = "RawInput"') | Set-Content -LiteralPath $config -Encoding utf8NoBOM
    Run-Player @('--boot-play-sample','--autoplay-on-start','--smoke-exit-after-play-frames','120') 'rawinput'
    $logs = (Get-ChildItem -LiteralPath (Join-Path $copy 'data\logs') -File | Get-Content) -join "`n"
    $logs | Set-Content -LiteralPath (Join-Path $output 'startup.log') -Encoding utf8NoBOM
    if ([regex]::Matches($logs, 'smoke play frame count reached').Count -ne 2 -or $logs -notmatch 'backend="Raw Input"') { throw 'Sample playback or Raw Input initialization was not confirmed' }
    if ($logs -notmatch 'window and renderer surface ready' -or $logs -notmatch [regex]::Escape((Join-Path $copy 'resources'))) { throw 'Packaged rendering/resources were not confirmed' }
    if (Test-Path -LiteralPath (Join-Path $package 'data')) { throw 'Release package was contaminated by runtime data' }
    Write-Host 'Packaged CLI, updater, Auto and Raw Input sample playback passed on the CI host; Windows 8.1 remains unverified.'
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
