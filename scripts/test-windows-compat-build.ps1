# Check player/updater flag separation and restoration without building Rust.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$names = @('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','VCPKGRS_TRIPLET')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$state = [pscustomobject]@{Calls=[Collections.Generic.List[object]]::new(); FailAt=0}
Set-Item Function:cargo -Value {
    $state.Calls.Add([pscustomobject]@{Arguments=@($args); Rustflags=$env:RUSTFLAGS; Encoded=$env:CARGO_ENCODED_RUSTFLAGS})
    $global:LASTEXITCODE = if ($state.FailAt -eq $state.Calls.Count) { 1 } else { 0 }
}.GetNewClosure()
try {
    foreach ($case in @('plain','encoded','failure')) {
        $state.Calls.Clear()
        $state.FailAt = if ($case -eq 'failure') { 2 } else { 0 }
        $env:RUSTFLAGS = '--cfg compat_fixture'
        $env:CARGO_ENCODED_RUSTFLAGS = if ($case -eq 'plain') { $null } else { '--cfg' + [char]31 + 'compat_fixture' }
        $beforeEncoded = $env:CARGO_ENCODED_RUSTFLAGS
        $env:VCPKGRS_TRIPLET = 'compat-test-triplet'
        $failed = $false
        try { & (Join-Path $PSScriptRoot 'build-windows-compat.ps1') }
        catch { if ($case -ne 'failure') { throw }; $failed = $true }
        if ($failed -ne ($case -eq 'failure') -or $state.Calls.Count -ne 2) { throw "Unexpected cargo result: $case" }
        if ($state.Calls[0].Arguments -notcontains 'bmz-player' -or $state.Calls[1].Arguments -notcontains 'bmz-updater') { throw 'Wrong build targets' }
        if ($state.Calls[0].Rustflags -ne '--cfg compat_fixture' -or $state.Calls[0].Encoded -ne $beforeEncoded) { throw 'Player flags changed' }
        $updaterFlags = if ($beforeEncoded) { $state.Calls[1].Encoded } else { $state.Calls[1].Rustflags }
        if ($updaterFlags -notmatch '-Ctarget-feature=\+crt-static$') { throw 'Updater CRT is not static' }
        if ($env:RUSTFLAGS -ne '--cfg compat_fixture' -or $env:CARGO_ENCODED_RUSTFLAGS -ne $beforeEncoded -or $env:VCPKGRS_TRIPLET -ne 'compat-test-triplet') { throw 'Build environment was not restored' }
    }
    Write-Host 'Compatibility build flag and failure-path checks passed.'
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
$global:LASTEXITCODE = 0
