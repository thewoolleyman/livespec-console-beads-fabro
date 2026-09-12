#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo false
  echo "classify-ci-changes: expected BASE_SHA HEAD_SHA" >&2
  exit 64
fi

base_sha="$1"
head_sha="$2"

if ! git cat-file -e "${base_sha}^{commit}" 2>/dev/null; then
  echo false
  echo "classify-ci-changes: base commit is unavailable: ${base_sha}" >&2
  exit 65
fi
if ! git cat-file -e "${head_sha}^{commit}" 2>/dev/null; then
  echo false
  echo "classify-ci-changes: head commit is unavailable: ${head_sha}" >&2
  exit 65
fi

diff_file="$(mktemp)"
trap 'rm -f "$diff_file"' EXIT
if ! git diff --raw -z --no-renames --no-ext-diff "$base_sha" "$head_sha" -- >"$diff_file"; then
  echo false
  echo "classify-ci-changes: git diff failed" >&2
  exit 65
fi

mapfile -d '' -t fields <"$diff_file"
if [ "${#fields[@]}" -eq 0 ]; then
  echo false
  exit 0
fi
if [ "$(( ${#fields[@]} % 2 ))" -ne 0 ]; then
  echo false
  echo "classify-ci-changes: malformed raw diff" >&2
  exit 65
fi

allowed_path() {
  case "$1" in
    CHANGELOG.md | docs/doc-custody.md | docs/factory-confirmations.md)
      return 0
      ;;
    .ai/*.md | plan/*.md)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

for ((index = 0; index < ${#fields[@]}; index += 2)); do
  header="${fields[index]}"
  changed_path="${fields[index + 1]}"
  read -r old_mode new_mode _old_object _new_object status <<<"${header#:}"

  case "$status:$old_mode:$new_mode" in
    A:000000:100644 | D:100644:000000 | M:100644:100644)
      ;;
    *)
      echo false
      exit 0
      ;;
  esac

  if ! allowed_path "$changed_path"; then
    echo false
    exit 0
  fi
done

echo true
