#!/usr/bin/env python3
# Copyright (c) contributors. MPL-2.0.
"""Build a dual-surface visual gallery from ci-visual artifacts.

Reads downloaded Actions artifacts (manifests + PNGs) and writes:
  - site/index.html (+ assets + PNGs) for GitHub Pages
  - comment.md for a sticky PR comment (Markdown tables + gallery link)
  - summary.json including cross-OS comparison results
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import re
import shutil
import struct
from collections import defaultdict
from pathlib import Path
from urllib.parse import quote


MARKER = "<!-- easel-ci-visual -->"
EXPECTED_OS = ("macos-latest", "ubuntu-latest", "windows-latest")
# Stages where byte-identical cross-OS output is expected (regression signal).
STRICT_STAGES = frozenset({"apply-payload"})

CSS = """\
@import url("https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;650&family=IBM+Plex+Mono:wght@400;500&display=swap");
:root {
  --bg: #12161c;
  --bg-elev: #1a212b;
  --ink: #eef2f6;
  --muted: #93a0af;
  --line: #2a3441;
  --accent: #3d9cf0;
  --good: #3cb879;
  --warn: #d4a017;
  --bad: #e35d6a;
  --chip: #243041;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  color: var(--ink);
  background:
    radial-gradient(900px 420px at 8% -8%, rgba(61, 156, 240, 0.14), transparent 60%),
    radial-gradient(700px 380px at 92% 0%, rgba(60, 184, 121, 0.10), transparent 55%),
    linear-gradient(180deg, #161b22 0%, var(--bg) 40%, #10141a 100%);
  line-height: 1.45;
  min-height: 100vh;
}
header, main { max-width: 1180px; margin: 0 auto; padding: 1.4rem 1.35rem; }
header h1 {
  margin: 0 0 0.4rem;
  font-size: clamp(1.45rem, 2.4vw, 1.85rem);
  font-weight: 650;
  letter-spacing: -0.02em;
}
header .lede { margin: 0 0 1rem; color: var(--muted); max-width: 52rem; }
.meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 0.65rem;
  margin: 0 0 0.35rem;
}
.meta-card {
  background: color-mix(in srgb, var(--bg-elev) 88%, black);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.7rem 0.85rem;
}
.meta-card .label {
  display: block;
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted);
  margin-bottom: 0.2rem;
}
.meta-card .value {
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.92rem;
  font-weight: 500;
  word-break: break-word;
}
.meta-card a { color: var(--accent); text-decoration: none; }
.meta-card a:hover { text-decoration: underline; }
.summary-banner {
  margin-top: 1rem;
  padding: 0.85rem 1rem;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--chip);
  display: flex;
  flex-wrap: wrap;
  gap: 0.55rem 0.9rem;
  align-items: center;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  border-radius: 999px;
  padding: 0.18rem 0.65rem;
  font-size: 0.78rem;
  font-weight: 600;
  border: 1px solid transparent;
}
.chip.match { background: rgba(60, 184, 121, 0.16); color: #8ee0b3; border-color: rgba(60, 184, 121, 0.35); }
.chip.mismatch { background: rgba(227, 93, 106, 0.16); color: #f5a3ab; border-color: rgba(227, 93, 106, 0.4); }
.chip.warn { background: rgba(212, 160, 23, 0.16); color: #f0d28a; border-color: rgba(212, 160, 23, 0.4); }
.chip.info { background: rgba(61, 156, 240, 0.14); color: #9cc9f5; border-color: rgba(61, 156, 240, 0.35); }
section {
  margin: 1.35rem 0;
  padding: 1rem 1.05rem 1.15rem;
  background: color-mix(in srgb, var(--bg-elev) 92%, black);
  border: 1px solid var(--line);
  border-radius: 12px;
}
section h2 {
  margin: 0 0 0.35rem;
  font-size: 1.12rem;
  font-weight: 600;
}
section .stage-note { margin: 0 0 0.9rem; color: var(--muted); font-size: 0.9rem; }
.compare-wrap { overflow-x: auto; }
table.compare {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.86rem;
  min-width: 640px;
}
table.compare th, table.compare td {
  border: 1px solid var(--line);
  padding: 0.55rem 0.6rem;
  vertical-align: top;
  text-align: center;
}
table.compare th {
  background: #151c25;
  color: var(--muted);
  font-weight: 600;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
table.compare td.asset {
  text-align: left;
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.8rem;
  white-space: nowrap;
}
table.compare figure {
  margin: 0;
  background: #0b1015;
  border: 1px solid var(--line);
  border-radius: 8px;
  overflow: hidden;
}
table.compare img {
  display: block;
  width: 100%;
  max-width: 220px;
  height: auto;
  margin: 0 auto;
  background: #080b0f;
}
.cell-meta {
  margin-top: 0.35rem;
  color: var(--muted);
  font-family: "IBM Plex Mono", ui-monospace, monospace;
  font-size: 0.7rem;
  line-height: 1.35;
}
.status-cell { min-width: 7.5rem; }
.missing {
  color: var(--muted);
  font-style: italic;
  padding: 1.2rem 0.4rem;
}
details.raw {
  margin-top: 0.9rem;
  border-top: 1px solid var(--line);
  padding-top: 0.7rem;
}
details.raw summary {
  cursor: pointer;
  color: var(--muted);
  font-size: 0.88rem;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 0.75rem;
  margin-top: 0.75rem;
}
.grid figure {
  margin: 0;
  background: #0b1015;
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow: hidden;
}
.grid figcaption {
  padding: 0.5rem 0.65rem 0.65rem;
  font-size: 0.78rem;
  color: var(--muted);
}
.grid figcaption strong { color: var(--ink); font-weight: 600; }
dialog {
  border: 1px solid var(--line);
  border-radius: 12px;
  background: #0b1015;
  color: var(--ink);
  max-width: min(96vw, 1200px);
  padding: 0;
}
dialog::backdrop { background: rgba(0, 0, 0, 0.72); }
dialog img { max-width: 96vw; max-height: 88vh; display: block; }
dialog form { padding: 0.5rem; text-align: right; }
button {
  appearance: none;
  border: 1px solid var(--line);
  background: var(--bg-elev);
  color: var(--ink);
  border-radius: 8px;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
}
@media (max-width: 700px) {
  header, main { padding: 1rem; }
  table.compare { font-size: 0.8rem; }
}
"""

JS = """\
document.querySelectorAll("[data-lightbox]").forEach((link) => {
  link.addEventListener("click", (event) => {
    event.preventDefault();
    const dialog = document.getElementById("lightbox");
    const img = dialog.querySelector("img");
    img.src = link.href;
    img.alt = link.dataset.caption || "";
    dialog.showModal();
  });
});
"""


def load_manifests(artifact_root: Path) -> list[dict]:
    manifests: list[dict] = []
    for path in sorted(artifact_root.rglob("ci-visual-manifest-*.json")):
        manifests.append(json.loads(path.read_text(encoding="utf8")))
    return manifests


def png_meta(path: Path) -> dict:
    """Return byte size, sha256, and IHDR dimensions when available."""
    data = path.read_bytes()
    meta = {
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "width": None,
        "height": None,
    }
    if len(data) >= 24 and data[:8] == b"\x89PNG\r\n\x1a\n":
        meta["width"], meta["height"] = struct.unpack(">II", data[16:24])
    return meta


def collect_images(
    artifact_root: Path, manifests: list[dict]
) -> list[dict]:
    """Merge manifest entries with on-disk PNG paths and file metadata."""
    png_index = {p.name: p for p in artifact_root.rglob("*.png")}
    images: list[dict] = []
    for manifest in manifests:
        for image in manifest.get("images", []):
            try:
                filename = safe_filename(image["filename"])
            except ValueError:
                continue
            src = png_index.get(filename)
            if src is None:
                continue
            meta = png_meta(src)
            images.append(
                {
                    "stage": str(manifest["stage"]),
                    "os": str(manifest["os"]),
                    "filename": filename,
                    "stem": str(image.get("stem", Path(filename).stem)),
                    "src_path": src,
                    **meta,
                }
            )
    # Also pick up PNGs that lost their manifest (best-effort).
    known = {img["filename"] for img in images}
    for name, src in sorted(png_index.items()):
        if name in known or name.startswith("ci-visual-manifest-"):
            continue
        try:
            filename = safe_filename(name)
        except ValueError:
            continue
        stage, os_name, stem = infer_from_filename(filename)
        meta = png_meta(src)
        images.append(
            {
                "stage": stage,
                "os": os_name,
                "filename": filename,
                "stem": stem,
                "src_path": src,
                **meta,
            }
        )
    return images


def infer_from_filename(name: str) -> tuple[str, str, str]:
    stem = Path(name).stem
    # apply-payload-ubuntu-latest-apply-display-0
    m = re.match(
        r"^(?P<stage>.+?)-(?P<os>(?:ubuntu|windows|macos)-latest)(?:-(?P<rest>.+))?$",
        stem,
    )
    if m:
        return m.group("stage"), m.group("os"), m.group("rest") or stem
    return "unknown", "unknown", stem


def safe_filename(filename: str) -> str:
    """Return a basename-only filename, rejecting path traversal.

    Also reject Windows-style backslash paths: on POSIX, Path('a\\\\b').name is
    the full string, so a name!=filename check alone is not enough.
    """
    name = Path(filename).name
    if (
        not name
        or name in {".", ".."}
        or name != filename
        or "/" in filename
        or "\\" in filename
        or "\x00" in filename
    ):
        raise ValueError(f"unsafe artifact filename: {filename!r}")
    return name


def pages_url(
    pages_base: str,
    pr_number: str,
    filename: str,
    *,
    cache_buster: str = "",
) -> str:
    base = pages_base.rstrip("/")
    encoded_name = quote(safe_filename(filename), safe="._-")
    encoded_pr = quote(str(pr_number), safe="")
    url = f"{base}/pr/{encoded_pr}/{encoded_name}"
    token = re.sub(r"[^0-9a-fA-F]", "", cache_buster)[:12]
    if token:
        url = f"{url}?v={token}"
    return url


def raw_asset_base(pages_base: str) -> str:
    """Map Pages site URL to the gh-pages raw.githubusercontent.com root.

    raw.githubusercontent.com is available as soon as the gallery push finishes;
    github.io can lag by tens of seconds, which makes GitHub's camo proxy cache
    404s for sticky-comment embeds.
    """
    base = pages_base.rstrip("/")
    match = re.match(r"^https://([^.]+)\.github\.io/([^/]+)$", base)
    if match:
        owner, repo = match.group(1), match.group(2)
        return f"https://raw.githubusercontent.com/{owner}/{repo}/gh-pages"
    return base


def comment_thumbnail(
    pages_base: str,
    pr_number: str,
    filename: str,
    *,
    alt: str,
    cache_buster: str,
    width: int = 240,
) -> str:
    """HTML <img> for sticky comments (reliable in GFM tables; sized thumbnails)."""
    url = pages_url(
        raw_asset_base(pages_base),
        pr_number,
        filename,
        cache_buster=cache_buster,
    )
    return (
        f'<img src="{attr(url)}" alt="{attr(alt)}" width="{int(width)}" />'
    )


def attr(value: str) -> str:
    return html.escape(value, quote=True)


def text(value: str) -> str:
    return html.escape(value, quote=False)


def asset_key(image: dict) -> str:
    if image["stage"] == "apply-payload":
        return extract_display_label(image["stem"])
    return image["stem"]


def short_hash(sha256: str | None) -> str:
    if not sha256:
        return "—"
    return sha256[:12]


def dim_label(image: dict | None) -> str:
    if image is None or image.get("width") is None or image.get("height") is None:
        return "—"
    return f"{image['width']}×{image['height']}"


def build_comparisons(images: list[dict]) -> list[dict]:
    """Group artifacts by stage+asset and compare content across OS."""
    grouped: dict[tuple[str, str], dict[str, dict]] = defaultdict(dict)
    for image in images:
        grouped[(image["stage"], asset_key(image))][image["os"]] = image

    comparisons: list[dict] = []
    for (stage, key), by_os in sorted(
        grouped.items(), key=lambda item: (item[0][0], display_sort_key(item[0][1]))
    ):
        present = [os_name for os_name in EXPECTED_OS if os_name in by_os]
        extra = sorted(os_name for os_name in by_os if os_name not in EXPECTED_OS)
        all_os = present + extra
        hashes = {os_name: by_os[os_name]["sha256"] for os_name in all_os}
        dims = {
            os_name: (by_os[os_name].get("width"), by_os[os_name].get("height"))
            for os_name in all_os
        }
        unique_hashes = set(hashes.values())
        unique_dims = set(dims.values())
        missing = [os_name for os_name in EXPECTED_OS if os_name not in by_os]
        strict = stage in STRICT_STAGES

        if not all_os:
            status = "empty"
        elif len(all_os) == 1:
            status = "single-os"
        elif len(unique_dims) > 1:
            status = "size-mismatch"
        elif len(unique_hashes) > 1:
            status = "content-mismatch"
        elif missing:
            status = "match-incomplete"
        else:
            status = "match"

        # gui-smoke (and other non-strict stages) still report hashes, but variance
        # is expected (platform chrome). Treat mismatches as informational.
        gate = "strict" if strict else "informational"
        gate_ok = True
        if strict:
            gate_ok = status in {"match", "match-incomplete", "single-os"}
            # Incomplete/single are warnings, not hard failures — content mismatch is.
            if status in {"content-mismatch", "size-mismatch"}:
                gate_ok = False

        comparisons.append(
            {
                "stage": stage,
                "asset": key,
                "status": status,
                "gate": gate,
                "gate_ok": gate_ok,
                "missing_os": missing,
                "os": {
                    os_name: {
                        "filename": by_os[os_name]["filename"],
                        "sha256": by_os[os_name]["sha256"],
                        "bytes": by_os[os_name]["bytes"],
                        "width": by_os[os_name].get("width"),
                        "height": by_os[os_name].get("height"),
                    }
                    for os_name in all_os
                },
            }
        )
    return comparisons


def comparison_totals(comparisons: list[dict]) -> dict:
    strict = [c for c in comparisons if c["gate"] == "strict"]
    informational = [c for c in comparisons if c["gate"] == "informational"]
    strict_bad = [c for c in strict if not c["gate_ok"]]
    strict_match = [c for c in strict if c["status"] == "match"]
    strict_warn = [
        c
        for c in strict
        if c["gate_ok"] and c["status"] in {"match-incomplete", "single-os"}
    ]
    info_variance = [
        c
        for c in informational
        if c["status"] in {"content-mismatch", "size-mismatch"}
    ]
    return {
        "strict_total": len(strict),
        # Fully identical with complete OS coverage only.
        "strict_match": len(strict_match),
        # Gate-pass count (match + incomplete/single-os warnings).
        "strict_ok": len(strict) - len(strict_bad),
        "strict_failed": len(strict_bad),
        "strict_warnings": len(strict_warn),
        "informational_total": len(informational),
        "informational_variance": len(info_variance),
        "gate_ok": len(strict_bad) == 0,
    }


def status_chip_class(status: str, *, gate: str) -> str:
    if status == "match":
        return "match"
    if status in {"content-mismatch", "size-mismatch"}:
        return "info" if gate == "informational" else "mismatch"
    if status in {"match-incomplete", "single-os"}:
        return "warn"
    return "info"


def status_label(status: str, *, gate: str) -> str:
    labels = {
        "match": "identical across OS",
        "match-incomplete": "identical (matrix incomplete)",
        "single-os": "single OS only",
        "content-mismatch": (
            "platform variance" if gate == "informational" else "content mismatch"
        ),
        "size-mismatch": (
            "size variance" if gate == "informational" else "size mismatch"
        ),
        "empty": "empty",
    }
    return labels.get(status, status)


def write_site(
    out_dir: Path,
    images: list[dict],
    comparisons: list[dict],
    totals: dict,
    *,
    pr_number: str,
    sha: str,
    run_url: str,
    run_id: str,
    pages_base: str,
) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "styles.css").write_text(CSS, encoding="utf8")
    (out_dir / "gallery.js").write_text(JS, encoding="utf8")

    by_stage: dict[str, list[dict]] = defaultdict(list)
    for image in images:
        filename = safe_filename(image["filename"])
        dest = out_dir / filename
        src = Path(image["src_path"])
        if src.resolve() != dest.resolve():
            shutil.copy2(src, dest)
        by_stage[image["stage"]].append({**image, "filename": filename})

    comparisons_by_stage: dict[str, list[dict]] = defaultdict(list)
    for row in comparisons:
        comparisons_by_stage[row["stage"]].append(row)

    os_present = sorted({img["os"] for img in images})
    short_sha = sha[:7] if sha else "unknown"
    full_sha = sha if sha else "unknown"

    banner_chips: list[str] = []
    if totals["strict_total"]:
        if not totals["gate_ok"]:
            banner_chips.append(
                f'<span class="chip mismatch">apply-payload '
                f'{totals["strict_failed"]} OS regression(s)</span>'
            )
        if totals["strict_match"]:
            banner_chips.append(
                f'<span class="chip match">apply-payload {totals["strict_match"]}/'
                f'{totals["strict_total"]} fully OS-identical</span>'
            )
        if totals["strict_warnings"]:
            banner_chips.append(
                f'<span class="chip warn">{totals["strict_warnings"]} incomplete matrix</span>'
            )
    if totals["informational_total"]:
        banner_chips.append(
            f'<span class="chip info">gui/other: {totals["informational_variance"]} '
            f"platform variance / {totals['informational_total']} assets</span>"
        )
    if not banner_chips:
        banner_chips.append('<span class="chip info">No visual artifacts</span>')

    sections: list[str] = []
    for stage in sorted(by_stage):
        stage_comparisons = comparisons_by_stage.get(stage, [])
        gate = "strict" if stage in STRICT_STAGES else "informational"
        note = (
            "Byte-identical rasters expected across OS — mismatches are regression signals."
            if gate == "strict"
            else "Platform chrome differs by OS; variance here is informational, not a gate."
        )
        matrix = html_comparison_matrix(stage_comparisons, by_stage[stage], gate=gate)
        sections.append(
            f"""<section>
  <h2>{text(stage)}</h2>
  <p class="stage-note">{text(note)}</p>
  <div class="compare-wrap">
    {matrix}
  </div>
</section>"""
        )

    page = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{text(f"Easel visual harness · PR #{pr_number}")}</title>
  <link rel="stylesheet" href="styles.css" />
</head>
<body>
  <header>
    <h1>Easel visual harness</h1>
    <p class="lede">Cross-OS artifact review for pull request #{text(str(pr_number))}.
      Strict stages highlight content drift between runners; GUI smoke remains informational.</p>
    <div class="meta">
      <div class="meta-card"><span class="label">Pull request</span><span class="value">#{text(str(pr_number))}</span></div>
      <div class="meta-card"><span class="label">Commit</span><span class="value" title="{attr(full_sha)}">{text(short_sha)}</span></div>
      <div class="meta-card"><span class="label">Workflow run</span><span class="value"><a href="{attr(run_url)}">{text(run_id or "open")}</a></span></div>
      <div class="meta-card"><span class="label">Artifacts</span><span class="value">{text(str(len(images)))} PNG · {text(str(len(os_present)))} OS</span></div>
      <div class="meta-card"><span class="label">OS coverage</span><span class="value">{text(", ".join(os_present) if os_present else "—")}</span></div>
      <div class="meta-card"><span class="label">Gallery host</span><span class="value">{text(pages_base.rstrip("/"))}</span></div>
    </div>
    <div class="summary-banner">
      <strong>OS compare</strong>
      {"".join(banner_chips)}
    </div>
  </header>
  <main>
    {"".join(sections) if sections else "<p>No visual artifacts were found for this run.</p>"}
  </main>
  <dialog id="lightbox">
    <form method="dialog"><button value="close">Close</button></form>
    <img alt="" />
  </dialog>
  <script src="gallery.js"></script>
</body>
</html>
"""
    (out_dir / "index.html").write_text(page, encoding="utf8")


def html_comparison_matrix(
    comparisons: list[dict],
    stage_images: list[dict],
    *,
    gate: str,
) -> str:
    os_list = [os_name for os_name in EXPECTED_OS if any(i["os"] == os_name for i in stage_images)]
    extras = sorted({i["os"] for i in stage_images if i["os"] not in EXPECTED_OS})
    os_list.extend(extras)
    if not comparisons:
        return "<p>No comparable assets in this stage.</p>"

    header_cells = "".join(f"<th>{text(os_name)}</th>" for os_name in os_list)
    rows: list[str] = []
    for row in comparisons:
        status = row["status"]
        chip = (
            f'<span class="chip {status_chip_class(status, gate=gate)}">'
            f"{text(status_label(status, gate=gate))}</span>"
        )
        cells = [
            f'<td class="asset">{text(row["asset"])}<div class="cell-meta">{chip}</div></td>'
        ]
        for os_name in os_list:
            info = row["os"].get(os_name)
            if info is None:
                cells.append('<td><div class="missing">missing</div></td>')
                continue
            href = quote(info["filename"], safe="._-")
            caption = f"{os_name} · {row['asset']}"
            cells.append(
                f"""<td>
  <figure>
    <a href="{attr(href)}" data-lightbox data-caption="{attr(caption)}">
      <img src="{attr(href)}" alt="{attr(caption)}" loading="lazy" />
    </a>
  </figure>
  <div class="cell-meta">{text(dim_label(info))}<br />{text(short_hash(info["sha256"]))}<br />{text(str(info["bytes"]))} B</div>
</td>"""
            )
        rows.append("<tr>" + "".join(cells) + "</tr>")

    return (
        '<table class="compare">\n'
        f"<thead><tr><th>Asset</th>{header_cells}</tr></thead>\n"
        f"<tbody>\n{''.join(rows)}\n</tbody>\n"
        "</table>"
    )


def write_comment(
    out_path: Path,
    images: list[dict],
    comparisons: list[dict],
    totals: dict,
    *,
    pr_number: str,
    sha: str,
    run_url: str,
    run_id: str,
    pages_base: str,
    deployed: bool,
) -> None:
    gallery_link = f"{pages_base.rstrip('/')}/pr/{pr_number}/"
    short_sha = sha[:7] if sha else "unknown"
    os_present = sorted({img["os"] for img in images})
    lines = [
        MARKER,
        "## Visual harness",
        "",
    ]
    if deployed:
        lines.append(
            f"**[Open full gallery]({gallery_link})** · commit `{short_sha}` · "
            f"[workflow run]({run_url})"
        )
    else:
        lines.append(
            f"Full HTML gallery deploy skipped (`EASEL_CI_VISUAL_TOKEN` unset). "
            f"Commit `{short_sha}` · [workflow run]({run_url})"
        )
    lines.append("")

    # Metadata + comparison summary
    lines.extend(
        [
            "| | |",
            "| --- | --- |",
            f"| Artifacts | {len(images)} PNG · {len(os_present)} OS (`{'`, `'.join(os_present) or '—'}`) |",
            f"| Commit | `{sha or 'unknown'}` |",
            f"| Run | [`{run_id or 'open'}`]({run_url}) |",
        ]
    )
    if totals["strict_total"]:
        if not totals["gate_ok"]:
            lines.append(
                f"| Apply-payload OS compare | ❌ {totals['strict_failed']} "
                f"regression(s) (content/size mismatch) |"
            )
        else:
            lines.append(
                f"| Apply-payload OS compare | ✅ {totals['strict_match']}/"
                f"{totals['strict_total']} fully identical across runners |"
            )
        if totals["strict_warnings"]:
            lines.append(
                f"| Matrix coverage | ⚠️ {totals['strict_warnings']} asset(s) missing an OS |"
            )
    if totals["informational_total"]:
        lines.append(
            f"| GUI / other | informational · {totals['informational_variance']} "
            f"platform variance / {totals['informational_total']} assets |"
        )
    lines.append("")

    by_stage: dict[str, list[dict]] = defaultdict(list)
    for image in images:
        by_stage[image["stage"]].append(image)

    cache_buster = sha[:12] if sha else ""
    for stage in sorted(by_stage):
        lines.append(f"### {stage}")
        lines.append("")
        stage_images = by_stage[stage]
        if stage == "apply-payload":
            lines.extend(
                markdown_apply_payload_table(
                    stage_images,
                    pr_number,
                    pages_base,
                    deployed,
                    cache_buster=cache_buster,
                )
            )
        else:
            lines.extend(
                markdown_os_table(
                    stage_images,
                    pr_number,
                    pages_base,
                    deployed,
                    cache_buster=cache_buster,
                )
            )
        lines.append("")
        stage_rows = [c for c in comparisons if c["stage"] == stage]
        if stage_rows:
            lines.extend(markdown_comparison_table(stage_rows))
            lines.append("")

    if not by_stage:
        lines.append("_No visual artifacts were found for this run._")
        lines.append("")

    out_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf8")


def markdown_comparison_table(rows: list[dict]) -> list[str]:
    lines = [
        "<details>",
        "<summary>Cross-OS metadata</summary>",
        "",
        "| Asset | Status | " + " | ".join(f"`{os_name}`" for os_name in EXPECTED_OS) + " |",
        "| --- | --- | " + " | ".join("---" for _ in EXPECTED_OS) + " |",
    ]
    for row in rows:
        gate = row["gate"]
        cells = [
            f"`{row['asset']}`",
            status_label(row["status"], gate=gate),
        ]
        for os_name in EXPECTED_OS:
            info = row["os"].get(os_name)
            if info is None:
                cells.append("—")
            else:
                cells.append(
                    f"`{short_hash(info['sha256'])}` · {dim_label(info)} · {info['bytes']}B"
                )
        lines.append("| " + " | ".join(cells) + " |")
    lines.extend(["", "</details>"])
    return lines


def markdown_os_table(
    images: list[dict],
    pr_number: str,
    pages_base: str,
    deployed: bool,
    *,
    cache_buster: str = "",
) -> list[str]:
    lines = [
        "| OS | Preview | Size | SHA-256 (12) |",
        "| --- | --- | --- | --- |",
    ]
    for image in sorted(images, key=lambda i: i["os"]):
        preview = (
            comment_thumbnail(
                pages_base,
                pr_number,
                image["filename"],
                alt=f"{image['os']} {image['stem']}",
                cache_buster=cache_buster,
                width=320,
            )
            if deployed
            else "_deploy pending_"
        )
        lines.append(
            f"| `{image['os']}` | {preview} | {dim_label(image)} · {image['bytes']}B | "
            f"`{short_hash(image.get('sha256'))}` |"
        )
    return lines


def markdown_apply_payload_table(
    images: list[dict],
    pr_number: str,
    pages_base: str,
    deployed: bool,
    *,
    cache_buster: str = "",
) -> list[str]:
    # Columns by OS, rows by display index inferred from stem (...-display-N / apply-display-N).
    os_list = [os_name for os_name in EXPECTED_OS if any(i["os"] == os_name for i in images)]
    os_list.extend(sorted({i["os"] for i in images if i["os"] not in EXPECTED_OS}))
    by_display: dict[str, dict[str, dict]] = defaultdict(dict)
    for image in images:
        display = extract_display_label(image["stem"])
        by_display[display][image["os"]] = image

    header = "| Display | " + " | ".join(f"`{os_name}`" for os_name in os_list) + " |"
    sep = "| --- | " + " | ".join("---" for _ in os_list) + " |"
    lines = [header, sep]
    for display in sorted(by_display, key=display_sort_key):
        cells = [display]
        for os_name in os_list:
            image = by_display[display].get(os_name)
            if image is None:
                cells.append("—")
            elif deployed:
                cells.append(
                    comment_thumbnail(
                        pages_base,
                        pr_number,
                        image["filename"],
                        alt=f"{os_name} display {display}",
                        cache_buster=cache_buster,
                        width=200,
                    )
                )
            else:
                cells.append("_deploy pending_")
        lines.append("| " + " | ".join(cells) + " |")
    return lines


def extract_display_label(stem: str) -> str:
    m = re.search(r"display[-_](\d+)$", stem)
    if m:
        return m.group(1)
    return stem


def display_sort_key(label: str) -> tuple[int, str]:
    return (0, f"{int(label):04d}") if label.isdigit() else (1, label)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pr-number", required=True)
    parser.add_argument("--sha", default="")
    parser.add_argument("--run-url", default="")
    parser.add_argument("--run-id", default="")
    parser.add_argument(
        "--pages-base",
        default="https://fritz-fritz.github.io/easel-ci-visual",
    )
    parser.add_argument(
        "--deployed",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Whether Pages URLs should be embedded (false when deploy skipped)",
    )
    parser.add_argument(
        "--fail-on-os-mismatch",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Exit non-zero when strict-stage cross-OS content/size mismatches exist",
    )
    args = parser.parse_args()

    manifests = load_manifests(args.artifacts)
    images = collect_images(args.artifacts, manifests)
    comparisons = build_comparisons(images)
    totals = comparison_totals(comparisons)
    site_dir = args.out / "site"
    write_site(
        site_dir,
        images,
        comparisons,
        totals,
        pr_number=args.pr_number,
        sha=args.sha,
        run_url=args.run_url,
        run_id=args.run_id or "",
        pages_base=args.pages_base,
    )
    write_comment(
        args.out / "comment.md",
        images,
        comparisons,
        totals,
        pr_number=args.pr_number,
        sha=args.sha,
        run_url=args.run_url,
        run_id=args.run_id or "",
        pages_base=args.pages_base,
        deployed=args.deployed,
    )
    summary = {
        "pr_number": args.pr_number,
        "sha": args.sha,
        "run_id": args.run_id,
        "image_count": len(images),
        "stages": sorted({img["stage"] for img in images}),
        "os": sorted({img["os"] for img in images}),
        "deployed": args.deployed,
        "comparison": totals,
        "comparisons": comparisons,
    }
    (args.out / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf8"
    )
    print(json.dumps({"pr_number": args.pr_number, "image_count": len(images), **totals}))
    if args.fail_on_os_mismatch and not totals["gate_ok"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
