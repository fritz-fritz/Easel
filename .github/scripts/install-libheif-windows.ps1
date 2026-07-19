# Install prebuilt MSVC static libheif into a minimal vcpkg tree for libheif-sys.
#
# Preferred source: fritz-fritz/easel-deps GitHub Releases (owned prebuilds),
# pinned by .github/libheif-windows.lock.json (checksum + version verified).
# Interim fallback: vegidio/binaries-heif only when the pinned easel-deps asset
# is missing or fails version verification (e.g. mis-tagged first release).
#
# Usage (from repo root, PowerShell):
#   .\.github\scripts\install-libheif-windows.ps1

$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    if ($PSScriptRoot) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return $PWD.Path
}

$RepoRoot = Get-RepoRoot
$LockPath = Join-Path $RepoRoot ".github\libheif-windows.lock.json"
if (-not (Test-Path $LockPath)) {
    throw "missing lock file: $LockPath"
}
$Lock = Get-Content $LockPath -Raw | ConvertFrom-Json

$LibheifVersion = [string]$Lock.version
$Triplet = if ($Lock.triplet) { [string]$Lock.triplet } else { "x64-windows-static-md" }
$EaselDepsRepo = if ($Lock.repo) { [string]$Lock.repo } else { "fritz-fritz/easel-deps" }
$EaselDepsTag = [string]$Lock.tag
$EaselDepsAsset = [string]$Lock.asset
$ExpectedSha256 = if ($Lock.sha256) { ([string]$Lock.sha256).ToLowerInvariant() } else { $null }
$EaselDepsUrl = "https://github.com/$EaselDepsRepo/releases/download/$EaselDepsTag/$EaselDepsAsset"

# Interim fallback (source-only upstream; third-party MSVC zip).
$FallbackTag = "26.7.0"
$FallbackAsset = "static_windows_x64.zip"
$FallbackUrl = "https://github.com/vegidio/binaries-heif/releases/download/$FallbackTag/$FallbackAsset"
$FallbackLibheifVersion = "1.23.0"
$FallbackSha256 = "6021c0643460e525a4ece60b15eecfa4f772d7448cefb178552861c032a8a870"

$Root = if ($env:EASEL_VCPKG_ROOT) {
    $env:EASEL_VCPKG_ROOT
} elseif ($env:RUNNER_TEMP) {
    Join-Path $env:RUNNER_TEMP "easel-vcpkg"
} else {
    Join-Path $PWD "target\easel-vcpkg"
}

function Test-UrlExists([string]$Url) {
    try {
        $resp = Invoke-WebRequest -Uri $Url -Method Get -UseBasicParsing -Headers @{ Range = "bytes=0-0" }
        return ($resp.StatusCode -ge 200 -and $resp.StatusCode -lt 400)
    } catch {
        try {
            $resp = Invoke-WebRequest -Uri $Url -Method Head -UseBasicParsing -MaximumRedirection 5
            return ($resp.StatusCode -ge 200 -and $resp.StatusCode -lt 400)
        } catch {
            return $false
        }
    }
}

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLowerInvariant()
}

function Assert-Sha256([string]$Path, [string]$Expected) {
    if (-not $Expected) { return }
    $actual = Get-FileSha256 $Path
    if ($actual -ne $Expected.ToLowerInvariant()) {
        throw "SHA256 mismatch for $(Split-Path -Leaf $Path): expected $Expected got $actual"
    }
}

function Get-StagedLibheifVersion([string]$DestRoot) {
    $hdr = Join-Path $DestRoot "installed\$Triplet\include\libheif\heif_version.h"
    if (Test-Path $hdr) {
        $text = Get-Content $hdr -Raw
        if ($text -match 'LIBHEIF_VERSION\s+"([^"]+)"') {
            return $Matches[1]
        }
    }
    $status = Join-Path $DestRoot "installed\vcpkg\status"
    if (Test-Path $status) {
        $blockMatch = Select-String -Path $status -Pattern '(?ms)^Package:\s*libheif\s*$.*?^Version:\s*(\S+)' -AllMatches
        foreach ($m in $blockMatch.Matches) {
            # Prefer the non-feature package stanza (first Version after Package: libheif).
            return $m.Groups[1].Value
        }
    }
    return $null
}

function Install-FromEaselDepsZip([string]$ZipPath, [string]$DestRoot, [string]$ExpectedVersion) {
    if (Test-Path $DestRoot) { Remove-Item -Recurse -Force $DestRoot }
    $Staging = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-deps-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $Staging | Out-Null
    Expand-Archive -Path $ZipPath -DestinationPath $Staging -Force
    New-Item -ItemType Directory -Path $DestRoot -Force | Out-Null
    Copy-Item -Recurse -Force (Join-Path $Staging "*") $DestRoot
    Remove-Item -Recurse -Force $Staging
    if (-not (Test-Path (Join-Path $DestRoot ".vcpkg-root"))) {
        throw "easel-deps zip missing .vcpkg-root"
    }
    if (-not (Test-Path (Join-Path $DestRoot "installed\$Triplet\lib\heif.lib"))) {
        throw "easel-deps zip missing heif.lib"
    }
    $actual = Get-StagedLibheifVersion $DestRoot
    if (-not $actual) {
        throw "easel-deps zip missing libheif version metadata"
    }
    if ($actual -ne $ExpectedVersion) {
        throw "easel-deps asset claims $ExpectedVersion but heif_version.h/status report $actual"
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
    "easel-deps|$EaselDepsTag|$EaselDepsAsset|$ExpectedSha256|$LibheifVersion"
} else {
    "fallback|$FallbackTag|$FallbackAsset|$FallbackSha256"
}

if ((Test-Path (Join-Path $Root ".vcpkg-root")) -and (Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Expected)) {
    Write-Host "Reusing staged libheif at $Root ($Expected)"
} else {
    $DlDir = Join-Path ([System.IO.Path]::GetTempPath()) ("easel-heif-dl-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $DlDir | Out-Null
    $installedOk = $false
    try {
        if ($UseEaselDeps) {
            $Zip = Join-Path $DlDir $EaselDepsAsset
            Write-Host "Downloading easel-deps asset $EaselDepsUrl"
            Invoke-WebRequest -Uri $EaselDepsUrl -OutFile $Zip -UseBasicParsing
            try {
                Assert-Sha256 -Path $Zip -Expected $ExpectedSha256
                Install-FromEaselDepsZip -ZipPath $Zip -DestRoot $Root -ExpectedVersion $LibheifVersion
                $installedOk = $true
                Set-Content -Path $Marker -Value $Expected -Encoding ascii
            } catch {
                Write-Warning "easel-deps asset rejected: $($_.Exception.Message)"
                Write-Warning "Falling back to interim third-party zip. Republish easel-deps with a matching vcpkg ref."
            }
        }
        if (-not $installedOk) {
            if ($UseEaselDeps) {
                Write-Warning "Pinned easel-deps release was unusable ($EaselDepsUrl)."
            } else {
                Write-Warning "easel-deps release not found yet ($EaselDepsUrl)."
            }
            $Zip = Join-Path $DlDir $FallbackAsset
            Write-Host "Downloading fallback $FallbackUrl"
            Invoke-WebRequest -Uri $FallbackUrl -OutFile $Zip -UseBasicParsing
            Assert-Sha256 -Path $Zip -Expected $FallbackSha256
            Install-FromFallbackZip -ZipPath $Zip -DestRoot $Root
            Set-Content -Path $Marker -Value "fallback|$FallbackTag|$FallbackAsset|$FallbackSha256" -Encoding ascii
        }
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
