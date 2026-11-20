# Makes a throwaway Besfa game that builds against this checkout's crates.
# Usage: pwsh -NoProfile -File .claude/skills/verify-besfa/new-game.ps1 -Parent <folder> [-Name verify_123456] [-Template 3d|2d]
# Prints the project's path. Build and start it as SKILL.md says.
param(
    [Parameter(Mandatory)] [string] $Parent,
    [string] $Name = "verify_$(Get-Date -Format HHmmss)",
    [ValidateSet('3d', '2d')] [string] $Template = '3d'
)
$ErrorActionPreference = 'Stop'
$checkout = (Resolve-Path "$PSScriptRoot/../../..").Path

# The installed besfa.exe can trail the template; build this checkout's CLI,
# into CARGO_TARGET_DIR when set (a worktree points it at the main checkout's).
cargo build -q -p besfa_cli --manifest-path "$checkout/Cargo.toml"
if ($LASTEXITCODE) { throw "besfa_cli did not build" }
$target = (cargo metadata -q --no-deps --format-version 1 --manifest-path "$checkout/Cargo.toml" | ConvertFrom-Json).target_directory
# The game builds into the shared target its .cargo/config.toml names, never here.
Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue

New-Item -ItemType Directory -Force $Parent | Out-Null
$env:CARGO_CARGO_NEW_VCS = 'none'  # git init fails on long scratch paths
Push-Location $Parent
try { & "$target/debug/besfa.exe" new $Name --template $Template --output json | Out-Null }
finally { Pop-Location }
if ($LASTEXITCODE) { throw "besfa new failed" }

$project = Join-Path (Resolve-Path $Parent).Path $Name
$crates = ($checkout -replace '\\', '/') + '/crates'
Add-Content "$project/Cargo.toml" @"

# verify-besfa: build against this checkout's crates, not GitHub's.
[patch."https://github.com/ski0090/besfa"]
besfa_components = { path = "$crates/besfa_components" }
besfa_editor_plugin = { path = "$crates/besfa_editor_plugin" }
"@
$project
