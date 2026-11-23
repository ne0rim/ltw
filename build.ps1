<#
.SYNOPSIS
    Builds everything Lead the Way needs so you can press Run in Godot.

.DESCRIPTION
    1. Runs the core sim tests (skip with -SkipTests).
    2. Builds the Rust GDExtension (debug by default, -Release for optimised).
    3. On first setup only, runs a headless Godot import so the editor knows
       about the extension's classes.

.EXAMPLE
    .\build.ps1
    .\build.ps1 -Release
    .\build.ps1 -SkipTests
#>
param(
    [switch]$Release,
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$started = Get-Date

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "`nBUILD FAILED: $msg" -ForegroundColor Red; exit 1 }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "cargo not found. Install Rust from https://rustup.rs"
}

# 1. Tests
if (-not $SkipTests) {
    Write-Step "Testing core sim"
    cargo test --quiet --manifest-path "$root\core\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { Fail "core tests failed" }
}

# 2. Extension
$profileName = 'debug'
$cargoArgs = @('build', '--manifest-path', "$root\godot-extension\Cargo.toml")
if ($Release) { $profileName = 'release'; $cargoArgs += '--release' }

Write-Step "Building Godot extension ($profileName)"
cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    Fail "extension build failed. If it says the .dll is in use, close Godot (or the running game) and try again."
}
$dll = "$root\godot-extension\target\$profileName\ltw_godot.dll"
if (-not (Test-Path $dll)) { Fail "expected $dll but it wasn't produced" }

# 3. First-time Godot import (only if Godot has never registered the extension)
$extList = "$root\client\.godot\extension_list.cfg"
if (-not (Test-Path $extList) -or -not (Select-String -Path $extList -Pattern 'lt_way.gdextension' -Quiet)) {
    $godot = $env:LTW_GODOT
    if (-not $godot) {
        $godot = Get-ChildItem 'C:\Program Files\Godot*\Godot*_console.exe' -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
    if ($godot -and (Test-Path $godot)) {
        Write-Step "First-time Godot import"
        & $godot --headless --editor --quit --path "$root\client" *> $null
    } else {
        Write-Host "`nCouldn't find Godot to do the first-time import. Opening the project in the editor once does the same thing. (Set LTW_GODOT to the Godot console .exe to automate it.)" -ForegroundColor Yellow
    }
}

$secs = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
Write-Host "`nBUILD OK ($profileName, ${secs}s) -> $dll" -ForegroundColor Green
if (Get-Process -Name 'Godot*' -ErrorAction SilentlyContinue) {
    Write-Host "Godot is open: click back into the editor to hot-reload the extension, or restart it if the changes don't show." -ForegroundColor Yellow
} else {
    Write-Host "Open client\project.godot in Godot and press Run (F5)."
}
