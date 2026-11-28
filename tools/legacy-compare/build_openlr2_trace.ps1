param(
    [string]$SourceDirectory = 'D:\Tools\openlr2-research',
    [string]$BuildToolsDirectory = 'D:\Tools\bmz-build',
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo '.local\openlr2-parity-trace' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$sourceRoot = [IO.Path]::GetFullPath($SourceDirectory)
if ($OutputDirectory.StartsWith($sourceRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Output must be separate from the original OpenLR2 source and build tree'
}
$buildRoot = Join-Path $sourceRoot 'out\windows-vs-x64'
$project = Join-Path $buildRoot 'OpenLR2.vcxproj'
[xml]$projectXml = Get-Content -LiteralPath $project -Raw
$group = @($projectXml.Project.ItemDefinitionGroup | Where-Object { $_.Condition -like '*RelWithDebInfo|x64*' })[0]
if (-not $group) { throw 'RelWithDebInfo x64 build metadata is required' }
& python (Join-Path $PSScriptRoot 'prepare_openlr2_trace.py') --source (Join-Path $sourceRoot 'LR2\Scene04_Play.cpp') --output $OutputDirectory
if ($LASTEXITCODE -ne 0) { throw 'Trace overlay preparation failed' }
$env:VSCMD_SKIP_SENDTELEMETRY = '1'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$vcvars = Join-Path $BuildToolsDirectory 'vs\VC\Auxiliary\Build\vcvars64.bat'
$vsEnv = & cmd.exe /d /s /c ('"' + $vcvars + '" >nul && set')
if ($LASTEXITCODE -ne 0) { throw 'Visual C++ setup failed' }
foreach ($entry in $vsEnv) {
    if ($entry -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$compileOptions = @('/nologo','/c','/std:c++latest','/EHsc','/MT','/O2','/utf-8','/wd4244','/DWIN32','/D_WINDOWS',
    '/DNDEBUG','/DNOMINMAX','/DWIN32_LEAN_AND_MEAN','/DDX_LIB_NOT_DEFAULTPATH','/D_CRT_SECURE_NO_WARNINGS',
    ('/I' + (Join-Path $sourceRoot 'LR2')))
foreach ($include in ([string]$group.ClCompile.AdditionalIncludeDirectories).Split(';')) {
    if ($include -and -not $include.Contains('%(')) { $compileOptions += '/I' + $include }
}
foreach ($match in [regex]::Matches([string]$group.ClCompile.AdditionalOptions, '/external:I\s+"([^"]+)"')) {
    $compileOptions += '/I' + $match.Groups[1].Value
}
Push-Location $OutputDirectory
try {
    & cl.exe @compileOptions '/FoScene04_Play.obj' 'Scene04_Play.cpp'
    if ($LASTEXITCODE -ne 0) { throw 'Instrumented Scene04_Play compilation failed' }
    & cl.exe @compileOptions '/FoMain.obj' (Join-Path $sourceRoot 'LR2\Main.cpp')
    if ($LASTEXITCODE -ne 0) { throw 'Main compilation failed' }
    $linkOptions = @('/NOLOGO','/SUBSYSTEM:WINDOWS','/MACHINE:X64','/INCREMENTAL:NO',
        '/OUT:OpenLR2_ParityTrace_x64.exe','Scene04_Play.obj','Main.obj')
    # Main keeps the large game structure on the stack. Preserve the original
    # project's reserve; the linker default (1 MiB) crashes before startup.
    $stackReserve = [string]$group.Link.StackReserveSize
    if (-not $stackReserve -or $stackReserve -notmatch '^\d+$') { throw 'Original stack reserve is missing from build metadata' }
    $linkOptions += '/STACK:' + $stackReserve
    # The original manifest enables the real Windows version and UTF-8 paths.
    $linkOptions += '/MANIFEST:EMBED'
    foreach ($manifest in ([string]$group.Manifest.AdditionalManifestFiles).Split(';')) {
        if (-not $manifest -or $manifest.Contains('%(')) { continue }
        if (-not (Test-Path -LiteralPath $manifest)) { throw "Original manifest is missing: $manifest" }
        $linkOptions += '/MANIFESTINPUT:' + $manifest
    }
    foreach ($dependency in ([string]$group.Link.AdditionalDependencies).Split(';')) {
        if (-not $dependency -or $dependency.Contains('%(')) { continue }
        if (-not [IO.Path]::IsPathRooted($dependency) -and $dependency.Contains('\')) { $dependency = Join-Path $buildRoot $dependency }
        $linkOptions += $dependency
    }
    $resource = Join-Path $buildRoot 'OpenLR2.dir\RelWithDebInfo\OpenLR2.res'
    if (Test-Path -LiteralPath $resource) { $linkOptions += $resource }
    & link.exe @linkOptions
    if ($LASTEXITCODE -ne 0) { throw 'Trace executable link failed' }
    $fmod = Join-Path $buildRoot 'RelWithDebInfo\fmod.dll'
    if (Test-Path -LiteralPath $fmod) { Copy-Item -LiteralPath $fmod -Destination (Join-Path $OutputDirectory 'fmod.dll') -Force }
    Get-Item -LiteralPath (Join-Path $OutputDirectory 'OpenLR2_ParityTrace_x64.exe') | Select-Object FullName,Length
} finally { Pop-Location }
