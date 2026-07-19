#!/usr/bin/env python3
"""Align versions.json with upstream libheif + the matching microsoft/vcpkg port.

Looks up the latest strukturag/libheif release, finds the newest microsoft/vcpkg
commit whose ports/libheif/vcpkg.json reports that version (or newer), and rewrites
versions.json when our pin is behind. Emits GitHub Actions outputs.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from typing import Any


UPSTREAM_LIBHEIF = "strukturag/libheif"
VCPKG_REPO = "microsoft/vcpkg"
VCPKG_PORT_PATH = "ports/libheif/vcpkg.json"


def gh_api(path: str, token: str | None = None) -> Any:
    url = path if path.startswith("http") else f"https://api.github.com{path}"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "easel-deps-sync-upstream",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.load(resp)


def parse_version(tag: str) -> tuple[int, ...]:
    raw = tag.lstrip("vV")
    parts = re.split(r"[^0-9]+", raw)
    nums = tuple(int(p) for p in parts if p.isdigit())
    if not nums:
        raise ValueError(f"unparseable version: {tag!r}")
    return nums


def version_ge(a: str, b: str) -> bool:
    return parse_version(a) >= parse_version(b)


def latest_libheif_release(token: str | None) -> str:
    data = gh_api(f"/repos/{UPSTREAM_LIBHEIF}/releases/latest", token)
    tag = data["tag_name"]
    return tag.lstrip("vV")


def vcpkg_port_at(ref: str, token: str | None) -> dict[str, Any]:
    meta = gh_api(
        f"/repos/{VCPKG_REPO}/contents/{VCPKG_PORT_PATH}?ref={ref}", token
    )
    import base64

    text = base64.b64decode(meta["content"]).decode("utf-8")
    return json.loads(text)


def find_vcpkg_commit_for(version: str, token: str | None) -> tuple[str, str]:
    """Return (commit_sha, port_version) for the newest commit with port >= version."""
    commits = gh_api(
        f"/repos/{VCPKG_REPO}/commits?path={VCPKG_PORT_PATH}&per_page=30", token
    )
    best: tuple[str, str] | None = None
    for c in commits:
        sha = c["sha"]
        try:
            port = vcpkg_port_at(sha, token)
        except urllib.error.HTTPError:
            continue
        port_ver = port.get("version") or port.get("version-string")
        if not port_ver:
            continue
        if version_ge(port_ver, version):
            # First page is newest-first; keep the newest matching commit.
            if best is None or version_ge(port_ver, best[1]):
                best = (sha, port_ver)
                # Newest commit that satisfies is enough.
                break
    if best is None:
        raise SystemExit(
            f"no microsoft/vcpkg commit found with libheif port >= {version}"
        )
    return best


def write_output(path: str | None, key: str, value: str) -> None:
    line = f"{key}={value}"
    print(line)
    if not path:
        return
    with open(path, "a", encoding="utf-8") as fh:
        if "\n" in value:
            delim = "EOF"
            while delim in value:
                delim += "X"
            fh.write(f"{key}<<{delim}\n{value}\n{delim}\n")
        else:
            fh.write(line + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--versions", default="versions.json")
    parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT"))
    parser.add_argument(
        "--token",
        default=os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN"),
    )
    args = parser.parse_args()

    with open(args.versions, encoding="utf-8") as fh:
        versions = json.load(fh)

    current = versions["libheif"]["version"]
    upstream = latest_libheif_release(args.token)
    print(f"pinned libheif={current} upstream={upstream}", file=sys.stderr)

    # Target the max(pinned, upstream) so we never silently downgrade, but still
    # repair a pin that claims a version vcpkg cannot yet build.
    target = upstream if version_ge(upstream, current) else current
    sha, port_ver = find_vcpkg_commit_for(target, args.token)
    print(f"vcpkg commit={sha[:12]} port={port_ver}", file=sys.stderr)

    # Prefer a dated monthly tag when it already contains the port version.
    vcpkg_ref = sha
    vcpkg_ref_kind = "commit"
    try:
        tags = gh_api(f"/repos/{VCPKG_REPO}/tags?per_page=20", args.token)
        for t in tags:
            name = t["name"]
            if not re.match(r"^\d{4}\.\d{2}\.\d{2}$", name):
                continue
            try:
                tagged_port = vcpkg_port_at(name, args.token)
            except urllib.error.HTTPError:
                continue
            tagged_ver = tagged_port.get("version") or tagged_port.get("version-string")
            if tagged_ver and version_ge(tagged_ver, port_ver):
                vcpkg_ref = name
                vcpkg_ref_kind = "tag"
                break
    except urllib.error.HTTPError as exc:
        print(f"tag probe skipped: {exc}", file=sys.stderr)

    old_ref = versions.get("vcpkg", {}).get("ref") or versions.get("vcpkg", {}).get(
        "tag"
    )
    changed = (port_ver != current) or (old_ref != vcpkg_ref)

    versions["libheif"]["version"] = port_ver
    versions.setdefault("vcpkg", {})
    versions["vcpkg"]["git"] = f"https://github.com/{VCPKG_REPO}"
    versions["vcpkg"]["ref"] = vcpkg_ref
    versions["vcpkg"]["ref_kind"] = vcpkg_ref_kind
    versions["vcpkg"].pop("tag", None)
    if vcpkg_ref_kind == "commit":
        versions["vcpkg"]["comment"] = (
            f"microsoft/vcpkg commit whose libheif port is {port_ver} "
            "(no monthly tag with this port yet)."
        )
    else:
        versions["vcpkg"]["comment"] = (
            f"microsoft/vcpkg tag {vcpkg_ref} includes libheif {port_ver}."
        )

    if changed:
        with open(args.versions, "w", encoding="utf-8") as fh:
            json.dump(versions, fh, indent=2)
            fh.write("\n")

    pr_body = (
        f"Automated bump from `{current}` → `{port_ver}`.\n\n"
        f"- Upstream libheif latest: `{upstream}`\n"
        f"- vcpkg {vcpkg_ref_kind}: `{vcpkg_ref}`\n"
        f"- Port version at that ref: `{port_ver}`\n\n"
        "Merging triggers **Build libheif (Windows MSVC)** which republishes "
        f"`libheif-v{port_ver}`."
    )
    with open("pr-body.md", "w", encoding="utf-8") as fh:
        fh.write(pr_body)

    write_output(args.github_output, "changed", "true" if changed else "false")
    write_output(args.github_output, "libheif", port_ver)
    write_output(args.github_output, "vcpkg_ref", vcpkg_ref)
    write_output(args.github_output, "vcpkg_ref_kind", vcpkg_ref_kind)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
