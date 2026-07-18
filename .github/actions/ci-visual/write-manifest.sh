#!/usr/bin/env bash
# Build ci-visual-manifest-*.json from staged PNGs (included in the stage×OS zip).
# Compatible with macOS Bash 3.2 (no mapfile).
set -euo pipefail

staged_dir="${STAGED_DIR:?}"
stage="${INPUT_STAGE:?}"
runner_os="${INPUT_RUNNER_OS:?}"
count="${FILE_COUNT:?}"
bundle_name="${BUNDLE_NAME:-ci-visual-${stage}-${runner_os}}"

manifest_name="ci-visual-manifest-${stage}-${runner_os}.json"
manifest_path="${staged_dir}/${manifest_name}"

list_file="${staged_dir}/files.list"
if [[ ! -f "$list_file" ]]; then
  # Fallback if an older stage step omitted files.list
  find "$staged_dir" -maxdepth 1 -type f -name '*.png' | LC_ALL=C sort >"$list_file"
fi

export STAGED_DIR INPUT_STAGE INPUT_RUNNER_OS FILE_COUNT BUNDLE_NAME="$bundle_name"
i=0
while IFS= read -r path || [[ -n "${path:-}" ]]; do
  [[ -n "$path" ]] || continue
  export "STAGED_FILE_${i}=${path}"
  i=$((i + 1))
done <"$list_file"

if [[ "$i" -ne "$count" ]]; then
  echo "::warning::ci-visual: staged PNG count ${i} != reported count ${count}"
fi

python3 - <<'PY' >"$manifest_path"
import json
import os
from pathlib import Path

count = int(os.environ["FILE_COUNT"])
server = os.environ.get("GITHUB_SERVER_URL", "https://github.com")
repo = os.environ.get("GITHUB_REPOSITORY", "")
run_id = os.environ.get("GITHUB_RUN_ID", "")
sha = os.environ.get("GITHUB_SHA", "")
attempt = os.environ.get("GITHUB_RUN_ATTEMPT", "1")
bundle_name = os.environ.get("BUNDLE_NAME", "")

images = []
stage = os.environ["INPUT_STAGE"]
runner_os = os.environ["INPUT_RUNNER_OS"]
prefix = "%s-%s-" % (stage, runner_os)
for index in range(count):
    file_path = os.environ.get("STAGED_FILE_%d" % index, "")
    if not file_path:
        continue
    path = Path(file_path)
    # Logical producer stem (e.g. gui-preview), not the staged
    # gui-smoke-<os>-gui-preview name — so gallery rows group across OS.
    name = path.stem
    logical = name[len(prefix):] if name.startswith(prefix) else name
    images.append(
        {
            "filename": path.name,
            "stem": logical,
        }
    )

manifest = {
    "stage": os.environ["INPUT_STAGE"],
    "os": os.environ["INPUT_RUNNER_OS"],
    "sha": sha,
    "run_id": run_id,
    "run_attempt": attempt,
    "repository": repo,
    "bundle": bundle_name,
    "images": images,
}
if repo and run_id:
    manifest["run_url"] = "%s/%s/actions/runs/%s" % (server, repo, run_id)
print(json.dumps(manifest, indent=2))
PY

echo "ci-visual: wrote manifest ${manifest_path}"
cat "$manifest_path"
echo "manifest-path=${manifest_path}" >> "$GITHUB_OUTPUT"
