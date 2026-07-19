# Package a vcpkg install tree into the release zip layout consumed by Easel.
#
# Expects:
#   $env:VCPKG_ROOT  - vcpkg checkout that already has libheif installed
#   $env:TRIPLET     - e.g. x64-windows-static-md
#   $env:OUT_ZIP     - destination zip path
#   $env:LIBHEIF_VERSION (optional) - expected port version; verified against status
#   $env:INCLUDE_DEBUG (optional) - "true" to keep debug/; default false
#
# Produces a zip whose root contains .vcpkg-root + installed/... (+ versions.json).

$ErrorActionPreference = "Stop"

$VcpkgRoot = $env:VCPKG_ROOT
$Triplet = if ($env:TRIPLET) { $env:TRIPLET } else { "x64-windows-static-md" }
$OutZip = $env:OUT_ZIP
$ExpectedVersion = $env:LIBHEIF_VERSION
$IncludeDebug = ($env:INCLUDE_DEBUG -eq "true")

if (-not $VcpkgRoot) { throw "VCPKG_ROOT is required" }
if (-not $OutZip) { throw "OUT_ZIP is required" }

$Installed = Join-Path $VcpkgRoot "installed\$Triplet"
$HeifLib = Join-Path $Installed "lib\heif.lib"
if (-not (Test-Path $HeifLib)) {
    throw "libheif not installed for triplet $Triplet (missing $HeifLib)"
}

# Resolve actual port version from vcpkg status (authoritative).
$StatusPath = Join-Path $VcpkgRoot "installed\vcpkg\status"
if (-not (Test-Path $StatusPath)) {
    throw "missing vcpkg status at $StatusPath"
}
$StatusText = Get-Content $StatusPath -Raw
$ActualVersion = $null
foreach ($block in ($StatusText -split "(?m)^$")) {
    if ($block -match "(?m)^Package:\s*libheif\s*$" -and $block -match "(?m)^Version:\s*(\S+)\s*$" -and $block -notmatch "(?m)^Feature:") {
        $ActualVersion = $Matches[1]
        break
    }
}
if (-not $ActualVersion) {
    throw "could not parse libheif Version from $StatusPath"
}

$VersionHeader = Join-Path $Installed "include\libheif\heif_version.h"
if (Test-Path $VersionHeader) {
    $hdr = Get-Content $VersionHeader -Raw
    if ($hdr -match 'LIBHEIF_VERSION\s+"([^"]+)"') {
        $HeaderVersion = $Matches[1]
        if ($HeaderVersion -ne $ActualVersion) {
            throw "heif_version.h ($HeaderVersion) disagrees with vcpkg status ($ActualVersion)"
        }
    }
}

if ($ExpectedVersion -and $ExpectedVersion -ne $ActualVersion) {
    throw "version mismatch: versions.json expects libheif $ExpectedVersion but vcpkg installed $ActualVersion (wrong vcpkg ref?)"
}

Write-Host "Packaging libheif $ActualVersion for $Triplet (include_debug=$IncludeDebug)"

$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-deps-pkg-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Path $Stage | Out-Null

# Minimal marker file (empty) so vcpkg-rs accepts the root.
New-Item -ItemType File -Path (Join-Path $Stage ".vcpkg-root") -Force | Out-Null

$StageInstalled = Join-Path $Stage "installed"
$StageTriplet = Join-Path $StageInstalled $Triplet
New-Item -ItemType Directory -Path $StageTriplet | Out-Null

# Copy release install tree; optionally drop debug/ (halves zip size, unused by Easel CI).
Get-ChildItem $Installed | ForEach-Object {
    if (-not $IncludeDebug -and $_.Name -eq "debug") { return }
    Copy-Item -Recurse -Force $_.FullName (Join-Path $StageTriplet $_.Name)
}

# Copy vcpkg status database, then rewrite .list paths if we dropped debug/.
Copy-Item -Recurse -Force (Join-Path $VcpkgRoot "installed\vcpkg") (Join-Path $StageInstalled "vcpkg")
$Updates = Join-Path $StageInstalled "vcpkg\updates"
if (-not (Test-Path $Updates)) {
    New-Item -ItemType Directory -Path $Updates | Out-Null
}

if (-not $IncludeDebug) {
    $InfoDir = Join-Path $StageInstalled "vcpkg\info"
    if (Test-Path $InfoDir) {
        Get-ChildItem $InfoDir -Filter "*.list" | ForEach-Object {
            $lines = Get-Content $_.FullName | Where-Object { $_ -notmatch "/debug/" -and $_ -notmatch "\\debug\\" }
            Set-Content -Path $_.FullName -Value ($lines -join "`n") -Encoding ascii
        }
    }
}

# Provenance: copy versions.json and stamp the resolved libheif version.
$VersionsSrc = Join-Path $PSScriptRoot "..\versions.json"
$VersionsDst = Join-Path $Stage "versions.json"
if (Test-Path $VersionsSrc) {
    $v = Get-Content $VersionsSrc -Raw | ConvertFrom-Json
    $v.libheif.version = $ActualVersion
    $v | Add-Member -NotePropertyName resolved -NotePropertyValue ([pscustomobject]@{
        libheif_version = $ActualVersion
        triplet         = $Triplet
        packaged_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        include_debug   = $IncludeDebug
    }) -Force
    ($v | ConvertTo-Json -Depth 8) | Set-Content -Path $VersionsDst -Encoding utf8
} else {
    @{
        libheif = @{ version = $ActualVersion; triplet = $Triplet }
    } | ConvertTo-Json | Set-Content -Path $VersionsDst -Encoding utf8
}

$OutDir = Split-Path -Parent $OutZip
if ($OutDir -and -not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}
if (Test-Path $OutZip) { Remove-Item -Force $OutZip }

Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $OutZip -CompressionLevel Optimal

# Emit sidecar digest for release upload.
$Hash = (Get-FileHash -Algorithm SHA256 -Path $OutZip).Hash.ToLowerInvariant()
$SumPath = "$OutZip.sha256"
Set-Content -Path $SumPath -Value "$Hash  $(Split-Path -Leaf $OutZip)" -Encoding ascii

Write-Host "Wrote $OutZip"
Write-Host "SHA256: $Hash"
Write-Host "Size: $((Get-Item $OutZip).Length) bytes"
Get-ChildItem (Join-Path $StageTriplet "lib") | Select-Object -ExpandProperty Name | ForEach-Object { Write-Host "lib/$_" }

# Surface outputs for GitHub Actions.
if ($env:GITHUB_OUTPUT) {
    "resolved_version=$ActualVersion" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "sha256=$Hash" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}

Remove-Item -Recurse -Force $Stage
