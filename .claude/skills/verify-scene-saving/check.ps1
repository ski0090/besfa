# Model-checks SaveHandshake.tla: every rule under every set of assumptions,
# one TLC run each, and compares the results with expected.txt.
# Usage: pwsh -NoProfile -File .claude/skills/verify-scene-saving/check.ps1
# Exits 1 when a result differs from expected.txt. Traces land in
# target/verify-scene-saving/<assumptions>-<rule>.out.
param([string] $Out = (Join-Path $PSScriptRoot '../../../target/verify-scene-saving'))
$ErrorActionPreference = 'Stop'
$jar = if ($env:TLA2TOOLS) { $env:TLA2TOOLS } else { 'C:\codes\.tools\tla\tla2tools.jar' }
if (-not (Test-Path $jar)) { throw "No TLC at $jar. See SKILL.md, Setup." }
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

$rules = 'NoLossOnPluginUpdate', 'NoLossOnStop', 'NoLossOnRebuild', 'NoLossOnNetPlay', 'NoLossOnClose', 'NoLossOnLeave'
$assumptions = [ordered]@{
    'patient-reliable' = 'PatientUser = TRUE  SaveCanFail = FALSE SaveCanTimeOut = FALSE'
    'patient-failing'  = 'PatientUser = TRUE  SaveCanFail = TRUE  SaveCanTimeOut = TRUE'
    'hasty-reliable'   = 'PatientUser = FALSE SaveCanFail = FALSE SaveCanTimeOut = FALSE'
}
$expected = @{}
foreach ($line in Get-Content "$PSScriptRoot/expected.txt" | Where-Object { $_ -match '^\S+ \S+ (PASS|FAIL)$' }) {
    $a, $r, $v = $line -split ' '
    $expected["$a $r"] = $v
}

$differs = $false
Push-Location $PSScriptRoot
try {
    foreach ($a in $assumptions.Keys) {
        foreach ($rule in $rules) {
            $name = "$a-$rule"
            Set-Content "$Out/$name.cfg" @"
CONSTANTS MaxEdits = 2  MaxRestarts = 1  $($assumptions[$a])
SPECIFICATION Spec
CONSTRAINT Bounds
INVARIANT TypeOK
INVARIANT $rule
"@
            $text = java -XX:+UseParallelGC -cp $jar tlc2.TLC -workers 1 `
                -metadir "$Out/states/$name" -config "$Out/$name.cfg" SaveHandshake.tla 2>&1 | Out-String
            Set-Content "$Out/$name.out" $text
            $result = if ($text -match "Invariant $rule is violated") { 'FAIL' }
                      elseif ($text -match 'No error has been found') { 'PASS' }
                      else { 'ERROR' }
            $path = ([regex]::Matches($text, '(?m)^State \d+: <(\w+)') | ForEach-Object { $_.Groups[1].Value }) -join ' > '
            $states = if ($text -match '(\d+) distinct states found') { $Matches[1] } else { '?' }
            $want = $expected["$a $rule"]
            $mark = if ($result -eq $want) { '  ' } else { $differs = $true; '!!' }
            "$mark $a $rule $result (expected $want, $states states)$(if ($path) { "`n     $path" })"
        }
    }
}
finally { Pop-Location }
Remove-Item -Recurse -Force "$Out/states" -ErrorAction SilentlyContinue
if ($differs) { "!! marks a result that differs from expected.txt"; exit 1 }
