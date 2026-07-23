#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: verify-release-assets.sh VERSION DIST_DIR" >&2
    exit 2
fi

VERSION=${1#v}
DIST=$2
TARGETS='aarch64-apple-darwin
x86_64-apple-darwin
aarch64-unknown-linux-gnu
x86_64-unknown-linux-gnu'

test -f "$DIST/SHA256SUMS" || {
    echo "thread-to-tab: SHA256SUMS is missing" >&2
    exit 1
}

expected_count=0
for target in $TARGETS; do
    asset="thread-to-tab-v${VERSION}-${target}.tar.gz"
    test -f "$DIST/$asset" || {
        echo "thread-to-tab: release asset is missing: $asset" >&2
        exit 1
    }
    entries=$(tar -tzf "$DIST/$asset" | LC_ALL=C sort)
    test "$entries" = "LICENSE
thread-to-tab" || {
        echo "thread-to-tab: unexpected archive contents: $asset" >&2
        exit 1
    }
    expected_count=$((expected_count + 1))
done

actual_count=$(find "$DIST" -maxdepth 1 -type f -name 'thread-to-tab-v*.tar.gz' | wc -l | tr -d ' ')
test "$actual_count" -eq "$expected_count" || {
    echo "thread-to-tab: expected $expected_count archives, found $actual_count" >&2
    exit 1
}

checksum_count=$(awk 'NF == 2 { count += 1 } END { print count + 0 }' "$DIST/SHA256SUMS")
test "$checksum_count" -eq "$expected_count" || {
    echo "thread-to-tab: SHA256SUMS must list exactly $expected_count assets" >&2
    exit 1
}

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$DIST" && sha256sum -c SHA256SUMS)
elif command -v shasum >/dev/null 2>&1; then
    while read -r expected name; do
        actual=$(shasum -a 256 "$DIST/$name" | awk '{print $1}')
        test "$actual" = "$expected" || {
            echo "thread-to-tab: checksum verification failed for $name" >&2
            exit 1
        }
    done <"$DIST/SHA256SUMS"
else
    echo "thread-to-tab: sha256sum or shasum is required" >&2
    exit 1
fi
