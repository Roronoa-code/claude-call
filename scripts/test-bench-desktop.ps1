$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'bench-desktop.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Benchmark script has syntax errors' }
$function = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-Gpu3DMaximum' }, $true).Body.GetScriptBlock()
if ((& $function @(10, 60)) -ne 60) { throw 'Must report the busiest engine, not a sum' }
if ((& $function @(0, 0)) -ne 0) { throw 'Zero is a valid measurement' }
foreach ($invalid in @(@(), @(231.209), @(-1), @([double]::NaN), @([double]::PositiveInfinity), @(10, 101))) {
    if ($null -ne (& $function $invalid)) { throw 'Missing or invalid samples must remain unknown' }
}
'GPU aggregation checks passed; no app launched.'
