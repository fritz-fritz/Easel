#!/usr/bin/env bash
# Build ci-visual-manifest-*.json from staged PNGs + upload-artifact outputs.
set -euo pipefail

staged_dir="${STAGED_DIR:?}"
stage="${INPUT_STAGE:?}"
runner_os="${INPUT_RUNNER_OS:?}"
count="${FILE_COUNT:?}"

manifest_name="ci-visual-manifest-${stage}-${runner_os}.json"
manifest_path="${staged_dir}/${manifest_name}"

mapfile -t pngs < <(if [[ -f "${staged_dir}/files.list" ]]; then cat "${staged_dir}/files.list"; else find "$staged_dir" -maxdepth 1 -type f -name '*.png' | LC_ALL=C sort; fi)
if ((${#pngs[@]} != count)); then
  echo "::warning::ci-visual: staged PNG count ${#pngs[@]} != reported count ${count}"
fi

export STAGED_DIR INPUT_STAGE INPUT_RUNNER_OS FILE_COUNT
for i in "${!pngs[@]}"; do
  export "STAGED_FILE_${i}=${pngs[$i]}"
done
for i in $(seq 0 11); do
  id_var="UP${i}_ID"
  url_var="UP${i}_URL"
  export "${id_var}=${!id_var-}"
  export "${url_var}=${!url_var-}"
done

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

images = []
for index in range(count):
    file_path = os.environ.get(f"STAGED_FILE_{index}", "")
    if not file_path:
        continue
    path = Path(file_path)
    artifact_id = os.environ.get(f"UP{index}_ID", "") or None
    artifact_url = os.environ.get(f"UP{index}_URL", "") or ""
    if artifact_id and not artifact_url and repo and run_id:
        artifact_url = (
            f"{server}/{repo}/actions/runs/{run_id}/artifacts/{artifact_id}"
        )
    images.append(
        {
            "filename": path.name,
            "stem": path.stem,
            "artifact_id": (
                int(artifact_id)
                if artifact_id and str(artifact_id).isdigit()
                else artifact_id
            ),
            "artifact_url": artifact_url,
        }
    )

manifest = {
    "stage": os.environ["INPUT_STAGE"],
    "os": os.environ["INPUT_RUNNER_OS"],
    "sha": sha,
    "run_id": run_id,
    "run_attempt": attempt,
    "repository": repo,
    "images": images,
}
print(json.dumps(manifest, indent=2))
PY

echo "ci-visual: wrote manifest ${manifest_path}"
cat "$manifest_path"
echo "manifest-path=${manifest_path}" >> "$GITHUB_OUTPUT"
