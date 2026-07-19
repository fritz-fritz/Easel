# Package a vcpkg install tree into the release zip layout consumed by Easel.
#
# Expects:
#   $env:VCPKG_ROOT  - vcpkg checkout that already has libheif installed
#   $env:TRIPLET     - e.g. x64-windows-static-md
#   $env:OUT_ZIP     - destination zip path
#
# Produces a zip whose root contains .vcpkg-root + installed/...

$ErrorActionPreference = "Stop"

$VcpkgRoot = $env:VCPKG_ROOT
$Triplet = if ($env:TRIPLET) { $env:TRIPLET } else { "x64-windows-static-md" }
$OutZip = $env:OUT_ZIP

if (-not $VcpkgRoot) { throw "VCPKG_ROOT is required" }
if (-not $OutZip) { throw "OUT_ZIP is required" }

$Installed = Join-Path $VcpkgRoot "installed\$Triplet"
$HeifLib = Join-Path $Installed "lib\heif.lib"
if (-not (Test-Path $HeifLib)) {
    throw "libheif not installed for triplet $Triplet (missing $HeifLib)"
}

$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-deps-pkg-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Path $Stage | Out-Null

# Minimal marker file (empty) so vcpkg-rs accepts the root.
New-Item -ItemType File -Path (Join-Path $Stage ".vcpkg-root") -Force | Out-Null

$StageInstalled = Join-Path $Stage "installed"
New-Item -ItemType Directory -Path $StageInstalled | Out-Null

# Copy the triplet install tree and vcpkg status database.
Copy-Item -Recurse -Force (Join-Path $VcpkgRoot "installed\$Triplet") (Join-Path $StageInstalled $Triplet)
Copy-Item -Recurse -Force (Join-Path $VcpkgRoot "installed\vcpkg") (Join-Path $StageInstalled "vcpkg")

# Ensure updates/ exists (vcpkg-rs always reads it).
$Updates = Join-Path $StageInstalled "vcpkg\updates"
if (-not (Test-Path $Updates)) {
    New-Item -ItemType Directory -Path $Updates | Out-Null
}

# Provenance
$VersionsSrc = Join-Path $PSScriptRoot "..\versions.json"
if (Test-Path $VersionsSrc) {
    Copy-Item -Force $VersionsSrc (Join-Path $Stage "versions.json")
}

$OutDir = Split-Path -Parent $OutZip
if ($OutDir -and -not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}
if (Test-Path $OutZip) { Remove-Item -Force $OutZip }

Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $OutZip -CompressionLevel Optimal
Write-Host "Wrote $OutZip"
Write-Host "Size: $((Get-Item $OutZip).Length) bytes"

# Sanity listing
Get-ChildItem (Join-Path $StageInstalled "$Triplet\lib") | Select-Object -ExpandProperty Name | ForEach-Object { Write-Host "lib/$_" }

Remove-Item -Recurse -Force $Stage
