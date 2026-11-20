param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [string]$VcRuntimeDirectory = "",
    [string]$UcrtDirectory = "",
    [switch]$RejectDuplicates
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$destinationPath = (Resolve-Path -LiteralPath $Destination).Path
if (-not $VcRuntimeDirectory) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    $installation = & $vswhere -latest -products '*' -version '[17.0,18.0)' -property installationPath
    if (-not $installation) { throw "Visual Studio 2022 is required for the Windows 8.1 runtime" }
    $redist = Get-ChildItem -LiteralPath (Join-Path $installation 'VC\Redist\MSVC') -Directory |
        Where-Object { $_.Name -match '^14\.\d+\.\d+$' -and ([version]$_.Name).Minor -le 44 } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (-not $redist) { throw 'The VS 2022 v143 redistributable files are missing' }
    $VcRuntimeDirectory = Join-Path $redist.FullName "x64\Microsoft.VC143.CRT"
}
if (-not $UcrtDirectory) {
    $UcrtDirectory = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\Redist\ucrt\DLLs\x64"
}
$vc = foreach ($name in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')) {
    $dll = Get-Item -LiteralPath (Join-Path $VcRuntimeDirectory $name)
    if ($dll.VersionInfo.FileMajorPart -ne 14 -or $dll.VersionInfo.FileMinorPart -gt 44) {
        throw "Use the VS 2022 v143 runtime, not a newer Windows 10-only runtime: $($dll.Name) $($dll.VersionInfo.FileVersion)"
    }
    $dll
}
$ucrt = Get-Item -LiteralPath (Join-Path $UcrtDirectory "ucrtbase.dll")
if ($ucrt.VersionInfo.FileMajorPart -ne 10 -or $ucrt.VersionInfo.FileBuildPart -gt 14393) {
    throw "Windows 8.1 packaging needs the legacy SDK app-local UCRT (build <=14393): $($ucrt.VersionInfo.FileVersion)"
}
if (-not (Test-Path -LiteralPath (Join-Path $UcrtDirectory "api-ms-win-crt-runtime-l1-1-0.dll"))) {
    throw "The complete app-local UCRT set is required"
}
$dlls = @($vc) + @(Get-ChildItem -LiteralPath $UcrtDirectory -Filter '*.dll' -File)
$inventory = foreach ($dll in $dlls) {
    if ($RejectDuplicates -and (Test-Path -LiteralPath (Join-Path $destinationPath $dll.Name))) {
        throw "Duplicate DLL in SxS input: $($dll.Name)"
    }
    Copy-Item -LiteralPath $dll.FullName -Destination $destinationPath -Force
    [ordered]@{name = $dll.Name; version = $dll.VersionInfo.FileVersion; sha256 = (Get-FileHash -LiteralPath $dll.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$licenses = Join-Path $destinationPath "resources\licenses"
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
$inventory | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $licenses "windows-runtime.json") -Encoding utf8NoBOM
@'
Microsoft runtime components
Copyright (c) Microsoft Corporation. All rights reserved.
Unmodified x64 redistributable binaries from Visual Studio 2022 and the Windows SDK.
https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files
https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment
Windows 10/11 use the system UCRT even when an app-local copy is bundled.
'@ | Set-Content -LiteralPath (Join-Path $licenses "windows-runtime-notice.txt") -Encoding utf8NoBOM
Write-Host "Bundled $($dlls.Count) VC/UCRT DLLs; VC $($vc[0].VersionInfo.FileVersion), UCRT $($ucrt.VersionInfo.FileVersion)"
