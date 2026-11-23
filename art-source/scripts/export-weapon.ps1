<#
.SYNOPSIS
    Export a hand-edited model .blend (weapon, first-person arms, ...) to the game (.glb).

.EXAMPLE
    .\art-source\scripts\export-weapon.ps1 m16a2
    Exports art-source\weapons\m16a2\m16a2.blend to client\assets\models\weapons\m16a2.glb.

.EXAMPLE
    .\art-source\scripts\export-weapon.ps1 fp_arms
    Exports art-source\characters\fp_arms\fp_arms.blend to client\assets\models\characters\fp_arms.glb.

.NOTES
    Set LTW_BLENDER to blender.exe if Blender isn't in C:\Program Files\Blender Foundation.
    Save your file in Blender first: this exports what's on disk.
#>
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Weapon
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')

# Look in every category: art-source\<category>\<id>\<id>.blend
$blend = Get-ChildItem (Join-Path $root "art-source\*\$Weapon\$Weapon.blend") -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $blend) {
    Write-Host "No hand-edited file art-source\<category>\$Weapon\$Weapon.blend. Run its build script first." -ForegroundColor Red
    exit 1
}
$category = Split-Path (Split-Path (Split-Path $blend -Parent) -Parent) -Leaf

$blender = $env:LTW_BLENDER
if (-not $blender) {
    $blender = Get-ChildItem 'C:\Program Files\Blender Foundation\Blender*\blender.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $blender -or -not (Test-Path $blender)) {
    Write-Host "Couldn't find blender.exe. Set LTW_BLENDER to its full path." -ForegroundColor Red
    exit 1
}

$script = Join-Path $PSScriptRoot 'blender\export_weapon.py'
$out = & $blender --background $blend --python $script 2>&1
$out | Where-Object { $_ -match '^\[export\]|Error|Traceback' } | ForEach-Object { Write-Host $_ }
$glb = Join-Path $root "client\assets\models\$category\$Weapon.glb"
if ($LASTEXITCODE -ne 0 -or -not ($out -match '^\[export\] .* -> ')) {
    Write-Host "EXPORT FAILED" -ForegroundColor Red
    exit 1
}
Write-Host "EXPORT OK -> $glb  (Godot re-imports it when you focus the editor)" -ForegroundColor Green
