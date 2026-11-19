#Requires -Version 4.0
param(
    [Parameter(Mandatory)][string]$Packages,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipRendering,
    [switch]$SkipPlay,
    [switch]$RequireAppLocalUcrt,
    [string]$BgaChart
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Run in 64-bit PowerShell' }
$packagesRoot = (Resolve-Path -LiteralPath $Packages).Path
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
if ((Test-Path -LiteralPath $output) -or $output.StartsWith("$packagesRoot\", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a fresh output directory outside the input packages'
}
if ($BgaChart) { $BgaChart = (Resolve-Path -LiteralPath $BgaChart).Path }
New-Item -ItemType Directory -Path $output | Out-Null
$foreign = Join-Path $output 'empty-working-directory'
New-Item -ItemType Directory -Path $foreign | Out-Null
$assembly = 'dll'
$assemblyManifest = "$assembly.manifest"
$exeName = 'bmz-player.exe'
$marker = 'updater\bmz-package.json'
$system32 = [Environment]::SystemDirectory
$cleanPath = "$system32;$env:WINDIR;$system32\Wbem"
$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$os | Select-Object ProductName, CurrentVersion, DisplayVersion, CurrentBuild, UBR, BuildLabEx |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'os.json') -Encoding UTF8
$modernUcrt = [int]$os.CurrentBuild -ge 10240
$centralUcrt = Test-Path -LiteralPath (Join-Path $system32 'ucrtbase.dll')
if ($RequireAppLocalUcrt -and
    ([int]$os.CurrentBuild -ne 9600 -or $centralUcrt -or (Test-Path -LiteralPath (Join-Path $system32 'vcruntime140.dll')))) {
    throw 'The app-local UCRT gate requires Windows 8.1 x64 without central UCRT or VC runtime'
}
$started = Get-Date

function File-Evidence([string]$Path) {
    $item = Get-Item -LiteralPath $Path
    [pscustomobject]@{
        name = $item.Name
        path = $item.FullName
        version = $item.VersionInfo.FileVersion
        product_version = $item.VersionInfo.ProductVersion
        sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    }
}
$central = @(foreach ($name in @('ucrtbase.dll', 'vcruntime140.dll', 'api-ms-win-crt-runtime-l1-1-0.dll')) {
    $path = Join-Path $system32 $name
    if (Test-Path -LiteralPath $path) { File-Evidence $path }
})
ConvertTo-Json -InputObject $central | Set-Content -LiteralPath (Join-Path $output 'system-runtime.json') -Encoding UTF8
Get-HotFix -ErrorAction SilentlyContinue | Select-Object HotFixID, InstalledOn |
    Export-Csv -LiteralPath (Join-Path $output 'hotfixes.csv') -NoTypeInformation -Encoding UTF8

if (-not ('BmzPoC.Native' -as [type])) {
    Add-Type -Namespace BmzPoC -Name Native -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint SetErrorMode(uint mode);
'@
}
$results = New-Object 'Collections.Generic.List[object]'
$failures = New-Object 'Collections.Generic.List[string]'
$required = 'avcodec-*.dll', 'avformat-*.dll', 'avutil-*.dll', 'swresample-*.dll', 'swscale-*.dll', 'vcruntime140.dll'

function Check([bool]$Passed, [string]$Message) {
    if (-not $Passed) { $failures.Add($Message) }
}
function Copy-Pristine([string]$Label, [string]$Destination) {
    $source = Join-Path $packagesRoot $Label
    if (Test-Path -LiteralPath (Join-Path $source 'data')) { throw "Expected pristine input: $source" }
    & robocopy $source $Destination /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP | Out-Host
    if ($LASTEXITCODE -gt 7) { throw "Could not copy $Label" }
}
function Copy-SxsBinaries([string]$Destination) {
    $source = Join-Path $packagesRoot 'sxs'
    New-Item -ItemType Directory -Path $Destination | Out-Null
    Copy-Item -LiteralPath (Join-Path $source $exeName) -Destination $Destination
    Copy-Item -LiteralPath (Join-Path $source $assembly) -Destination $Destination -Recurse
}
function Run-Probe([string]$Label, [string]$Root, [string[]]$PlayerArgs, [switch]$Modules, [string]$Executable = $exeName) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = Join-Path $Root $Executable
    $info.Arguments = ($PlayerArgs | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $info.WorkingDirectory = $foreign
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    foreach ($key in @($info.EnvironmentVariables.Keys)) {
        if ($key -like 'BMZ_*' -or $key -like 'VK_*') { $info.EnvironmentVariables.Remove($key) }
    }
    $info.EnvironmentVariables['PATH'] = $cleanPath
    $info.EnvironmentVariables['APPDATA'] = Join-Path $output "appdata\$Label\roaming"
    $info.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $output "appdata\$Label\local"
    $info.EnvironmentVariables['RUST_LOG'] = 'info,bmz_video::ffmpeg_decode=debug'
    $info.EnvironmentVariables['BMZ_LATENCY_DIAGNOSTICS'] = '1'
    $modulePaths = @{}
    $result = [pscustomobject]@{
        label = $Label; executable = $info.FileName; arguments = $info.Arguments
        cwd = $foreign; path = $cleanPath; exit_code = $null; exit_hex = $null
        start_error = $null; message = $null; timed_out = $false; modules = @()
    }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $info
    try {
        try {
            [void]$process.Start()
        } catch {
            $cause = $_.Exception.GetBaseException()
            if ($cause -isnot [ComponentModel.Win32Exception]) { throw }
            $result.start_error = $cause.NativeErrorCode
            $result.message = $cause.Message
            return $result
        }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $timer = [Diagnostics.Stopwatch]::StartNew()
        while (-not $process.HasExited) {
            if ($Modules) {
                try {
                    $process.Refresh()
                    foreach ($module in $process.Modules) { $modulePaths[$module.FileName] = $true }
                } catch [InvalidOperationException], [ComponentModel.Win32Exception] {
                    # Early snapshots may race loader initialization or process exit.
                    $result.message = $_.Exception.Message
                }
            }
            if ($timer.Elapsed.TotalSeconds -ge 55 -and -not $process.HasExited) {
                $result.timed_out = $true
                $process.Kill()
                break
            }
            [void]$process.WaitForExit(25)
        }
        $process.WaitForExit()
        $result.exit_code = $process.ExitCode
        $result.exit_hex = '0x{0:X8}' -f $process.ExitCode
        $stdout.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $output "$Label.stdout.txt") -Encoding UTF8
        $stderr.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $output "$Label.stderr.txt") -Encoding UTF8
        if ($Modules) {
            $modulePaths.Keys | Sort-Object | Set-Content -LiteralPath (Join-Path $output "$Label.all-modules.txt") -Encoding UTF8
            $result.modules = @(foreach ($path in ($modulePaths.Keys | Sort-Object)) {
                if ([IO.Path]::GetFileName($path) -match '^(?:av\w+-\d+|swresample-\d+|swscale-\d+|pkgconf-\d+|vcruntime\w*|msvcp\w*|ucrtbase|api-ms-win-.+)\.dll$') {
                    File-Evidence $path
                }
            })
            $result.modules | Export-Csv -LiteralPath (Join-Path $output "$Label.modules.csv") -NoTypeInformation -Encoding UTF8
        }
        return $result
    } finally {
        $results.Add($result)
        Write-Host "$Label -> exit $($result.exit_hex), CreateProcess error $($result.start_error), runtime modules $($result.modules.Count)"
        $process.Dispose()
    }
}
function Check-Modules($Probe, [string]$DllDirectory, [string]$VcDirectory) {
    foreach ($pattern in $required) {
        $loaded = @($Probe.modules | Where-Object name -Like $pattern)
        Check ($loaded.Count -eq 1) "$($Probe.label): missing/duplicate module $pattern"
        foreach ($module in $loaded) {
            $directory = if ($pattern -eq 'vcruntime140.dll') { $VcDirectory } else { $DllDirectory }
            Check ($module.path -eq (Join-Path $directory $module.name)) "$($Probe.label): wrong DLL origin $($module.path)"
        }
    }
    $ucrt = @($Probe.modules | Where-Object name -EQ 'ucrtbase.dll')
    Check ($ucrt.Count -eq 1) "$($Probe.label): missing/duplicate ucrtbase.dll"
    if ($ucrt.Count -eq 1 -and $modernUcrt) {
        Check ($ucrt[0].path -eq (Join-Path $system32 'ucrtbase.dll')) "$($Probe.label): unexpected modern system UCRT path $($ucrt[0].path)"
    }
    if (-not $modernUcrt -and -not $centralUcrt) {
        Check ($ucrt.Count -eq 1 -and $ucrt[0].path -eq (Join-Path $VcDirectory 'ucrtbase.dll')) "$($Probe.label): app-local UCRT did not load"
        $forwarders = @($Probe.modules | Where-Object name -Like 'api-ms-win-crt-*.dll')
        Check ($forwarders.Count -gt 0) "$($Probe.label): no app-local CRT forwarders observed"
        foreach ($module in $forwarders) {
            Check ($module.path -eq (Join-Path $VcDirectory $module.name)) "$($Probe.label): unexpected CRT forwarder $($module.path)"
        }
    }
}
function Check-Text([string]$Label, [string]$Stream, [string]$Pattern) {
    $file = Join-Path $output "$Label.$Stream.txt"
    Check ((Test-Path -LiteralPath $file) -and ([IO.File]::ReadAllText($file) -match $Pattern)) "$Label ${Stream}: expected $Pattern"
}

$previousErrorMode = [BmzPoC.Native]::SetErrorMode(0x8003)
try {
    $layouts = if (Test-Path -LiteralPath (Join-Path $packagesRoot 'ffmpeg-only')) {
        @('baseline', 'ffmpeg-only', 'sxs')
    } else { @('baseline', 'sxs') }
    foreach ($layout in $layouts) {
        $case = Join-Path $output $layout
        $root = Join-Path $case ("BMZ Player " + [char]0x65E5 + [char]0x672C + [char]0x8A9E)
        Copy-Pristine $layout $root
        $dllDir = if ($layout -eq 'baseline') { $root } else { Join-Path $root $assembly }
        $vcDir = if ($layout -eq 'sxs') { $dllDir } else { $root }
        if ($layout -eq 'sxs' -and @(Get-ChildItem -LiteralPath $root -Filter '*.dll' -File).Count -ne 0) {
            throw 'Clean SxS package has root DLLs'
        }
        foreach ($command in @('help', 'version')) {
            $p = Run-Probe "$layout-$command" $root @("--$command")
            Check ($p.exit_code -eq 0) "$($p.label): CLI failed"
            Check-Text $p.label stdout 'bmz-player'
        }
        $p = Run-Probe "$layout-invalid-cli" $root @('--gauge', 'invalid')
        Check ($p.exit_code -eq 1) "$($p.label): invalid CLI did not exit 1"
        Check-Text $p.label stderr 'invalid --gauge'
        $helper = if (Test-Path -LiteralPath (Join-Path $root 'updater\bmz-updater.exe')) {
            'updater\bmz-updater.exe'
        } else { 'bmz-updater.exe' }
        $p = Run-Probe "$layout-updater" $root @('--help') -Executable $helper
        Check ($p.exit_code -eq 0) "$($p.label): updater CLI failed"
        $p = Run-Probe "$layout-profile" $root @('profile', 'create', 'sxs-poc', '--activate') -Modules:$SkipRendering
        Check ($p.exit_code -eq 0) "$($p.label): profile creation failed"
        $config = Join-Path $root 'data\config.toml'
        Check (Test-Path -LiteralPath (Join-Path $root 'data\profiles\sxs-poc\profile.toml')) "$layout did not save adjacent profile"
        if ($SkipRendering) { Check-Modules $p $dllDir $vcDir }
        if (-not $SkipRendering -and (Test-Path -LiteralPath $config)) {
            $originalConfig = [IO.File]::ReadAllText($config).Replace('check_on_startup = true', 'check_on_startup = false')
            [IO.File]::WriteAllText($config, $originalConfig)
            $p = Run-Probe "$layout-select" $root @('--smoke-exit-after-frames', '120') -Modules
            Check ($p.exit_code -eq 0) "$($p.label): Select startup failed"
            Check-Text $p.label stderr 'smoke exit frame count reached'
            Check-Modules $p $dllDir $vcDir
            $rawInput = $originalConfig -replace 'gamepad_backend = "[^"]+"', 'gamepad_backend = "RawInput"'
            [IO.File]::WriteAllText($config, $rawInput)
            $p = Run-Probe "$layout-result" $root @('--boot-result-sample', '--smoke-exit-after-result-frames', '120') -Modules
            Check ($p.exit_code -eq 0) "$($p.label): Result smoke failed"
            Check-Text $p.label stderr 'smoke result frame count reached'
            Check-Text $p.label stderr 'backend="Raw Input"'
            Check-Modules $p $dllDir $vcDir
            if (-not $SkipPlay -and $layout -in @('baseline', 'sxs')) {
                $p = Run-Probe "$layout-play" $root @('--boot-play-sample', '--autoplay-on-start', '--smoke-exit-after-play-frames', '240')
                Check ($p.exit_code -eq 0) "$($p.label): sample play failed"
                Check-Text $p.label stderr 'smoke play frame count reached'
                Check-Text $p.label stderr 'decoded_sources=[1-9]'
                Check-Text $p.label stderr '"rendered_frames":[1-9]'
                if ($BgaChart) {
                    $p = Run-Probe "$layout-bga" $root @($BgaChart, '--autoplay-on-start', '--smoke-exit-on-result')
                    Check ($p.exit_code -eq 0) "$($p.label): video BGA fixture failed"
                    Check-Text $p.label stderr 'opened video BGA decoder'
                    Check-Text $p.label stderr 'configured video (?:RGBA scaler|YUV420)'
                    Check-Text $p.label stderr 'smoke result reached'
                }
            }
        }
        if ($layout -eq 'sxs') {
            $moved = Join-Path $output 'relocated BMZ Player'
            Move-Item -LiteralPath $root -Destination $moved
            $p = Run-Probe 'sxs-relocated-profile' $moved @('profile', 'create', 'moved-poc')
            Check ($p.exit_code -eq 0) 'Relocated CLI failed'
            Check (Test-Path -LiteralPath (Join-Path $moved 'data\profiles\sxs-poc\profile.toml')) 'Moved profile not preserved'
            Check (Test-Path -LiteralPath (Join-Path $moved 'data\profiles\moved-poc\profile.toml')) 'Moved CLI did not use relocated data'
            if (-not $SkipRendering) {
                $p = Run-Probe 'sxs-relocated-result' $moved @('--boot-result-sample', '--smoke-exit-after-result-frames', '120') -Modules
                Check ($p.exit_code -eq 0) 'Relocated Result failed'
                $movedDlls = Join-Path $moved $assembly
                Check-Modules $p $movedDlls $movedDlls
            }
        }
    }

    $faults = @('no-manifest', 'identity-mismatch', 'no-avcodec')
    if ($RequireAppLocalUcrt) { $faults += 'no-ucrt' }
    foreach ($fault in $faults) {
        $root = Join-Path $output $fault
        Copy-SxsBinaries $root
        $dllDir = Join-Path $root $assembly
        $manifest = Join-Path $dllDir $assemblyManifest
        switch ($fault) {
            'no-manifest' { Remove-Item -LiteralPath $manifest }
            'identity-mismatch' {
                [xml]$xml = [IO.File]::ReadAllText($manifest)
                $xml.assembly.assemblyIdentity.version = '2.0.0.0'
                $xml.Save($manifest)
            }
            'no-avcodec' {
                $codec = @(Get-ChildItem -LiteralPath $dllDir -Filter 'avcodec-*.dll' -File)
                if ($codec.Count -ne 1) { throw 'Expected exactly one avcodec DLL' }
                Remove-Item -LiteralPath $codec[0].FullName
            }
            'no-ucrt' { Remove-Item -LiteralPath (Join-Path $dllDir 'ucrtbase.dll') }
        }
        $p = Run-Probe $fault $root @('--help')
        $diagnostic = if ($fault -in @('no-avcodec', 'no-ucrt')) {
            $p.exit_hex -eq '0xC0000135' -or $p.start_error -eq 126
        } else {
            $p.start_error -eq 14001 -or $p.exit_hex -eq '0xC0150002'
        }
        Check ($diagnostic -and -not $p.timed_out) "${fault}: missing expected loader failure"
    }

    $sxs = Join-Path $packagesRoot 'sxs'
    $probeMarker = [IO.File]::ReadAllText((Join-Path $sxs $marker)) | ConvertFrom-Json
    $root = Join-Path $output 'installer-marker'
    Copy-SxsBinaries $root
    New-Item -ItemType Directory -Path (Join-Path $root 'updater') | Out-Null
    $probeMarker.kind = 'installer'
    [IO.File]::WriteAllText((Join-Path $root $marker), ($probeMarker | ConvertTo-Json -Depth 5))
    $p = Run-Probe 'installer-profile' $root @('profile', 'create', 'sxs-poc')
    Check ($p.exit_code -eq 0) 'Installer marker profile did not start'
    $roamingProfile = Join-Path $output 'appdata\installer-profile\roaming\BMZ Player\profiles\sxs-poc\profile.toml'
    Check (Test-Path -LiteralPath $roamingProfile) 'Installer profile did not use isolated roaming directory'
    Check (-not (Test-Path -LiteralPath (Join-Path $root 'data'))) 'Installer marker created adjacent data'

    if ($probeMarker.min_updater_protocol -lt 4) {
        $probeMarker.kind = 'portable'
        $fullInventory = [IO.File]::ReadAllText((Join-Path $packagesRoot 'sxs.files.json')) | ConvertFrom-Json
        # Windows PowerShell wraps a top-level JSON array. Materialize a plain array for serialization.
        $probeMarker.files = @(foreach ($entry in $fullInventory) { $entry })
        [IO.File]::WriteAllText((Join-Path $root $marker), ($probeMarker | ConvertTo-Json -Depth 5))
        $p = Run-Probe 'full-inventory-profile' $root @('profile', 'create', 'sxs-poc')
        Check ($p.exit_code -eq 1) 'Current app accepted runtime paths in full package inventory'
        Check-Text $p.label stderr 'package must not manage user files'
    }

    $root = Join-Path $output 'overlay'
    Copy-Pristine 'baseline' $root
    Copy-Pristine 'sxs' $root
    $staleRootDlls = @(Get-ChildItem -LiteralPath $root -Filter '*.dll' -File)
    $staleRootDlls.Name | Set-Content -LiteralPath (Join-Path $output 'overlay-root-dlls.txt') -Encoding UTF8
    Check ($staleRootDlls.Count -gt 0) 'Overlay did not retain old DLLs'
    $p = Run-Probe 'overlay-profile' $root @('profile', 'create', 'sxs-poc') -Modules
    Check ($p.exit_code -eq 0) 'Overlay startup failed'
    $overlayDlls = Join-Path $root $assembly
    Check-Modules $p $overlayDlls $overlayDlls

    Check (@(Get-ChildItem -LiteralPath $foreign -Force).Count -eq 0) 'Unexpected files in working directory'
} finally {
    [void][BmzPoC.Native]::SetErrorMode($previousErrorMode)
    ConvertTo-Json -InputObject $results.ToArray() -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding UTF8
    $failures | Set-Content -LiteralPath (Join-Path $output 'failures.txt') -Encoding UTF8
    Get-WinEvent -FilterHashtable @{LogName = 'Application'; ProviderName = 'SideBySide'; StartTime = $started} -ErrorAction SilentlyContinue |
        Where-Object { $_.Message -like "*$output*" } | Format-List TimeCreated, Id, Message |
        Out-File -LiteralPath (Join-Path $output 'side-by-side-events.txt') -Encoding UTF8
}
if ($failures.Count) { throw "$($failures.Count) checks failed. See $output\failures.txt" }
