# Runs the actual update helpers on isolated portable installs.
param(
    [Parameter(Mandatory)][string]$BridgePackage,
    [Parameter(Mandatory)][string]$SxsPackage,
    [Parameter(Mandatory)][string]$OldHelper,
    [ValidateSet(1, 2, 3)][int]$OldProtocol = 3,
    [Parameter(Mandatory)][string]$OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bridgeSource = (Resolve-Path -LiteralPath $BridgePackage).Path
$sxsSource = (Resolve-Path -LiteralPath $SxsPackage).Path
$oldHelperPath = (Resolve-Path -LiteralPath $OldHelper).Path
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
foreach ($source in @($bridgeSource, $sxsSource)) {
    if ($output -eq $source -or $output.StartsWith("$source\", [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Use an output outside the source packages'
    }
}
New-Item -ItemType Directory -Path $output | Out-Null
$metadata = Join-Path $PSScriptRoot 'generate-update-metadata.mjs'
$node = (Get-Command node -CommandType Application | Select-Object -First 1).Source
$legacyManifest = 'bmz-package.json'
$sxsManifest = 'updater\bmz-package.json'
$version = (Get-Content -LiteralPath (Join-Path $sxsSource $sxsManifest) -Raw | ConvertFrom-Json).version
$bridgeVersion = '0.0.2'
$retired = 'test-retired.dll'
$userFiles = @{'data\profiles\keep\score.db' = 'score'; 'resources\skins\custom.txt' = 'skin'; 'unknown.dll' = 'user DLL'}

function Copy-UpdateFixture([string]$Source, [string]$Destination, [bool]$Sxs) {
    New-Item -ItemType Directory -Path (Join-Path $Destination 'resources\skins') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $Source 'bmz-player.exe') -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $Source 'resources\skins\default') -Destination (Join-Path $Destination 'resources\skins') -Recurse
    if ($Sxs) {
        Copy-Item -LiteralPath (Join-Path $Source 'dll') -Destination $Destination -Recurse
        New-Item -ItemType Directory -Path (Join-Path $Destination 'updater') | Out-Null
        Copy-Item -LiteralPath (Join-Path $Source 'updater\bmz-updater.exe') -Destination (Join-Path $Destination 'updater')
    } else {
        Get-ChildItem -LiteralPath $Source -Filter '*.dll' -File | Copy-Item -Destination $Destination
        Copy-Item -LiteralPath (Join-Path $Source 'bmz-updater.exe') -Destination $Destination
    }
}

function Write-Package([string]$Root, [string]$Version, [bool]$Sxs) {
    $layout, $dllLayout = if ($Sxs) { 'grouped', 'sxs' } else { 'legacy', 'flat' }
    & $node $metadata package $Root portable windows-x64 $Version $layout $dllLayout
    if ($LASTEXITCODE -ne 0) { throw 'Fixture package generation failed' }
}

function Get-Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }

function New-FixtureStart([string]$Executable, [string]$Root) {
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WorkingDirectory = $output
    foreach ($name in @($start.Environment.Keys)) {
        if ($name -like 'BMZ_*') { [void]$start.Environment.Remove($name) }
    }
    $start.Environment['PATH'] = "$env:WINDIR\System32;$env:WINDIR;$env:WINDIR\System32\Wbem"
    $start.Environment['APPDATA'] = Join-Path $Root 'test-roaming'
    $start.Environment['LOCALAPPDATA'] = Join-Path $Root 'test-local'
    return $start
}

function Initialize-FixtureProfile([string]$Root) {
    $start = New-FixtureStart (Join-Path $Root 'bmz-player.exe') $Root
    foreach ($arg in @('profile', 'create', 'sxs-fixture', '--activate')) { $start.ArgumentList.Add($arg) }
    $child = [Diagnostics.Process]::Start($start)
    try {
        if (-not $child.WaitForExit(30000) -or $child.ExitCode -ne 0) { throw 'Could not create update test profile' }
        $config = Join-Path $Root 'data\config.toml'
        [IO.File]::WriteAllText($config, [IO.File]::ReadAllText($config).Replace('check_on_startup = true', 'check_on_startup = false'))
    } finally {
        if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
        $child.Dispose()
    }
}

function Stop-FixturePlayer([string]$Root) {
    $exe = Join-Path $Root 'bmz-player.exe'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $player = Get-Process -Name 'bmz-player' -ErrorAction SilentlyContinue |
            Where-Object Path -EQ $exe | Select-Object -First 1
        if ($player -and $player.CloseMainWindow() -and $player.WaitForExit(5000)) { return }
        Start-Sleep -Milliseconds 100
    } while ($clock.Elapsed.TotalSeconds -lt 15)
    if ($player -and -not $player.HasExited) { $player.Kill(); $player.WaitForExit() }
    throw "Updated player did not restart and close normally: $Root"
}

function Invoke-Update([string]$Root, [string]$Work, [string]$InstalledHelper, [bool]$ExpectSuccess) {
    $helper = Join-Path $Work 'helper.exe'
    Copy-Item -LiteralPath (Join-Path $Root $InstalledHelper) -Destination $helper
    $request = Join-Path $Work 'request.json'
    @{
        root = $Root; work = $Work
        restart = @{
            profile = 'sxs-fixture'; data_dir = (Join-Path $Root 'data'); cache_dir = (Join-Path $Root 'data\cache')
            logs_dir = (Join-Path $Root 'data\logs'); resource_dir = (Join-Path $Root 'resources')
        }
    } | ConvertTo-Json | Set-Content -LiteralPath $request -Encoding utf8NoBOM
    $start = New-FixtureStart $helper $Root
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.ArgumentList.Add('--apply')
    $start.ArgumentList.Add($request)
    $child = [Diagnostics.Process]::Start($start)
    try {
        $err = $child.StandardError.ReadToEndAsync()
        $ready = $child.StandardOutput.ReadLineAsync()
        if (-not $ready.Wait(30000)) { throw "Helper preflight timed out: $Work" }
        if ($ready.Result -ne 'READY') {
            throw "Helper failed preflight: $($ready.Result)"
        }
        $child.StandardInput.WriteLine('APPLY')
        $child.StandardInput.Close()
        $clock = [Diagnostics.Stopwatch]::StartNew()
        while (-not $child.WaitForExit(100)) {
            if ($clock.Elapsed.TotalSeconds -ge 40) { throw "Helper did not finish: $Work" }
            if (-not $ExpectSuccess -and (Test-Path -LiteralPath (Join-Path $Work 'update.log'))) {
                $child.Refresh()
                if ($child.MainWindowTitle -eq 'BMZ Player Update') { [void]$child.CloseMainWindow() }
            }
        }
        if (($child.ExitCode -eq 0) -ne $ExpectSuccess) { throw "Unexpected helper result: $($err.Result)" }
        Stop-FixturePlayer $Root
    } finally {
        if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
        $child.Dispose()
    }
}

foreach ($broken in @($false, $true)) {
    $case = if ($broken) { 'rollback' } else { 'success' }
    $root = Join-Path $output "$case\BMZ Player"
    Copy-UpdateFixture $bridgeSource $root $false
    Copy-Item -LiteralPath $oldHelperPath -Destination (Join-Path $root 'bmz-updater.exe') -Force
    Write-Package $root '0.0.1' $false
    Initialize-FixtureProfile $root
    foreach ($entry in $userFiles.GetEnumerator()) {
        $path = Join-Path $root $entry.Key
        New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
        [IO.File]::WriteAllText($path, $entry.Value)
    }
    $firstWork = if ($OldProtocol -eq 1) { '.bmz-update\job-bridge' } else { 'updater\job-bridge' }
    $firstWork = Join-Path $root $firstWork
    $stage = Join-Path $firstWork 'stage'
    Copy-UpdateFixture $bridgeSource $stage $false
    [IO.File]::WriteAllText((Join-Path $stage $retired), 'original managed DLL')
    Write-Package $stage $bridgeVersion $false
    Invoke-Update $root $firstWork 'bmz-updater.exe' $true
    $installed = Get-Content -LiteralPath (Join-Path $root $legacyManifest) -Raw | ConvertFrom-Json
    if ($installed.version -ne $bridgeVersion -or $installed.min_updater_protocol -ne 1 -or
        (Get-Hash (Join-Path $root 'bmz-updater.exe')) -ne (Get-Hash (Join-Path $bridgeSource 'bmz-updater.exe'))) {
        throw 'The legacy bridge did not replace the installed helper'
    }
    [IO.File]::WriteAllText((Join-Path $root $retired), 'user modified managed DLL')
    $oldFiles = @{}
    foreach ($entry in @($installed.files.path) + @($legacyManifest)) {
        $oldFiles[$entry] = Get-Hash (Join-Path $root $entry)
    }

    $work = Join-Path $root 'updater\job-sxs'
    $stage = Join-Path $work 'stage'
    Copy-UpdateFixture $sxsSource $stage $true
    if ($broken) {
        $assembly = Join-Path $stage 'dll\dll.manifest'
        [xml]$xml = [IO.File]::ReadAllText($assembly)
        $xml.assembly.assemblyIdentity.version = '2.0.0.0'
        $xml.Save($assembly)
    }
    Write-Package $stage $version $true
    Invoke-Update $root $work 'bmz-updater.exe' (-not $broken)
    $result = Get-Content -LiteralPath (Join-Path $work 'result.json') -Raw | ConvertFrom-Json
    if ((Test-Path -LiteralPath (Join-Path $root 'updater\active.json')) -or $result.complete -eq $broken) {
        throw 'Unexpected update journal state'
    }
    if ($broken) {
        foreach ($entry in $oldFiles.GetEnumerator()) {
            if ((Get-Hash (Join-Path $root $entry.Key)) -ne $entry.Value) { throw "Rollback changed $($entry.Key)" }
        }
        if ((Test-Path -LiteralPath (Join-Path $root $sxsManifest)) -or
            (Test-Path -LiteralPath (Join-Path $root 'dll\dll.manifest'))) { throw 'Rollback left the new package active' }
        if ((Get-Content -LiteralPath (Join-Path $work 'update.log') -Raw) -notmatch 'SxS loader check') {
            throw 'Rollback was not caused by the loader check'
        }
    } else {
        $installed = Get-Content -LiteralPath (Join-Path $root $sxsManifest) -Raw | ConvertFrom-Json
        if ($installed.version -ne $version -or $installed.min_updater_protocol -ne 4) { throw 'Wrong installed package' }
        for ($i = 0; $i -lt $result.operations.Count; $i++) {
            $op = $result.operations[$i]
            if ($op.path.Contains('/') -or $op.path -notlike '*.dll') { continue }
            if ($op.install -or (Test-Path -LiteralPath (Join-Path $root $op.path)) -or
                (Get-Hash (Join-Path $work "backup\$i")) -ne $oldFiles[$op.path]) {
                throw "Managed root DLL was not retired correctly: $($op.path)"
            }
        }
        if ((Get-Content -LiteralPath (Join-Path $work 'startup-version.txt') -Raw) -notmatch "^bmz-player $([regex]::Escape($version)) ") {
            throw 'Missing successful SxS loader check'
        }
        foreach ($name in $oldFiles.Keys) {
            if (($name -notlike '*/*' -and $name -like '*.dll') -or $name -in @($legacyManifest, 'bmz-updater.exe')) {
                if (Test-Path -LiteralPath (Join-Path $root $name)) { throw "Retired file was left at the root: $name" }
            }
        }
    }
    foreach ($entry in $userFiles.GetEnumerator()) {
        if ([IO.File]::ReadAllText((Join-Path $root $entry.Key)) -cne $entry.Value) {
            throw "Changed user file: $($entry.Key)"
        }
    }
    Write-Host "PASS: protocol $OldProtocol -> flat bridge -> SxS $case"
}
