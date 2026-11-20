# Read-only: is this verification game worth driving?
# Usage: pwsh -NoProfile -File .claude/skills/verify-besfa/doctor.ps1 -Project <game folder>
# Prints one line per check and exits 1 when a FAIL line printed.
param([Parameter(Mandatory)] [string] $Project)
$checkout = (Resolve-Path "$PSScriptRoot/../../..").Path
$failed = $false
function Say($ok, $text) {
    $mark = if ($ok -is [string]) { 'WARN' } elseif ($ok) { 'ok  ' } else { 'FAIL' }
    if ($mark -eq 'FAIL') { $script:failed = $true }
    "$mark $text"
}

$manifest = Get-Content "$Project/Cargo.toml" -Raw -ErrorAction SilentlyContinue
Say ([bool]$manifest) "project manifest: $Project/Cargo.toml"
$name = if ($manifest -match '(?m)^name = "([^"]+)"') { $Matches[1] } else { '?' }
$crates = ($checkout -replace '\\', '/') + '/crates'
Say ($manifest -like "*besfa_editor_plugin = { path = `"$crates/besfa_editor_plugin`" }*") "game is patched to this checkout ($crates)"
Say (Test-Path "$Project/assets/scenes/main.scn.ron") 'scene file assets/scenes/main.scn.ron exists'

# The CLI new-game.ps1 built: in CARGO_TARGET_DIR when set, as for a worktree.
$target = (cargo metadata -q --no-deps --format-version 1 --manifest-path "$checkout/Cargo.toml" | ConvertFrom-Json).target_directory
$cli = Get-Item "$target/debug/besfa.exe" -ErrorAction SilentlyContinue
$cliChanged = [datetime](git -C $checkout log -1 --format=%cI -- crates/besfa_cli crates/besfa_mcp)
Say ($cli -and $cli.LastWriteTime -ge $cliChanged) "$target\debug\besfa.exe is newer than the last CLI commit ($cliChanged)"

$shared = "$env:LOCALAPPDATA\Besfa\target"
$procs = Get-CimInstance Win32_Process -Filter "Name='rustc.exe' or Name='cargo.exe' or Name='besfa_editor.exe'"
$others = $procs | Where-Object { $_.Name -eq 'rustc.exe' -and $_.CommandLine -like '*Besfa*target*' -and $_.CommandLine -notlike "*--crate-name $name *" }
if ($others) { Say 'warn' "a build is compiling into $shared (this run's own while it builds the plugin, else another; cargo waits for its lock): rustc pids $($others.ProcessId -join ', ')" }
else { Say $true "no other build in $shared" }
$editors = $procs | Where-Object Name -eq 'besfa_editor.exe'
if ($editors) { Say 'warn' "besfa_editor.exe is running (pids $($editors.ProcessId -join ', ')); never open this project in it" }

$game = Get-CimInstance Win32_Process -Filter "Name='$name.exe'" | Where-Object { $_.ExecutablePath -like "$shared*" }
if ($game) { Say $true "game $name.exe running: pid $($game.ProcessId -join ', ')" }
else { Say 'warn' "game $name.exe is not running (fine before start; after start it is still building or exited)" }
if ($failed) { exit 1 }
