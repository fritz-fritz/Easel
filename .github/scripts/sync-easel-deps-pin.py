#!/usr/bin/env python3
"""Update .github/libheif-windows.lock.json from fritz-fritz/easel-deps releases.

Rejects assets whose published version metadata cannot be confirmed (missing
SHA256 sidecar) and rewrites the rust-cache prefix-key in ci.yml to match.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.request
from typing import Any


DEFAULT_REPO = "fritz-fritz/easel-deps"
CI_YML = ".github/workflows/ci.yml"
PREFIX_RE = re.compile(
    r"(prefix-key:\s*)easel-deps-libheif-v[0-9]+(?:\.[0-9]+)*",
    re.MULTILINE,
)


def gh_api(path: str, token: str | None = None) -> Any:
    url = path if path.startswith("http") else f"https://api.github.com{path}"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "easel-sync-easel-deps-pin",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.load(resp)


def download_text(url: str, token: str | None = None) -> str:
    headers = {"User-Agent": "easel-sync-easel-deps-pin"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=60) as resp:
        return resp.read().decode("utf-8")


def write_output(path: str | None, key: str, value: str) -> None:
    print(f"{key}={value if chr(10) not in value else value.splitlines()[0] + '…'}")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as fh:
        if "\n" in value:
            delim = "EOF"
            while delim in value:
                delim += "X"
            fh.write(f"{key}<<{delim}\n{value}\n{delim}\n")
        else:
            fh.write(f"{key}={value}\n")


def parse_sha256_sidecar(text: str, asset_name: str) -> str | None:
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) >= 2 and parts[-1].endswith(asset_name):
            return parts[0].lower()
        if len(parts) == 1 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            return parts[0].lower()
    return None


def latest_libheif_release(repo: str, token: str | None) -> dict[str, Any]:
    releases = gh_api(f"/repos/{repo}/releases?per_page=20", token)
    for rel in releases:
        tag = rel.get("tag_name") or ""
        if re.fullmatch(r"libheif-v\d+\.\d+\.\d+", tag):
            return rel
    raise SystemExit(f"no libheif-v* release found on {repo}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", default=".github/libheif-windows.lock.json")
    parser.add_argument("--ci-yml", default=CI_YML)
    parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT"))
    parser.add_argument(
        "--token",
        default=os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN"),
    )
    args = parser.parse_args()

    with open(args.lock, encoding="utf-8") as fh:
        lock = json.load(fh)

    repo = lock.get("repo") or DEFAULT_REPO
    rel = latest_libheif_release(repo, args.token)
    tag = rel["tag_name"]
    version = tag.removeprefix("libheif-v")
    triplet = lock.get("triplet") or "x64-windows-static-md"
    asset_name = f"libheif-msvc-{triplet}-{version}.zip"

    assets = {a["name"]: a for a in rel.get("assets", [])}
    if asset_name not in assets:
        raise SystemExit(f"release {tag} missing asset {asset_name}")

    # Require the .sha256 sidecar produced by the fixed publisher. The first
    # libheif-v1.23.1 cut had an API digest but packaged port 1.21.2 — refuse it.
    sidecar = assets.get(f"{asset_name}.sha256")
    if not sidecar:
        print(
            f"release {tag} has no {asset_name}.sha256 sidecar; "
            "waiting for a corrected easel-deps publish",
            file=sys.stderr,
        )
        write_output(args.github_output, "changed", "false")
        write_output(args.github_output, "tag", tag)
        return 0

    text = download_text(sidecar["browser_download_url"], args.token)
    sha256 = parse_sha256_sidecar(text, asset_name)
    if not sha256:
        raise SystemExit(f"could not parse SHA256 from {asset_name}.sha256")

    new_lock = {
        "repo": repo,
        "tag": tag,
        "version": version,
        "triplet": triplet,
        "asset": asset_name,
        "sha256": sha256,
    }

    changed = any(
        new_lock[k] != lock.get(k) for k in ("repo", "tag", "version", "triplet", "asset", "sha256")
    )

    if changed:
        with open(args.lock, "w", encoding="utf-8") as fh:
            json.dump(new_lock, fh, indent=2)
            fh.write("\n")

        with open(args.ci_yml, encoding="utf-8") as fh:
            ci = fh.read()
        ci2, n = PREFIX_RE.subn(rf"\1easel-deps-libheif-v{version}", ci)
        if n == 0:
            raise SystemExit("failed to update rust-cache prefix-key in ci.yml")
        with open(args.ci_yml, "w", encoding="utf-8") as fh:
            fh.write(ci2)

    pr_body = (
        f"Automated pin update from easel-deps release `{tag}`.\n\n"
        f"- Asset: `{asset_name}`\n"
        f"- SHA256: `{sha256}`\n"
        f"- rust-cache prefix-key: `easel-deps-libheif-v{version}`\n"
    )
    with open("pr-body.md", "w", encoding="utf-8") as fh:
        fh.write(pr_body)

    write_output(args.github_output, "changed", "true" if changed else "false")
    write_output(args.github_output, "tag", tag)
    write_output(args.github_output, "version", version)
    write_output(args.github_output, "sha256", sha256)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
