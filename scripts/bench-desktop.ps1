param(
    [Parameter(Mandatory)] [int] $AppProcessId,
    [Parameter(Mandatory)] [string] $Scenario,
    [ValidateRange(1, 300)] [int] $Seconds = 60,
    [string] $OutputDirectory = 'D:\Outputs\CallMe\bench'
)
function Get-Gpu3DMaximum {
    param([double[]] $Values)
    if (-not $Values.Count -or @($Values | Where-Object { -not [double]::IsFinite($_) -or $_ -lt 0 -or $_ -gt 100 }).Count) { return $null }
    return [math]::Round(($Values | Measure-Object -Maximum).Maximum, 3)
}
$ErrorActionPreference = 'Stop'
$process = Get-Process -Id $AppProcessId
if ($process.ProcessName -notin @('ClaudeCall', 'callme-desk')) { throw 'Select the Claude Call desktop process.' }
$gpuCounters = @()
try {
    $category = [Diagnostics.PerformanceCounterCategory]::new('GPU Engine')
    foreach ($instance in $category.GetInstanceNames() | Where-Object { $_ -match "^pid_${AppProcessId}_.*engtype_3D$" }) {
        $counter = [Diagnostics.PerformanceCounter]::new('GPU Engine', 'Utilization Percentage', $instance, $true)
        $gpuCounters += $counter
        $null = $counter.NextValue()
    }
} catch {
    foreach ($counter in $gpuCounters) { $counter.Dispose() }
    $gpuCounters = @()
    Write-Warning 'GPU counters unavailable; GPU values will be empty.'
}
$previousCpu = $process.TotalProcessorTime.TotalSeconds
$previousTime = [Diagnostics.Stopwatch]::StartNew()
try {
$rows = for ($sample = 0; $sample -lt $Seconds; $sample++) {
    Start-Sleep -Seconds 1
    $process.Refresh()
    if ($process.HasExited) { throw 'The app exited before measurement finished.' }
    $cpu = $process.TotalProcessorTime.TotalSeconds
    $elapsed = $previousTime.Elapsed.TotalSeconds
    $previousTime.Restart()
    $gpu = $null
    if ($gpuCounters.Count) {
        try { $gpu = Get-Gpu3DMaximum @($gpuCounters | ForEach-Object { $_.NextValue() }) }
        catch { Write-Warning 'GPU sample unavailable; leaving this value empty.' }
    }
    [PSCustomObject]@{
        TimeUtc = [DateTime]::UtcNow.ToString('o')
        Scenario = $Scenario
        ProcessId = $AppProcessId
        CpuPercentOfOneCore = [math]::Round(100 * ($cpu - $previousCpu) / $elapsed, 2)
        WorkingSetMiB = [math]::Round($process.WorkingSet64 / 1MB, 2)
        PrivateMiB = [math]::Round($process.PrivateMemorySize64 / 1MB, 2)
        Gpu3DBusiestEnginePercent = $gpu
    }
    $previousCpu = $cpu
}
} finally {
    foreach ($counter in $gpuCounters) { $counter.Dispose() }
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
    GpuSamples = @($rows | Where-Object { $null -ne $_.Gpu3DBusiestEnginePercent }).Count
    AverageGpu3DBusiestEnginePercent = if (@($rows | Where-Object { $null -ne $_.Gpu3DBusiestEnginePercent }).Count) { [math]::Round(($rows | Measure-Object Gpu3DBusiestEnginePercent -Average).Average, 3) } else { $null }
}
