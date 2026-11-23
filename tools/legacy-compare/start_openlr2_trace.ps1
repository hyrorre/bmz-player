param(
    [string]$OpenLr2Directory = 'D:\OpenLR2',
    [string]$Executable,
    [string]$TracePath
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if (-not $Executable) { $Executable = Join-Path $repo '.local\openlr2-parity-trace\OpenLR2_ParityTrace_x64.exe' }
if (-not $TracePath) { $TracePath = Join-Path $repo '.local\openlr2-parity-trace\capture.csv' }
if (-not (Test-Path -LiteralPath $Executable)) { throw 'Build the OpenLR2 trace executable first' }
if (Test-Path -LiteralPath $TracePath) { throw "Trace already exists: $TracePath. Choose a new TracePath to preserve it." }
$oldTrace = $env:OPENLR2_PARITY_TRACE
$oldNoCd = $env:OPENLR2_NO_CD
$env:OPENLR2_PARITY_TRACE = [IO.Path]::GetFullPath($TracePath)
$env:OPENLR2_NO_CD = '1'
Push-Location $OpenLr2Directory
try {
    Write-Host "Starting OpenLR2. Play one chart and exit normally. Trace: $env:OPENLR2_PARITY_TRACE"
    $traceProcess = Start-Process -FilePath $Executable -WorkingDirectory $OpenLr2Directory -PassThru -Wait
    if ($traceProcess.ExitCode -ne 0) {
        $exitHex = '0x{0:X8}' -f ($traceProcess.ExitCode -band 0xffffffffL)
        throw "OpenLR2 exited with $exitHex ($($traceProcess.ExitCode)). Startup log: $OpenLr2Directory\Log.txt"
    }
    if (-not (Test-Path -LiteralPath $env:OPENLR2_PARITY_TRACE)) {
        throw 'OpenLR2 exited without a trace. Play one chart in the trace executable before exiting.'
    }
    Get-Item -LiteralPath $env:OPENLR2_PARITY_TRACE | Select-Object FullName,Length,LastWriteTime
} finally {
    Pop-Location
    $env:OPENLR2_PARITY_TRACE = $oldTrace
    $env:OPENLR2_NO_CD = $oldNoCd
}
