param(
    [Parameter(Mandatory)] [int] $AppProcessId,
    [Parameter(Mandatory)] [string] $Scenario,
    [ValidateRange(1, 300)] [int] $Seconds = 60,
    [string] $OutputDirectory = 'D:\Outputs\CallMe\bench'
)
$ErrorActionPreference = 'Stop'
$process = Get-Process -Id $AppProcessId
if ($process.ProcessName -notin @('ClaudeCall', 'callme-desk')) { throw 'Select the Claude Call desktop process.' }
$previousCpu = $process.TotalProcessorTime.TotalSeconds
$previousTime = [Diagnostics.Stopwatch]::StartNew()
$rows = for ($sample = 0; $sample -lt $Seconds; $sample++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if ($process.HasExited) { throw 'The app exited before measurement finished.' }
    $cpu = $process.TotalProcessorTime.TotalSeconds
    $elapsed = $previousTime.Elapsed.TotalSeconds
    $previousTime.Restart()
    [PSCustomObject]@{
        TimeUtc = [DateTime]::UtcNow.ToString('o')
        Scenario = $Scenario
        ProcessId = $AppProcessId
        CpuPercentOfOneCore = [math]::Round(100 * ($cpu - $previousCpu) / $elapsed, 2)
        WorkingSetMiB = [math]::Round($process.WorkingSet64 / 1MB, 2)
        PrivateMiB = [math]::Round($process.PrivateMemorySize64 / 1MB, 2)
    }
    $previousCpu = $cpu
}
$name = $Scenario -replace '[^a-zA-Z0-9_-]', '-'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$path = Join-Path $OutputDirectory (('{0}-{1}.csv' -f [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff'), $name))
$rows | Export-Csv -LiteralPath $path -NoTypeInformation
[PSCustomObject]@{
    Path = $path
    Samples = @($rows).Count
    AverageCpuPercentOfOneCore = [math]::Round(($rows | Measure-Object CpuPercentOfOneCore -Average).Average, 2)
    AverageWorkingSetMiB = [math]::Round(($rows | Measure-Object WorkingSetMiB -Average).Average, 2)
    AveragePrivateMiB = [math]::Round(($rows | Measure-Object PrivateMiB -Average).Average, 2)
}
