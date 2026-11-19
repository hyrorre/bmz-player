param(
    [ValidateSet("x86_64-win7-windows-msvc", "x86_64-pc-windows-msvc")]
    [string]$Target = "x86_64-win7-windows-msvc",
    [string]$Toolchain = "nightly",
    [ValidateSet("Debug", "Release")]
    [string]$Profile = "Release"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$previousTriplet = $env:VCPKGRS_TRIPLET
Push-Location (Join-Path $PSScriptRoot "..")
try {
    # vcpkg-rs 0.2.15 does not infer the MSVC ABI from the win7 target name.
    if (-not $env:VCPKGRS_TRIPLET) {
        $env:VCPKGRS_TRIPLET = "x64-windows"
    }
    # Rebuild std for both targets with the same compiler for A/B measurements.
    $arguments = @("+$Toolchain", "build", "-Z", "build-std=std,panic_abort",
        "--locked", "--target", $Target, "-p", "bmz-player", "-p", "bmz-updater")
    if ($Profile -eq "Release") {
        $arguments += "--release"
    }
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Windows compatibility build failed with exit code $LASTEXITCODE"
    }
} finally {
    $env:VCPKGRS_TRIPLET = $previousTriplet
    Pop-Location
}
