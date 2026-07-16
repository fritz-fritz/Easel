#!/usr/bin/env bash
# Stage harness visual files with CI naming. Emits outputs for upload + summary.
set -euo pipefail

stage="${INPUT_STAGE:?}"
source_dir="${INPUT_SOURCE_DIR:?}"
pattern="${INPUT_PATTERN:?}"
name_template="${INPUT_NAME_TEMPLATE:?}"
runner_os="${INPUT_RUNNER_OS:?}"
if_no_files="${INPUT_IF_NO_FILES:-ignore}"
summary_title="${INPUT_SUMMARY_TITLE:-}"

if [[ -z "$summary_title" ]]; then
  summary_title="${stage} (${runner_os})"
fi

staged_dir="${RUNNER_TEMP:-/tmp}/ci-visual-${stage}-${runner_os}-$$"
mkdir -p "$staged_dir"

shopt -s nullglob
# shellcheck disable=SC2086
matches=("${source_dir}"/${pattern})
shopt -u nullglob

count=0
if ((${#matches[@]} > 0)); then
  for src in "${matches[@]}"; do
    [[ -f "$src" ]] || continue
    base="$(basename "$src")"
    stem="${base%.*}"
    dest_name="$name_template"
    dest_name="${dest_name//\{stage\}/${stage}}"
    dest_name="${dest_name//\{os\}/${runner_os}}"
    dest_name="${dest_name//\{stem\}/${stem}}"
    # Avoid collisions when the template omits {stem} but multiple files match.
    if [[ -e "${staged_dir}/${dest_name}" ]]; then
      ext="${dest_name##*.}"
      if [[ "$ext" != "$dest_name" ]]; then
        dest_name="${dest_name%.*}-${stem}.${ext}"
      else
        dest_name="${dest_name}-${stem}"
      fi
    fi
    cp "$src" "${staged_dir}/${dest_name}"
    count=$((count + 1))
  done
fi

if ((count == 0)); then
  case "$if_no_files" in
    error)
      echo "::error::ci-visual: no files matched '${pattern}' in '${source_dir}'"
      exit 1
      ;;
    warn)
      echo "::warning::ci-visual: no files matched '${pattern}' in '${source_dir}'"
      ;;
    ignore | *)
      echo "ci-visual: no files matched '${pattern}' in '${source_dir}' (ignored)"
      ;;
  esac
else
  echo "ci-visual: staged ${count} file(s) into ${staged_dir}"
  ls -la "$staged_dir"
fi

{
  echo "staged-dir=${staged_dir}"
  echo "count=${count}"
  echo "summary-title=${summary_title}"
} >> "$GITHUB_OUTPUT"
