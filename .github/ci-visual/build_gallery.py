#!/usr/bin/env python3
# Copyright (c) contributors. MPL-2.0.
"""Build a dual-surface visual gallery from ci-visual artifacts.

Reads downloaded Actions artifacts (manifests + PNGs) and writes:
  - site/index.html (+ assets + PNGs) for GitHub Pages
  - comment.md for a sticky PR comment (Markdown tables + gallery link)
"""

from __future__ import annotations

import argparse
import html
import json
import re
import shutil
from collections import defaultdict
from pathlib import Path
from urllib.parse import quote


MARKER = "<!-- easel-ci-visual -->"

CSS = """\
:root {
  --bg: #0f1419;
  --panel: #1a222c;
  --text: #e7ecf1;
  --muted: #9aa7b5;
  --accent: #6cb3ff;
  --border: #2b3643;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "Segoe UI", "Helvetica Neue", sans-serif;
  background: radial-gradient(1200px 600px at 10% -10%, #1d2a38, transparent),
    radial-gradient(900px 500px at 90% 0%, #243122, transparent), var(--bg);
  color: var(--text);
  line-height: 1.45;
}
header, main { max-width: 1100px; margin: 0 auto; padding: 1.5rem; }
header h1 { margin: 0 0 0.35rem; font-size: 1.6rem; font-weight: 650; }
header p { margin: 0.2rem 0; color: var(--muted); }
header a { color: var(--accent); }
section {
  margin: 1.5rem 0;
  padding: 1rem 1.1rem 1.2rem;
  background: color-mix(in srgb, var(--panel) 92%, black);
  border: 1px solid var(--border);
  border-radius: 12px;
}
section h2 {
  margin: 0 0 0.85rem;
  font-size: 1.15rem;
  font-weight: 600;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 0.9rem;
}
figure {
  margin: 0;
  background: #0b1015;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
figure a { display: block; }
figure img {
  display: block;
  width: 100%;
  height: auto;
  background: #080b0f;
}
figcaption {
  padding: 0.55rem 0.7rem 0.7rem;
  font-size: 0.82rem;
  color: var(--muted);
}
figcaption strong { color: var(--text); font-weight: 600; }
dialog {
  border: 1px solid var(--border);
  border-radius: 12px;
  background: #0b1015;
  color: var(--text);
  max-width: min(96vw, 1200px);
  padding: 0;
}
dialog::backdrop { background: rgba(0, 0, 0, 0.72); }
dialog img { max-width: 96vw; max-height: 88vh; display: block; }
dialog form { padding: 0.5rem; text-align: right; }
button {
  appearance: none;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--text);
  border-radius: 8px;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
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


def collect_images(
    artifact_root: Path, manifests: list[dict]
) -> list[dict]:
    """Merge manifest entries with on-disk PNG paths."""
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
            images.append(
                {
                    "stage": str(manifest["stage"]),
                    "os": str(manifest["os"]),
                    "filename": filename,
                    "stem": str(image.get("stem", Path(filename).stem)),
                    "artifact_url": str(image.get("artifact_url", "")),
                    "src_path": src,
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
        images.append(
            {
                "stage": stage,
                "os": os_name,
                "filename": filename,
                "stem": stem,
                "artifact_url": "",
                "src_path": src,
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
    """Return a basename-only filename, rejecting path traversal."""
    name = Path(filename).name
    if not name or name in {".", ".."} or name != filename:
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


def write_site(
    out_dir: Path,
    images: list[dict],
    *,
    pr_number: str,
    sha: str,
    run_url: str,
    pages_base: str,
) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "styles.css").write_text(CSS, encoding="utf8")
    (out_dir / "gallery.js").write_text(JS, encoding="utf8")

    by_stage: dict[str, list[dict]] = defaultdict(list)
    for image in images:
        filename = safe_filename(image["filename"])
        dest = out_dir / filename
        shutil.copy2(image["src_path"], dest)
        by_stage[image["stage"]].append({**image, "filename": filename})

    sections: list[str] = []
    for stage in sorted(by_stage):
        cards: list[str] = []
        for image in sorted(by_stage[stage], key=lambda i: (i["os"], i["stem"])):
            href = quote(image["filename"], safe="._-")
            caption = f"{image['os']} · {image['stem']}"
            cards.append(
                f"""<figure>
  <a href="{attr(href)}" data-lightbox data-caption="{attr(caption)}">
    <img src="{attr(href)}" alt="{attr(caption)}" loading="lazy" />
  </a>
  <figcaption><strong>{text(image['os'])}</strong><br />{text(image['stem'])}</figcaption>
</figure>"""
            )
        sections.append(
            f"<section>\n  <h2>{text(stage)}</h2>\n  <div class=\"grid\">\n    "
            + "\n    ".join(cards)
            + "\n  </div>\n</section>"
        )

    short_sha = sha[:7] if sha else "unknown"
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
    <p>Pull request <strong>#{text(str(pr_number))}</strong> · commit <code>{text(short_sha)}</code></p>
    <p><a href="{attr(run_url)}">Workflow run</a> · gallery host <code>{text(pages_base.rstrip('/'))}</code></p>
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


def write_comment(
    out_path: Path,
    images: list[dict],
    *,
    pr_number: str,
    sha: str,
    run_url: str,
    pages_base: str,
    deployed: bool,
) -> None:
    gallery_link = f"{pages_base.rstrip('/')}/pr/{pr_number}/"
    short_sha = sha[:7] if sha else "unknown"
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

    if not by_stage:
        lines.append("_No visual artifacts were found for this run._")
        lines.append("")

    out_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf8")


def markdown_os_table(
    images: list[dict],
    pr_number: str,
    pages_base: str,
    deployed: bool,
    *,
    cache_buster: str = "",
) -> list[str]:
    lines = [
        "| OS | Preview | Artifact |",
        "| --- | --- | --- |",
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
        artifact = (
            f"[open]({image['artifact_url']})" if image.get("artifact_url") else "—"
        )
        lines.append(f"| `{image['os']}` | {preview} | {artifact} |")
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
    os_list = sorted({img["os"] for img in images})
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
    args = parser.parse_args()

    manifests = load_manifests(args.artifacts)
    images = collect_images(args.artifacts, manifests)
    site_dir = args.out / "site"
    write_site(
        site_dir,
        images,
        pr_number=args.pr_number,
        sha=args.sha,
        run_url=args.run_url,
        pages_base=args.pages_base,
    )
    write_comment(
        args.out / "comment.md",
        images,
        pr_number=args.pr_number,
        sha=args.sha,
        run_url=args.run_url,
        pages_base=args.pages_base,
        deployed=args.deployed,
    )
    summary = {
        "pr_number": args.pr_number,
        "image_count": len(images),
        "stages": sorted({img["stage"] for img in images}),
        "deployed": args.deployed,
    }
    (args.out / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf8"
    )
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
