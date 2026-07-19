# Install prebuilt MSVC static libheif into a minimal vcpkg tree for libheif-sys.
#
# Preferred source: fritz-fritz/easel-deps GitHub Releases (owned prebuilds).
# Interim fallback: vegidio/binaries-heif (HEVC only; no aom) if easel-deps has
# not published the pinned asset yet.
#
# Usage (from repo root, PowerShell):
#   .\.github\scripts\install-libheif-windows.ps1

$ErrorActionPreference = "Stop"

# Keep in sync with tools/easel-deps/versions.json
$LibheifVersion = "1.23.1"
$Triplet = "x64-windows-static-md"
$EaselDepsTag = "libheif-v$LibheifVersion"
$EaselDepsAsset = "libheif-msvc-$Triplet-$LibheifVersion.zip"
$EaselDepsUrl = "https://github.com/fritz-fritz/easel-deps/releases/download/$EaselDepsTag/$EaselDepsAsset"

# Interim fallback (source-only upstream; third-party MSVC zip).
$FallbackTag = "26.7.0"
$FallbackAsset = "static_windows_x64.zip"
$FallbackUrl = "https://github.com/vegidio/binaries-heif/releases/download/$FallbackTag/$FallbackAsset"
$FallbackLibheifVersion = "1.23.0"

$Root = if ($env:EASEL_VCPKG_ROOT) {
    $env:EASEL_VCPKG_ROOT
} elseif ($env:RUNNER_TEMP) {
    Join-Path $env:RUNNER_TEMP "easel-vcpkg"
} else {
    Join-Path $PWD "target\easel-vcpkg"
}

function Test-UrlExists([string]$Url) {
    try {
        $resp = Invoke-WebRequest -Uri $Url -Method Head -UseBasicParsing -MaximumRedirection 5
        return ($resp.StatusCode -ge 200 -and $resp.StatusCode -lt 400)
    } catch {
        return $false
    }
}

function Install-FromEaselDepsZip([string]$ZipPath, [string]$DestRoot) {
    if (Test-Path $DestRoot) { Remove-Item -Recurse -Force $DestRoot }
    $Staging = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-deps-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $Staging | Out-Null
    Expand-Archive -Path $ZipPath -DestinationPath $Staging -Force
    # Zip root already contains .vcpkg-root + installed/
    New-Item -ItemType Directory -Path $DestRoot -Force | Out-Null
    Copy-Item -Recurse -Force (Join-Path $Staging "*") $DestRoot
    Remove-Item -Recurse -Force $Staging
    if (-not (Test-Path (Join-Path $DestRoot ".vcpkg-root"))) {
        throw "easel-deps zip missing .vcpkg-root"
    }
    if (-not (Test-Path (Join-Path $DestRoot "installed\$Triplet\lib\heif.lib"))) {
        throw "easel-deps zip missing heif.lib"
    }
}

function Install-FromFallbackZip([string]$ZipPath, [string]$DestRoot) {
    if (Test-Path $DestRoot) { Remove-Item -Recurse -Force $DestRoot }
    $Staging = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-libheif-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $Staging | Out-Null
    Expand-Archive -Path $ZipPath -DestinationPath $Staging -Force

    $Installed = Join-Path $DestRoot "installed\$Triplet"
    $Include = Join-Path $Installed "include"
    $Lib = Join-Path $Installed "lib"
    $InfoDir = Join-Path $DestRoot "installed\vcpkg\info"
    $Updates = Join-Path $DestRoot "installed\vcpkg\updates"
    New-Item -ItemType Directory -Path $Include, $Lib, $InfoDir, $Updates -Force | Out-Null
    New-Item -ItemType File -Path (Join-Path $DestRoot ".vcpkg-root") -Force | Out-Null

    Copy-Item -Recurse -Force (Join-Path $Staging "include\*") $Include
    Copy-Item -Force (Join-Path $Staging "lib\heif.lib") (Join-Path $Lib "heif.lib")
    Copy-Item -Force (Join-Path $Staging "lib\libde265.lib") (Join-Path $Lib "libde265.lib")
    Copy-Item -Force (Join-Path $Staging "lib\x265-static.lib") (Join-Path $Lib "x265-static.lib")

    $Status = @"
Package: libheif
Version: $FallbackLibheifVersion
Architecture: $Triplet
Multi-Arch: same
Description: Interim third-party MSVC static libheif (HEVC) for Easel CI
Status: install ok installed

"@
    Set-Content -Path (Join-Path $DestRoot "installed\vcpkg\status") -Value $Status -Encoding ascii

    $ListPath = Join-Path $InfoDir "libheif_${FallbackLibheifVersion}_${Triplet}.list"
    $ListLines = @(
        "$Triplet/include/",
        "$Triplet/lib/",
        "$Triplet/lib/heif.lib",
        "$Triplet/lib/libde265.lib",
        "$Triplet/lib/x265-static.lib"
    )
    Get-ChildItem -Recurse -File $Include | ForEach-Object {
        $rel = $_.FullName.Substring($Installed.Length + 1).Replace("\", "/")
        $ListLines += "$Triplet/$rel"
    }
    Set-Content -Path $ListPath -Value ($ListLines -join "`n") -Encoding ascii
    Remove-Item -Recurse -Force $Staging
}

$Marker = Join-Path $Root ".easel-libheif-prebuilt"
$UseEaselDeps = Test-UrlExists $EaselDepsUrl
$Expected = if ($UseEaselDeps) {
    "easel-deps|$EaselDepsTag|$EaselDepsAsset"
} else {
    "fallback|$FallbackTag|$FallbackAsset"
}

if ((Test-Path (Join-Path $Root ".vcpkg-root")) -and (Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Expected)) {
    Write-Host "Reusing staged libheif at $Root ($Expected)"
} else {
    $DlDir = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-heif-dl-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $DlDir | Out-Null
    try {
        if ($UseEaselDeps) {
            $Zip = Join-Path $DlDir $EaselDepsAsset
            Write-Host "Downloading easel-deps asset $EaselDepsUrl"
            Invoke-WebRequest -Uri $EaselDepsUrl -OutFile $Zip -UseBasicParsing
            Install-FromEaselDepsZip -ZipPath $Zip -DestRoot $Root
        } else {
            Write-Warning "easel-deps release not found yet ($EaselDepsUrl). Using interim third-party fallback. Push tools/easel-deps to fritz-fritz/easel-deps and run its build workflow."
            $Zip = Join-Path $DlDir $FallbackAsset
            Write-Host "Downloading fallback $FallbackUrl"
            Invoke-WebRequest -Uri $FallbackUrl -OutFile $Zip -UseBasicParsing
            Install-FromFallbackZip -ZipPath $Zip -DestRoot $Root
        }
        Set-Content -Path $Marker -Value $Expected -Encoding ascii
        Write-Host "Staged libheif into $Root"
    } finally {
        Remove-Item -Recurse -Force $DlDir -ErrorAction SilentlyContinue
    }
}

$env:VCPKG_ROOT = $Root
if ($env:GITHUB_ENV) {
    "VCPKG_ROOT=$Root" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
Write-Host "VCPKG_ROOT=$Root"
