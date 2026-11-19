param([Parameter(Mandatory = $true)][string]$PackageDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products '*' -version '[17.0,18.0)' -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Sort-Object -Descending | Select-Object -First 1
if (-not $dumpbin) { throw 'MSVC dumpbin is required for compatibility import checks' }
$exports = @{}
function Read-Exports([string]$Name) {
    if (-not $exports.ContainsKey($Name)) {
        $symbols = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        foreach ($line in (& $dumpbin /exports (Join-Path $package $Name))) {
            if ($line -match '^\s+\d+\s+[0-9A-F]+\s+(?:[0-9A-F]{8}\s+)?([^\s]+)(?:\s+.*)?$') { [void]$symbols.Add($Matches[1]) }
        }
        if ($LASTEXITCODE -ne 0) { throw "Could not read exports: $Name" }
        $exports[$Name] = $symbols
    }
    return ,$exports[$Name]
}
$updater = Join-Path $package 'updater\bmz-updater.exe'
if (-not (Test-Path -LiteralPath $updater)) { $updater = Join-Path $package 'bmz-updater.exe' }
$files = @((Get-Item -LiteralPath (Join-Path $package 'bmz-player.exe')), (Get-Item -LiteralPath $updater)) + @(Get-ChildItem -LiteralPath $package -Filter '*.dll' -File)
$runtimePattern = '^(?:vcruntime|msvcp|concrt|vccorlib|vcomp|ucrtbase|api-ms-win-crt-)'
$checked = 0
foreach ($file in $files) {
    $module = $null
    foreach ($line in (& $dumpbin /imports $file.FullName)) {
        if ($line -match '^    ([^\s]+\.dll)\s*$') {
            $module = $Matches[1]
            if ($file.FullName -eq $updater -and $module -match '^(?:api-ms-win-crt-|vcruntime|ucrtbase|msvcp)') { throw "Updater needs an external CRT: $module" }
            $bundled = Test-Path -LiteralPath (Join-Path $package $module)
            # CI hosts have the VC redistributable in System32, but clean Windows 8.1 does not.
            if (-not $bundled -and $file.FullName -ne $updater -and $module -match $runtimePattern) { throw "Runtime DLL must be bundled: $($file.Name) -> $module" }
            if (-not $bundled -and $module -notlike 'api-ms-win-*' -and $module -notlike 'ext-ms-win-*' -and -not (Test-Path -LiteralPath (Join-Path $env:WINDIR "System32\$module"))) { throw "Missing package dependency: $($file.Name) -> $module" }
        } elseif ($module -and $line -match '^\s+[0-9A-F]+\s+([A-Za-z_?@][^\s]*)\s*$') {
            $symbol = $Matches[1]
            if ($module -match '^(?:kernel32|user32)\.dll$' -and $symbol -in @('SetThreadDescription','GetThreadDescription','GetDpiForWindow','GetDpiForSystem','SetThreadDpiAwarenessContext','IsWow64Process2','GetTempPath2W','GetTempPath2A')) { throw "Windows 10+ static import: $($file.Name) -> $symbol" }
            if (Test-Path -LiteralPath (Join-Path $package $module)) {
                if (-not (Read-Exports $module).Contains($symbol)) { throw "Missing DLL export: $($file.Name) -> ${module}!$symbol" }
                $checked++
            }
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "Could not read imports: $($file.Name)" }
}
Write-Host "Checked $checked bundled DLL imports; updater is CRT-independent. This is not Windows 8.1 runtime verification."
