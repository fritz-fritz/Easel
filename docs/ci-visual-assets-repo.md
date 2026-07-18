# CI visual assets repository

Easel publishes PR visual galleries to a **separate** GitHub repository so PNG/HTML
history does not bloat clones of this source tree.

## Repository

| Item | Value |
| --- | --- |
| Suggested name | [`fritz-fritz/easel-ci-visual`](https://github.com/fritz-fritz/easel-ci-visual) |
| Visibility | Public (required for GitHub Pages image embeds via camo) |
| Pages source | `gh-pages` branch (root) |
| Layout | `pr/<number>/index.html` plus PNGs, `styles.css`, `gallery.js` |

Create once (maintainer):

```sh
gh repo create fritz-fritz/easel-ci-visual --public \
  --description "Easel CI visual harness galleries (HTML + PNGs). Not product source." \
  --disable-issues --disable-wiki
```

Then enable **Settings → Pages → Deploy from branch → `gh-pages` / root**.
The first successful gallery workflow creates `gh-pages`.

## Secret on the Easel repository

Add a fine-grained (or classic) personal access token as repository secret:

| Secret | Permissions |
| --- | --- |
| `EASEL_CI_VISUAL_TOKEN` | `contents: write` on `fritz-fritz/easel-ci-visual` |

The [`CI Visual Gallery`](../.github/workflows/ci-visual-gallery.yml) workflow uses that
token only to push gallery output and to prune `pr/<n>/` when a PR closes.

Without the secret, CI still uploads visual artifacts and posts a sticky PR comment, but
inline Markdown image embeds and the hosted HTML gallery are skipped.

## Dual review surfaces

1. **Hosted HTML gallery** — comparison-first Pages site at
   `https://fritz-fritz.github.io/easel-ci-visual/pr/<n>/` (metadata strip, OS×asset matrix,
   SHA-256 / dimensions / byte size per cell)
2. **Sticky PR comment** — Markdown tables with HTML `<img>` thumbnails (via
   `raw.githubusercontent.com/.../gh-pages/...`, cache-busted with the commit SHA so
   GitHub’s camo proxy does not race or reuse a Pages CDN 404), a cross-OS compare summary,
   expandable per-asset metadata, and a link to the hosted HTML gallery on `github.io`
3. **Commit status** — `CI Visual Gallery / OS compare` on the PR head SHA for branch
   protection (see [QUALITY.md](QUALITY.md))

Generator code lives in this repo under [`.github/ci-visual/`](../.github/ci-visual/).
The privileged `workflow_run` publisher always checks out the **default branch** for that
tooling (never the PR head) so untrusted PR code cannot run with deploy secrets.
Keep `build_gallery.py` backward-compatible with older `ci-visual` manifests that still
list `filename` / `stem` (per-file Actions artifact URLs are no longer emitted).
Run `python3 .github/ci-visual/test_build_gallery.py` to exercise the gallery builder locally.

CI visual producers upload one zip per stage×OS via `actions/upload-artifact@v7`
(`ci-visual-<stage>-<os>`, manifest included). The gallery workflow downloads with
`pattern: ci-visual-*` and `merge-multiple: true`. When the triggering CI run has no
visual bundles, the gallery job exits early as success (no empty Pages site or sticky
comment).

## Cursor Demo

Cursor Cloud Agent **Demo** artifacts are a separate, agent-scoped surface. GitHub Actions
cannot populate Demo today. Keep CI galleries for every PR; use Demo when Cloud Agents
record screenshots/videos under `artifacts/`. See [QUALITY.md](QUALITY.md).
