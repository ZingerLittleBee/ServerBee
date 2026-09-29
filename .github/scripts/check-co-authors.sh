#!/usr/bin/env bash
# Fail if any commit in base..head has a Co-authored-by trailer.
# Usage: check-co-authors.sh <base_sha> <head_sha>
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <base_sha> <head_sha>" >&2
  exit 1
fi

base_sha=$1
head_sha=$2

if [[ -z "${base_sha}" || -z "${head_sha}" ]]; then
  echo "error: base and head SHAs must be non-empty" >&2
  exit 1
fi

_script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=attribution-lib.sh
. "${_script_dir}/attribution-lib.sh"

resolve_commit() {
  local sha=$1
  if git cat-file -e "${sha}^{commit}" 2>/dev/null; then
    return 0
  fi
  echo "Fetching missing commit ${sha}..."
  if git fetch --no-tags --depth=1 origin "${sha}"; then
    return 0
  fi
  echo "error: cannot resolve commit ${sha}" >&2
  echo "error: fetch the PR head (refs/pull/<n>/head) and retry" >&2
  exit 1
}

resolve_commit "${base_sha}"
resolve_commit "${head_sha}"

failed=0
commits=$(git rev-list "${base_sha}..${head_sha}")
while IFS= read -r commit || [[ -n "${commit}" ]]; do
  [[ -z "${commit}" ]] && continue
  body=$(git log -1 --format='%B' "${commit}" | tr -d '\r')
  short=$(git rev-parse --short "${commit}")
  co_matches=$(printf '%s\n' "${body}" | attribution_co_author_lines)
  gen_matches=$(printf '%s\n' "${body}" | attribution_generated_lines)
  email_matches=$(printf '%s\n' "${body}" | attribution_email_lines)
  matches=$(printf '%s\n%s\n%s\n' "${co_matches}" "${gen_matches}" "${email_matches}")
  matches=$(printf '%s\n' "${matches}" | sed '/^$/d' | sort -u)
  if [[ -n "${matches}" ]]; then
    failed=1
    while IFS= read -r line || [[ -n "${line}" ]]; do
      [[ -z "${line}" ]] && continue
      echo "::error::Commit ${short} has a Co-authored-by trailer, generated-by text, or an email address"
      echo "  ${short}  ${line}"
      echo "  Fix: git rebase -i ${base_sha}  # drop the matching line, then force-push"
    done <<< "${matches}"
  fi
done <<< "${commits}"

if [[ "${failed}" -ne 0 ]]; then
  exit 1
fi

echo "OK: no Co-authored-by, generated-by, or email attribution in ${base_sha}..${head_sha}"
