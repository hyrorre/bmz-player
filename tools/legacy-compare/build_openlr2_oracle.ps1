param(
    [string]$SourceDirectory = 'D:\Tools\openlr2-research',
    [string]$BuildToolsDirectory = 'D:\Tools\bmz-build',
    [string]$OutputDirectory
)
# Builds the headless OpenLR2 judgement oracle against the already built
# OpenLR2Lib of $SourceDirectory. The OpenLR2 source and libraries are only
# read; nothing in that tree is modified.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo '.local\openlr2-oracle' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$sourceRoot = [IO.Path]::GetFullPath($SourceDirectory)
if ($OutputDirectory.StartsWith($sourceRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Output must be separate from the original OpenLR2 source and build tree'
}
$buildRoot = Join-Path $sourceRoot 'out\windows-vs-x64'
# The isolated RianMod integration test is a console program linked against
# OpenLR2Lib; reuse its exact include paths and link inputs.
[xml]$projectXml = Get-Content -LiteralPath (Join-Path $buildRoot 'RianModIntegration.vcxproj') -Raw
$group = @($projectXml.Project.ItemDefinitionGroup | Where-Object { $_.Condition -like '*RelWithDebInfo|x64*' })[0]
if (-not $group) { throw 'RelWithDebInfo x64 build metadata is required' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

$env:VSCMD_SKIP_SENDTELEMETRY = '1'
$vcvars = Join-Path $BuildToolsDirectory 'vs\VC\Auxiliary\Build\vcvars64.bat'
$vsEnv = & cmd.exe /d /s /c ('"' + $vcvars + '" >nul && set')
if ($LASTEXITCODE -ne 0) { throw 'Visual C++ setup failed' }
foreach ($entry in $vsEnv) {
    if ($entry -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$compile = @('/nologo','/c','/std:c++latest','/EHsc','/MT','/O2','/utf-8','/wd4244','/DWIN32','/D_WINDOWS',
    '/DNDEBUG','/DNOMINMAX','/DWIN32_LEAN_AND_MEAN','/DDX_LIB_NOT_DEFAULTPATH','/D_CRT_SECURE_NO_WARNINGS',
    ('/I' + (Join-Path $sourceRoot 'LR2')))
foreach ($include in ([string]$group.ClCompile.AdditionalIncludeDirectories).Split(';')) {
    if ($include -and -not $include.Contains('%(')) { $compile += '/I' + $include }
}
foreach ($match in [regex]::Matches([string]$group.ClCompile.AdditionalOptions, '/external:I\s+"([^"]+)"')) {
    $compile += '/I' + $match.Groups[1].Value
}
Push-Location $OutputDirectory
try {
    # The oracle includes ../LR2/*.h relative to the OpenLR2 tests folder.
    $tests = Join-Path $OutputDirectory 'tests'
    New-Item -ItemType Directory -Force -Path $tests | Out-Null
    $oracleSource = Join-Path $tests 'openlr2_oracle.cpp'
    (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'openlr2_oracle.cpp') -Raw).Replace('"../LR2/', '"') |
        Set-Content -LiteralPath $oracleSource -Encoding utf8
    & cl.exe @compile '/Foopenlr2_oracle.obj' $oracleSource
    if ($LASTEXITCODE -ne 0) { throw 'Oracle compilation failed' }
    $link = @('/NOLOGO','/SUBSYSTEM:CONSOLE','/MACHINE:X64','/INCREMENTAL:NO','/OUT:openlr2_oracle.exe','openlr2_oracle.obj')
    $stackReserve = [string]$group.Link.StackReserveSize
    if ($stackReserve -match '^\d+$') { $link += '/STACK:' + $stackReserve }
    # UTF-8 active code page for chart paths, as in the game executable.
    $manifest = Join-Path $sourceRoot 'OpenLR2.manifest'
    if (Test-Path -LiteralPath $manifest) { $link += '/MANIFEST:EMBED'; $link += '/MANIFESTINPUT:' + $manifest }
    foreach ($dependency in ([string]$group.Link.AdditionalDependencies).Split(';')) {
        if (-not $dependency -or $dependency.Contains('%(')) { continue }
        if (-not [IO.Path]::IsPathRooted($dependency) -and $dependency.Contains('\')) { $dependency = Join-Path $buildRoot $dependency }
        $link += $dependency
    }
    & link.exe @link
    if ($LASTEXITCODE -ne 0) { throw 'Oracle link failed' }
    $fmod = Join-Path $buildRoot 'RelWithDebInfo\fmod.dll'
    if (Test-Path -LiteralPath $fmod) { Copy-Item -LiteralPath $fmod -Destination $OutputDirectory -Force }
    Get-Item -LiteralPath (Join-Path $OutputDirectory 'openlr2_oracle.exe') | Select-Object FullName, Length
} finally { Pop-Location }
