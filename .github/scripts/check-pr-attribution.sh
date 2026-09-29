#!/usr/bin/env bash
# Fail if the PR title or body contains generated-by attribution or any email.
# Reads PR_TITLE and PR_BODY from the environment. Null/unset body is empty.
set -euo pipefail

_script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=attribution-lib.sh
. "${_script_dir}/attribution-lib.sh"

title=${PR_TITLE-}
body=${PR_BODY-}
text=$(printf '%s\n%s\n' "${title}" "${body}" | tr -d '\r')

gen_matches=$(printf '%s\n' "${text}" | attribution_generated_lines)
email_matches=$(printf '%s\n' "${text}" | attribution_email_lines)
matches=$(printf '%s\n%s\n' "${gen_matches}" "${email_matches}" | sed '/^$/d' | sort -u)

if [[ -n "${matches}" ]]; then
  echo "::error::PR title or description contains generated-by attribution or an email address"
  while IFS= read -r line || [[ -n "${line}" ]]; do
    [[ -z "${line}" ]] && continue
    echo "  ${line}"
  done <<< "${matches}"
  echo "Remove those lines from the PR title and description, then save."
  exit 1
fi

echo "OK: PR title and description have no generated-by attribution or email address"
