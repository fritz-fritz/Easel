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

1. **Hosted HTML gallery** — styled Pages site at
   `https://fritz-fritz.github.io/easel-ci-visual/pr/<n>/`
2. **Sticky PR comment** — Markdown tables with `![]()` thumbnails (same PNG URLs) plus a
   link to the HTML gallery

Generator code lives in this repo under [`.github/ci-visual/`](../.github/ci-visual/).

## Cursor Demo

Cursor Cloud Agent **Demo** artifacts are a separate, agent-scoped surface. GitHub Actions
cannot populate Demo today. Keep CI galleries for every PR; use Demo when Cloud Agents
record screenshots/videos under `artifacts/`. See [QUALITY.md](QUALITY.md).
