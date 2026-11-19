#Requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$PackageDirectory,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$Mt,
    [Parameter(Mandatory)][string]$ReadObj
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $PackageDirectory).Path
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$mtExe = (Get-Command $Mt -CommandType Application).Source
$readObjExe = (Get-Command $ReadObj -CommandType Application).Source
if ((Test-Path -LiteralPath $output) -or $output.StartsWith("$source\", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a fresh output directory outside the input package'
}
if (Test-Path -LiteralPath (Join-Path $source 'data')) { throw 'Use pristine portable staging' }
$markerPath = 'updater\bmz-package.json'
$marker = Get-Content -LiteralPath (Join-Path $source $markerPath) -Raw | ConvertFrom-Json
if ($marker.kind -ne 'portable' -or $marker.target -ne 'windows-x64') {
    throw 'Expected a grouped Windows x64 portable package'
}
& (Join-Path $PSScriptRoot '..\verify-windows-compat-imports.ps1') -PackageDirectory $source
$dlls = @(Get-ChildItem -LiteralPath $source -Filter '*.dll' -File | Sort-Object Name)
$runtime = @(Get-Content -LiteralPath (Join-Path $source 'resources\licenses\windows-runtime.json') -Raw | ConvertFrom-Json)
foreach ($name in @('ucrtbase.dll', 'vcruntime140.dll', 'api-ms-win-crt-runtime-l1-1-0.dll')) {
    if ($name -notin $runtime.name) { throw "Missing compatibility runtime inventory entry: $name" }
}
foreach ($entry in $runtime) {
    $path = Join-Path $source $entry.name
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.sha256) {
        throw "Compatibility runtime inventory mismatch: $($entry.name)"
    }
}
$ffmpeg = @($dlls | Where-Object Name -Match '^(avcodec|avdevice|avfilter|avformat|avutil|swresample|swscale)-\d+\.dll$')
if ($ffmpeg.Count -lt 5) { throw 'Missing FFmpeg DLLs' }

function Invoke-Mt([string[]]$Arguments) {
    & $mtExe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "mt.exe failed: $LASTEXITCODE ($Arguments)" }
}
function Read-Pe([string]$Executable) {
    & $readObjExe --file-headers --coff-resources $Executable
    if ($LASTEXITCODE -ne 0) { throw "llvm-readobj failed: $Executable" }
}
function Get-PeSignature([string[]]$Lines) {
    $inManifest = $false
    foreach ($line in $Lines) {
        if ($line -match '^  Type: ') { $inManifest = $line -match '\(ID 24\)' }
        if (-not $inManifest -and $line -match '^\s+(Type:|Name:|Language:|DataSize:|[0-9A-F]+: [0-9A-F])') { $line }
        if ($line -match '^\s+(?:Subsystem|\w+SubsystemVersion|\w+OperatingSystemVersion): ') { $line }
    }
}
function Get-CanonicalXml([xml]$Document) {
    $transform = [Security.Cryptography.Xml.XmlDsigC14NTransform]::new()
    $transform.LoadInput($Document)
    $reader = [IO.StreamReader]::new($transform.GetOutput())
    try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
}
function Copy-Package([string]$Label) {
    $destination = Join-Path $output $Label
    & robocopy $source $destination /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP | Out-Host
    if ($LASTEXITCODE -gt 7) { throw "Could not copy $Label" }
    return $destination
}
function Assert-Layout([string]$Root, [string[]]$MovedNames) {
    $allowed = @{}
    foreach ($dll in $dlls) {
        $path = $dll.Name
        if ($dll.Name -in $MovedNames) { $path = "$assemblyName\$path" }
        $allowed.Add($dll.Name, (Join-Path $Root $path))
    }
    $files = @(Get-ChildItem -LiteralPath $Root -Recurse -Filter '*.dll' -File)
    if ($files.Count -ne $allowed.Count) { throw "DLL count mismatch: $Root" }
    foreach ($file in $files) {
        if ($file.FullName -ne $allowed[$file.Name]) { throw "Unexpected or duplicate DLL: $($file.FullName)" }
        $expected = $marker.files | Where-Object path -EQ $file.Name
        if ((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne $expected.sha256) {
            throw "DLL bytes changed: $($file.FullName)"
        }
    }
}

New-Item -ItemType Directory -Path $output | Out-Null
$baseline = Copy-Package 'baseline'
Assert-Layout $baseline @()
$original = Join-Path $output 'application.original.manifest'
$baselineExe = Join-Path $baseline 'bmz-player.exe'
$baselinePe = @(Read-Pe $baselineExe)
$baselineSignature = (Get-PeSignature $baselinePe) -join "`n"
if ($baselinePe -match '^  Type: .*\(ID 24\)') {
    Invoke-Mt @("-inputresource:${baselineExe};#1", "-out:$original")
    [xml]$application = Get-Content -LiteralPath $original -Raw
} else {
    'Original executable contains no RT_MANIFEST resource (type 24).' |
        Set-Content -LiteralPath (Join-Path $output 'application.original.txt')
    [xml]$application = @'
<?xml version="1.0" encoding="UTF-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="BMZ.Player" version="1.0.0.0" processorArchitecture="amd64"/>
</assembly>
'@
}
$template = Join-Path $PSScriptRoot 'dll.manifest'
[xml]$definition = Get-Content -LiteralPath $template -Raw
$identity = $definition.assembly.assemblyIdentity
$assemblyName = $identity.name
$xmlns = $definition.DocumentElement.NamespaceURI
$dependency = $application.CreateElement('dependency', $xmlns)
$dependent = $application.CreateElement('dependentAssembly', $xmlns)
[void]$dependent.AppendChild($application.ImportNode($identity, $true))
[void]$dependency.AppendChild($dependent)
[void]$application.DocumentElement.AppendChild($dependency)
$applicationPath = Join-Path $output 'application.sxs.manifest'
$application.Save($applicationPath)

foreach ($label in @('ffmpeg-only', 'sxs')) {
    $root = Copy-Package $label
    $assemblyDirectory = Join-Path $root $assemblyName
    New-Item -ItemType Directory -Path $assemblyDirectory | Out-Null
    $moved = if ($label -eq 'sxs') { $dlls } else { $ffmpeg }
    [xml]$assembly = Get-Content -LiteralPath $template -Raw
    foreach ($dll in $moved) {
        $file = $assembly.CreateElement('file', $xmlns)
        $file.SetAttribute('name', $dll.Name)
        [void]$assembly.DocumentElement.AppendChild($file)
        Move-Item -LiteralPath (Join-Path $root $dll.Name) -Destination $assemblyDirectory
    }
    $assemblyPath = Join-Path $assemblyDirectory "$assemblyName.manifest"
    $assembly.Save($assemblyPath)
    Invoke-Mt @('-nologo', '-validate_manifest', '-manifest', $assemblyPath)
    $exe = Join-Path $root 'bmz-player.exe'
    if ((Get-AuthenticodeSignature -LiteralPath $exe).Status -ne 'NotSigned') {
        throw 'Use unsigned PoC inputs. A signed executable must be re-signed after embedding.'
    }
    Invoke-Mt @('-nologo', '-manifest', $applicationPath, "-outputresource:$exe;#1")
    $embedded = Join-Path $output "$label.embedded.manifest"
    Invoke-Mt @("-inputresource:$exe;#1", "-out:$embedded")
    [xml]$extracted = Get-Content -LiteralPath $embedded -Raw
    if ((Get-CanonicalXml $extracted) -cne (Get-CanonicalXml $application)) {
        throw "Embedded XML differs from the application manifest: $label"
    }
    if (((Get-PeSignature @(Read-Pe $exe)) -join "`n") -ne $baselineSignature) {
        throw "Non-manifest PE resources or subsystem changed: $label"
    }
    $declared = @($assembly.assembly.file | ForEach-Object name)
    if (@(Compare-Object $moved.Name $declared).Count -ne 0) { throw 'Manifest DLL list mismatch' }
    Assert-Layout $root $declared
    if (Test-Path -LiteralPath "$exe.manifest") { throw 'Unexpected external application manifest' }

    # The current player validates this marker before resolving storage. Keep
    # the permitted subset for portable startup, and record the full inventory
    # separately. This marker must never be shipped as update metadata.
    $poCMarkerPath = Join-Path $root $markerPath
    $pocMarker = Get-Content -LiteralPath $poCMarkerPath -Raw | ConvertFrom-Json
    $pocMarker.files = @($pocMarker.files | Where-Object path -NotIn $declared)
    $playerEntry = $pocMarker.files | Where-Object path -EQ 'bmz-player.exe'
    $playerEntry.size = (Get-Item -LiteralPath $exe).Length
    $playerEntry.sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $pocMarker | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $poCMarkerPath -Encoding utf8NoBOM
    $complete = $pocMarker.files + @(foreach ($file in (Get-ChildItem -LiteralPath $assemblyDirectory -File)) {
        [ordered]@{
            path = "$assemblyName/$($file.Name)"
            size = $file.Length
            sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    })
    $complete | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output "$label.files.json") -Encoding utf8NoBOM
}
