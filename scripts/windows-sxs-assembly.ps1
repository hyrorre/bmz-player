# Shared by packaging and the release compatibility verifier.
function Get-WindowsManifestTool {
    $command = Get-Command mt.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { return $command.Source }
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    foreach ($version in (Get-ChildItem -LiteralPath $sdk -Directory | Where-Object Name -Match '^\d+(\.\d+){3}$' |
            Sort-Object { [version]$_.Name } -Descending)) {
        $tool = Join-Path $version.FullName 'x64\mt.exe'
        if (Test-Path -LiteralPath $tool -PathType Leaf) { return $tool }
    }
    throw 'The x64 Windows SDK Manifest Tool (mt.exe) is required'
}

function Invoke-WindowsManifestTool([string[]]$Arguments) {
    $tool = Get-WindowsManifestTool
    & $tool -nologo @Arguments | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "mt.exe failed: $Arguments" }
}

function Test-EmbeddedApplicationManifest([string]$Executable) {
    if (-not ('BmzPackaging.PeResource' -as [type])) {
        Add-Type -Namespace BmzPackaging -Name PeResource -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode, SetLastError = true)]
private static extern System.IntPtr LoadLibraryEx(string name, System.IntPtr reserved, uint flags);
[System.Runtime.InteropServices.DllImport("kernel32.dll", EntryPoint = "FindResourceW", SetLastError = true)]
private static extern System.IntPtr FindResource(System.IntPtr module, System.IntPtr name, System.IntPtr type);
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
private static extern bool FreeLibrary(System.IntPtr module);
public static bool HasApplicationManifest(string exe) {
    var module = LoadLibraryEx(exe, System.IntPtr.Zero, 0x22);
    if (module == System.IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
    try {
        if (FindResource(module, (System.IntPtr)1, (System.IntPtr)24) != System.IntPtr.Zero) return true;
        int error = System.Runtime.InteropServices.Marshal.GetLastWin32Error();
        if (error == 1812 || error == 1813 || error == 1814) return false;
        throw new System.ComponentModel.Win32Exception(error);
    } finally { FreeLibrary(module); }
}
'@
    }
    return [BmzPackaging.PeResource]::HasApplicationManifest($Executable)
}

function Install-WindowsPrivateAssembly([string]$Package) {
    $exe = Join-Path $Package 'bmz-player.exe'
    if ((Get-AuthenticodeSignature -LiteralPath $exe).Status -ne 'NotSigned') {
        throw 'Embed the Windows application manifest before signing the staged executable'
    }
    $dlls = @(Get-ChildItem -LiteralPath $Package -File -Filter '*.dll' | Sort-Object Name)
    if ($dlls.Count -eq 0) { throw 'No DLLs to place in the private assembly' }
    $assemblyDirectory = (New-Item -ItemType Directory -Path (Join-Path $Package 'dll')).FullName
    [xml]$assembly = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'dll.manifest'))
    $namespace = $assembly.DocumentElement.NamespaceURI
    foreach ($dll in $dlls) {
        $file = $assembly.CreateElement('file', $namespace)
        $file.SetAttribute('name', $dll.Name)
        [void]$assembly.DocumentElement.AppendChild($file)
        Move-Item -LiteralPath $dll.FullName -Destination $assemblyDirectory
    }
    $assemblyPath = Join-Path $assemblyDirectory 'dll.manifest'
    $assembly.Save($assemblyPath)
    Invoke-WindowsManifestTool @('-validate_manifest', '-manifest', $assemblyPath)

    $temporary = (New-TemporaryFile).FullName
    try {
        if (Test-EmbeddedApplicationManifest $exe) {
            Invoke-WindowsManifestTool @("-inputresource:${exe};#1", "-out:$temporary")
            [xml]$application = [IO.File]::ReadAllText($temporary)
        } else {
            [xml]$application = @'
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="BMZ.Player" version="1.0.0.0" processorArchitecture="amd64"/>
</assembly>
'@
        }
        $dependency = $application.CreateElement('dependency', $namespace)
        $dependent = $application.CreateElement('dependentAssembly', $namespace)
        [void]$dependent.AppendChild($application.ImportNode($assembly.assembly.assemblyIdentity, $true))
        [void]$dependency.AppendChild($dependent)
        [void]$application.DocumentElement.AppendChild($dependency)
        $application.Save($temporary)
        Invoke-WindowsManifestTool @('-manifest', $temporary, "-outputresource:${exe};#1")
    } finally {
        Remove-Item -LiteralPath $temporary
    }
}

function Get-WindowsPrivateAssemblyDlls([string]$Package) {
    $directory = Get-Item -LiteralPath (Join-Path $Package 'dll')
    if (-not $directory.PSIsContainer -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Expected an ordinary private DLL directory'
    }
    $dlls = @(Get-ChildItem -LiteralPath $directory.FullName -Force)
    foreach ($file in $dlls) {
        if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $file.Name -ieq 'dll.dll' -or $file.Name -notmatch '^(?:dll\.manifest|[a-z0-9][a-z0-9._-]*\.dll)$') {
            throw "Unexpected private assembly file: $($file.FullName)"
        }
    }
    $assemblyPath = Join-Path $directory.FullName 'dll.manifest'
    [xml]$assembly = [IO.File]::ReadAllText($assemblyPath)
    $ns = [Xml.XmlNamespaceManager]::new($assembly.NameTable)
    $ns.AddNamespace('asm', 'urn:schemas-microsoft-com:asm.v1')
    $identities = $assembly.SelectNodes('/asm:assembly/asm:assemblyIdentity', $ns)
    if ($identities.Count -ne 1) { throw 'Missing or duplicate private assembly identity' }
    $identity = $identities[0]
    if ($identity.GetAttribute('name') -cne 'dll' -or $identity.GetAttribute('type') -cne 'win32' -or
        $identity.GetAttribute('version') -cne '1.0.0.0' -or $identity.GetAttribute('processorArchitecture') -cne 'amd64') {
        throw 'Unexpected private assembly identity'
    }
    $declared = @($assembly.SelectNodes('/asm:assembly/asm:file', $ns) | ForEach-Object { $_.GetAttribute('name') })
    $dlls = @($dlls | Where-Object Extension -EQ '.dll')
    if ($dlls.Count -eq 0 -or $declared.Count -ne $dlls.Count -or
        @($declared | Sort-Object -Unique).Count -ne $declared.Count -or
        @(Compare-Object $dlls.Name $declared -CaseSensitive).Count -ne 0) {
        throw 'Private assembly manifest and DLL inventory differ'
    }
    if (@(Get-ChildItem -LiteralPath $Package -File -Filter '*.dll' -Force).Count -ne 0) {
        throw 'SxS package contains a DLL beside the executable'
    }
    $duplicates = Get-ChildItem -LiteralPath $Package -Recurse -File -Filter '*.dll' -Force |
        Where-Object { $_.DirectoryName -ne $directory.FullName -and $_.Name -in $declared }
    if ($duplicates) { throw 'Duplicate private assembly DLL outside dll/' }
    $exe = Join-Path $Package 'bmz-player.exe'
    foreach ($outside in @("$exe.manifest", (Join-Path $Package 'dll.manifest'))) {
        if (Test-Path -LiteralPath $outside) { throw "Unexpected root manifest: $outside" }
    }
    $temporary = (New-TemporaryFile).FullName
    try {
        Invoke-WindowsManifestTool @("-inputresource:${exe};#1", "-out:$temporary")
        [xml]$application = [IO.File]::ReadAllText($temporary)
        $ns = [Xml.XmlNamespaceManager]::new($application.NameTable)
        $ns.AddNamespace('asm', 'urn:schemas-microsoft-com:asm.v1')
        $refs = $application.SelectNodes('/asm:assembly/asm:dependency/asm:dependentAssembly/asm:assemblyIdentity[@name="dll"]', $ns)
        if ($refs.Count -ne 1 -or $refs[0].Attributes.Count -ne $identity.Attributes.Count) {
            throw 'Missing or duplicate embedded private assembly dependency'
        }
        foreach ($attribute in $identity.Attributes) {
            if ($refs[0].GetAttribute($attribute.Name) -cne $attribute.Value) {
                throw "Private assembly dependency mismatch: $($attribute.Name)"
            }
        }
    } finally {
        Remove-Item -LiteralPath $temporary
    }
    return $dlls
}
