#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: verify-glibc-baseline.sh BINARY MAX_GLIBC_VERSION" >&2
    exit 2
fi

BINARY=$1
MAX_VERSION=$2

test -f "$BINARY" || {
    echo "thread-to-tab: binary is missing: $BINARY" >&2
    exit 1
}

REQUIRED=$(
    strings "$BINARY" |
        sed -n 's/.*GLIBC_\([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' |
        sort -Vu |
        tail -n 1
)

test -n "$REQUIRED" || {
    echo "thread-to-tab: no GLIBC symbol versions found in $BINARY" >&2
    exit 1
}

HIGHEST=$(printf '%s\n%s\n' "$MAX_VERSION" "$REQUIRED" | sort -Vu | tail -n 1)
if [ "$HIGHEST" != "$MAX_VERSION" ]; then
    echo "thread-to-tab: $BINARY requires GLIBC_$REQUIRED, newer than GLIBC_$MAX_VERSION" >&2
    exit 1
fi

printf 'thread-to-tab: GLIBC_%s requirement is within GLIBC_%s baseline\n' "$REQUIRED" "$MAX_VERSION"
