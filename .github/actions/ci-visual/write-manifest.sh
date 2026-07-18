#!/usr/bin/env bash
# Build ci-visual-manifest-*.json from staged PNGs + upload-artifact outputs.
# Compatible with macOS Bash 3.2 (no mapfile).
set -euo pipefail

staged_dir="${STAGED_DIR:?}"
stage="${INPUT_STAGE:?}"
runner_os="${INPUT_RUNNER_OS:?}"
count="${FILE_COUNT:?}"

manifest_name="ci-visual-manifest-${stage}-${runner_os}.json"
manifest_path="${staged_dir}/${manifest_name}"

list_file="${staged_dir}/files.list"
if [[ ! -f "$list_file" ]]; then
  # Fallback if an older stage step omitted files.list
  find "$staged_dir" -maxdepth 1 -type f -name '*.png' | LC_ALL=C sort >"$list_file"
fi

export STAGED_DIR INPUT_STAGE INPUT_RUNNER_OS FILE_COUNT
i=0
while IFS= read -r path || [[ -n "${path:-}" ]]; do
  [[ -n "$path" ]] || continue
  export "STAGED_FILE_${i}=${path}"
  i=$((i + 1))
done <"$list_file"

if [[ "$i" -ne "$count" ]]; then
  echo "::warning::ci-visual: staged PNG count ${i} != reported count ${count}"
fi

for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
  id_var="UP${i}_ID"
  url_var="UP${i}_URL"
  # Bash 3.2-safe indirect expansion
  eval "export ${id_var}=\"\${${id_var}-}\""
  eval "export ${url_var}=\"\${${url_var}-}\""
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
    file_path = os.environ.get("STAGED_FILE_%d" % index, "")
    if not file_path:
        continue
    path = Path(file_path)
    artifact_id = os.environ.get("UP%d_ID" % index, "") or None
    artifact_url = os.environ.get("UP%d_URL" % index, "") or ""
    if artifact_id and not artifact_url and repo and run_id:
        artifact_url = (
            "%s/%s/actions/runs/%s/artifacts/%s"
            % (server, repo, run_id, artifact_id)
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
