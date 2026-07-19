# Resolve the release tag to publish for a libheif version.
#
# Modes:
#   first     — base tag libheif-vX.Y.Z if creatable; otherwise fail (manual -rN via dispatch)
#   rebuild   — next free libheif-vX.Y.Z-rN (skips live tags/releases and immutable-burned names)
#   exact     — use -ExactTag as-is after validating availability
#
# Burned immutable tags (deleted published releases) still block ref creation.
# We detect that with a create-ref probe + delete, never by retrying gh release create.
#
# IMPORTANT: Do not return empty .NET collections from PowerShell functions —
# an empty HashSet enumerates to nothing and becomes $null.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("first", "rebuild", "exact")]
    [string]$Mode,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$ExactTag = "",

    [string]$Repo = "",

    [string]$HeadSha = "",

    # Optional path: write the chosen tag here (avoids pipeline capture issues).
    [string]$OutFile = ""
)

$ErrorActionPreference = "Stop"

if (-not $Repo) {
    $Repo = $env:GITHUB_REPOSITORY
}
if (-not $Repo) {
    throw "Repo required (pass -Repo or set GITHUB_REPOSITORY)"
}

$Base = "libheif-v$Version"

# Hashtable keyed by tag name — never returned from a function (stays in script scope).
$script:Occupied = @{}

function Add-OccupiedTag([string]$Name) {
    if ($Name) { $script:Occupied[$Name] = $true }
}

function Test-OccupiedTag([string]$Name) {
    return $script:Occupied.ContainsKey($Name)
}

function Collect-OccupiedTags([string]$BaseTag) {
    $releasesJson = gh release list --repo $Repo --limit 200 --json tagName 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "gh release list failed: $releasesJson"
    }
    $releases = $releasesJson | ConvertFrom-Json
    foreach ($r in @($releases)) {
        if ($null -eq $r) { continue }
        $name = [string]$r.tagName
        if ($name -eq $BaseTag -or $name -match "^$([regex]::Escape($BaseTag))-r\d+$") {
            Add-OccupiedTag $name
        }
    }

    $refsJson = gh api "repos/$Repo/git/matching-refs/tags/$BaseTag" 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "gh api matching-refs failed: $refsJson"
    }
    $refs = $refsJson | ConvertFrom-Json
    foreach ($ref in @($refs)) {
        if ($null -eq $ref) { continue }
        $name = ([string]$ref.ref) -replace '^refs/tags/', ''
        if ($name -eq $BaseTag -or $name -match "^$([regex]::Escape($BaseTag))-r\d+$") {
            Add-OccupiedTag $name
        }
    }
}

function Test-TagCreatable([string]$Tag, [string]$Sha) {
    gh release view $Tag --repo $Repo 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { return $false }

    gh api "repos/$Repo/git/ref/tags/$Tag" 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { return $false }

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

function Get-MaxRebuildNumber([string]$BaseTag) {
    $max = 0
    foreach ($name in @($script:Occupied.Keys)) {
        if ($name -match "^$([regex]::Escape($BaseTag))-r(\d+)$") {
            $n = [int]$Matches[1]
            if ($n -gt $max) { $max = $n }
        }
    }
    return $max
}

function Emit-Tag([string]$Tag) {
    if ($OutFile) {
        Set-Content -Path $OutFile -Value $Tag -NoNewline -Encoding utf8
    }
    # Single string to stdout for callers that capture pipeline output.
    Write-Output $Tag
}

if (-not $HeadSha) {
    $HeadSha = (gh api "repos/$Repo/commits/main" --jq .sha)
}

Collect-OccupiedTags $Base
Write-Host ("occupied tags: " + (($script:Occupied.Keys | Sort-Object) -join ", "))

switch ($Mode) {
    "exact" {
        if (-not $ExactTag) { throw "-ExactTag required for mode=exact" }
        if ($ExactTag -notmatch "^libheif-v$([regex]::Escape($Version))(-r\d+)?$") {
            throw "exact tag $ExactTag does not match libheif-v$Version"
        }
        if (-not (Test-TagCreatable $ExactTag $HeadSha)) {
            throw "exact tag $ExactTag is occupied or immutable-burned; push a new libheif-v$Version-rN tag"
        }
        Emit-Tag $ExactTag
    }
    "first" {
        if ($script:Occupied.Count -gt 0) {
            $list = ($script:Occupied.Keys | Sort-Object) -join ", "
            throw "a release/tag for $Base already exists ($list); use workflow_dispatch for a -rN repack"
        }
        if (-not (Test-TagCreatable $Base $HeadSha)) {
            throw "base tag $Base is immutable-burned; use workflow_dispatch for a -rN repack"
        }
        Emit-Tag $Base
    }
    "rebuild" {
        $n = (Get-MaxRebuildNumber $Base) + 1
        if ($n -lt 1) { $n = 1 }
        $limit = $n + 200
        Write-Host "rebuild search starting at ${Base}-r${n}"
        while ($n -le $limit) {
            $candidate = "$Base-r$n"
            if (Test-OccupiedTag $candidate) {
                $n++
                continue
            }
            if (Test-TagCreatable $candidate $HeadSha) {
                Emit-Tag $candidate
                return
            }
            $n++
        }
        throw "could not find a free rebuild tag under $Base through -r$limit"
    }
}
