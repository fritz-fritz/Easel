# Install prebuilt MSVC static libheif into a minimal vcpkg tree for libheif-sys.
#
# strukturag/libheif GitHub releases ship source tarballs only (no Windows binaries).
# This script downloads the vegidio/binaries-heif MSVC static package (upstream libheif
# + libde265 + x265) and stages it so `vcpkg::find_package("libheif")` succeeds without
# compiling from source.
#
# Usage (from repo root, PowerShell):
#   .\.github\scripts\install-libheif-windows.ps1
#   # sets VCPKG_ROOT in the current process; on GHA also appends to GITHUB_ENV

$ErrorActionPreference = "Stop"

$ReleaseTag = "26.7.0"
$LibheifVersion = "1.23.0"
$AssetName = "static_windows_x64.zip"
$Url = "https://github.com/vegidio/binaries-heif/releases/download/$ReleaseTag/$AssetName"
$Triplet = "x64-windows-static-md"

$Root = if ($env:EASEL_VCPKG_ROOT) {
    $env:EASEL_VCPKG_ROOT
} elseif ($env:RUNNER_TEMP) {
    Join-Path $env:RUNNER_TEMP "easel-vcpkg"
} else {
    Join-Path $PWD "target\easel-vcpkg"
}

$Marker = Join-Path $Root ".easel-libheif-prebuilt"
$Expected = "$ReleaseTag|$LibheifVersion|$AssetName"

if ((Test-Path (Join-Path $Root ".vcpkg-root")) -and (Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Expected)) {
    Write-Host "Reusing staged libheif at $Root"
} else {
    if (Test-Path $Root) {
        Remove-Item -Recurse -Force $Root
    }

    $Staging = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-libheif-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $Staging | Out-Null
    $Zip = Join-Path $Staging $AssetName

    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing
    Expand-Archive -Path $Zip -DestinationPath $Staging -Force

    $Installed = Join-Path $Root "installed\$Triplet"
    $Include = Join-Path $Installed "include"
    $Lib = Join-Path $Installed "lib"
    $InfoDir = Join-Path $Root "installed\vcpkg\info"
    $Updates = Join-Path $Root "installed\vcpkg\updates"
    New-Item -ItemType Directory -Path $Include, $Lib, $InfoDir, $Updates -Force | Out-Null

    # Empty file marks a vcpkg root for vcpkg-rs.
    New-Item -ItemType File -Path (Join-Path $Root ".vcpkg-root") -Force | Out-Null

    Copy-Item -Recurse -Force (Join-Path $Staging "include\*") $Include
    Copy-Item -Force (Join-Path $Staging "lib\heif.lib") (Join-Path $Lib "heif.lib")
    Copy-Item -Force (Join-Path $Staging "lib\libde265.lib") (Join-Path $Lib "libde265.lib")
    # vcpkg-rs / MSVC expect x265.lib; the prebuilt archive uses x265-static.lib.
    Copy-Item -Force (Join-Path $Staging "lib\x265-static.lib") (Join-Path $Lib "x265.lib")

    $Status = @"
Package: libheif
Version: $LibheifVersion
Architecture: $Triplet
Multi-Arch: same
Description: Prebuilt MSVC static libheif (HEVC) for Easel CI
Status: install ok installed

"@
    Set-Content -Path (Join-Path $Root "installed\vcpkg\status") -Value $Status -Encoding ascii

    $ListPath = Join-Path $InfoDir "libheif_${LibheifVersion}_${Triplet}.list"
    $ListLines = @(
        "$Triplet/include/",
        "$Triplet/lib/",
        "$Triplet/lib/heif.lib",
        "$Triplet/lib/libde265.lib",
        "$Triplet/lib/x265.lib"
    )
    Get-ChildItem -Recurse -File $Include | ForEach-Object {
        $rel = $_.FullName.Substring($Installed.Length + 1).Replace("\", "/")
        $ListLines += "$Triplet/$rel"
    }
    Set-Content -Path $ListPath -Value ($ListLines -join "`n") -Encoding ascii

    Set-Content -Path $Marker -Value $Expected -Encoding ascii
    Remove-Item -Recurse -Force $Staging
    Write-Host "Staged libheif $LibheifVersion into $Root"
}

$env:VCPKG_ROOT = $Root
if ($env:GITHUB_ENV) {
    "VCPKG_ROOT=$Root" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
Write-Host "VCPKG_ROOT=$Root"
