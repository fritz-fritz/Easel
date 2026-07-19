# Resolve the release tag to publish for a libheif version.
#
# Modes:
#   first     — base tag libheif-vX.Y.Z if creatable; otherwise fail (manual -rN via dispatch)
#   rebuild   — next free libheif-vX.Y.Z-rN (skips live tags/releases and immutable-burned names)
#   exact     — use -ExactTag as-is after validating availability
#
# Burned immutable tags (deleted published releases) still block ref creation.
# We detect that with a create-ref probe + delete, never by retrying gh release create.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("first", "rebuild", "exact")]
    [string]$Mode,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$ExactTag = "",

    [string]$Repo = "",

    [string]$HeadSha = ""
)

$ErrorActionPreference = "Stop"

if (-not $Repo) {
    $Repo = $env:GITHUB_REPOSITORY
}
if (-not $Repo) {
    throw "Repo required (pass -Repo or set GITHUB_REPOSITORY)"
}

$Base = "libheif-v$Version"

function New-TagSet {
    return [System.Collections.Generic.HashSet[string]]::new(
        [StringComparer]::OrdinalIgnoreCase
    )
}

function Get-OccupiedTags([string]$BaseTag) {
    $occupied = New-TagSet

    $releases = gh release list --repo $Repo --limit 200 --json tagName | ConvertFrom-Json
    foreach ($r in @($releases)) {
        if ($null -eq $r) { continue }
        $name = [string]$r.tagName
        if ($name -eq $BaseTag -or $name -match "^$([regex]::Escape($BaseTag))-r\d+$") {
            [void]$occupied.Add($name)
        }
    }

    $refs = gh api "repos/$Repo/git/matching-refs/tags/$BaseTag" | ConvertFrom-Json
    foreach ($ref in @($refs)) {
        if ($null -eq $ref) { continue }
        $name = ([string]$ref.ref) -replace '^refs/tags/', ''
        if ($name -eq $BaseTag -or $name -match "^$([regex]::Escape($BaseTag))-r\d+$") {
            [void]$occupied.Add($name)
        }
    }

    # Leading comma stops PowerShell from unrolling HashSet -> Object[] (fixed-size).
    return , $occupied
}

function Test-TagCreatable([string]$Tag, [string]$Sha) {
    gh release view $Tag --repo $Repo 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { return $false }

    gh api "repos/$Repo/git/ref/tags/$Tag" 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { return $false }

    # Immutable-burned names reject ref creation even when no tag/release exists.
    $createOut = gh api -X POST "repos/$Repo/git/refs" `
        -f ref="refs/tags/$Tag" `
        -f sha=$Sha 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "tag probe ${Tag}: burned or blocked ($createOut)"
        return $false
    }

    gh api -X DELETE "repos/$Repo/git/refs/tags/$Tag" 2>$null | Out-Null
    return $true
}

function Get-MaxRebuildNumber($Occupied, [string]$BaseTag) {
    $max = 0
    foreach ($name in @($Occupied)) {
        if ([string]$name -match "^$([regex]::Escape($BaseTag))-r(\d+)$") {
            $n = [int]$Matches[1]
            if ($n -gt $max) { $max = $n }
        }
    }
    return $max
}

if (-not $HeadSha) {
    $HeadSha = (gh api "repos/$Repo/commits/main" --jq .sha)
}

$occupied = Get-OccupiedTags $Base

switch ($Mode) {
    "exact" {
        if (-not $ExactTag) { throw "-ExactTag required for mode=exact" }
        if ($ExactTag -notmatch "^libheif-v$([regex]::Escape($Version))(-r\d+)?$") {
            throw "exact tag $ExactTag does not match libheif-v$Version"
        }
        if (-not (Test-TagCreatable $ExactTag $HeadSha)) {
            throw "exact tag $ExactTag is occupied or immutable-burned; push a new libheif-v$Version-rN tag"
        }
        Write-Output $ExactTag
    }
    "first" {
        if ($occupied.Count -gt 0) {
            $list = (@($occupied) | Sort-Object) -join ", "
            throw "a release/tag for $Base already exists ($list); use workflow_dispatch for a -rN repack"
        }
        if (-not (Test-TagCreatable $Base $HeadSha)) {
            throw "base tag $Base is immutable-burned; use workflow_dispatch for a -rN repack"
        }
        Write-Output $Base
    }
    "rebuild" {
        # Walk upward from max(live)+1. Burned holes are skipped via create-ref
        # probes; no need to mutate $occupied (avoids fixed-size-array pitfalls).
        $n = (Get-MaxRebuildNumber $occupied $Base) + 1
        if ($n -lt 1) { $n = 1 }
        $limit = $n + 200
        while ($n -le $limit) {
            $candidate = "$Base-r$n"
            if ($occupied.Contains($candidate)) {
                $n++
                continue
            }
            if (Test-TagCreatable $candidate $HeadSha) {
                Write-Output $candidate
                return
            }
            $n++
        }
        throw "could not find a free rebuild tag under $Base through -r$limit"
    }
}
