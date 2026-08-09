#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 <version> [changelog]" >&2
    exit 2
fi

version=${1#v}
changelog=${2:-CHANGELOG.md}
heading="## [${version}]"

awk -v heading="$heading" '
    $0 == heading { found=1; next }
    index($0, heading " - ") == 1 {
        suffix=substr($0, length(heading) + 1)
        if (suffix ~ /^ - [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) {
            found=1
            next
        }
    }
    /^## \[/ { if (found) exit }
    found {
        print
        if ($0 ~ /[^[:space:]]/) content=1
    }
    END { if (!found || !content) exit 1 }
' "$changelog"
