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
$previousRustflags = $env:RUSTFLAGS
$previousEncodedRustflags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location (Join-Path $PSScriptRoot "..")
try {
    # vcpkg-rs 0.2.15 does not infer the MSVC ABI from the win7 target name.
    if (-not $env:VCPKGRS_TRIPLET) {
        $env:VCPKGRS_TRIPLET = "x64-windows"
    }
    # Rebuild std for both targets with the same compiler for A/B measurements.
    $arguments = @("+$Toolchain", "build", "-Z", "build-std=std,panic_abort",
        "--locked", "--target", $Target)
    if ($Profile -eq "Release") {
        $arguments += "--release"
    }
    & cargo @arguments -p bmz-player
    if ($LASTEXITCODE -ne 0) {
        throw "Windows compatibility build failed with exit code $LASTEXITCODE"
    }
    # The grouped updater must also start without a centrally installed VC/UCRT.
    if ($previousEncodedRustflags) {
        $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustflags + [char]31 + '-Ctarget-feature=+crt-static'
    } else {
        $env:RUSTFLAGS = "$previousRustflags -Ctarget-feature=+crt-static".Trim()
    }
    & cargo @arguments -p bmz-updater
    if ($LASTEXITCODE -ne 0) {
        throw "Windows updater compatibility build failed with exit code $LASTEXITCODE"
    }
} finally {
    $env:VCPKGRS_TRIPLET = $previousTriplet
    $env:RUSTFLAGS = $previousRustflags
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustflags
    Pop-Location
}
