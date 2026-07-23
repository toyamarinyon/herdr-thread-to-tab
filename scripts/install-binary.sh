#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MANIFEST="$ROOT/herdr-plugin.toml"
REPOSITORY=${THREAD_TO_TAB_REPOSITORY:-toyamarinyon/herdr-thread-to-tab}
VERSION=${THREAD_TO_TAB_VERSION:-$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$MANIFEST")}
OS=${THREAD_TO_TAB_OS:-$(uname -s)}
ARCH=${THREAD_TO_TAB_ARCH:-$(uname -m)}

case "$OS:$ARCH" in
    Darwin:arm64|Darwin:aarch64) TARGET=aarch64-apple-darwin ;;
    Darwin:x86_64|Darwin:amd64) TARGET=x86_64-apple-darwin ;;
    Linux:aarch64|Linux:arm64) TARGET=aarch64-unknown-linux-gnu ;;
    Linux:x86_64|Linux:amd64) TARGET=x86_64-unknown-linux-gnu ;;
    *) echo "thread-to-tab: unsupported target: $OS $ARCH" >&2; exit 1 ;;
esac

ASSET="thread-to-tab-v${VERSION}-${TARGET}.tar.gz"
BASE_URL=${THREAD_TO_TAB_BASE_URL:-"https://github.com/${REPOSITORY}/releases/download/v${VERSION}"}

if [ "${1:-}" = "--print-asset" ]; then
    printf '%s\n' "$ASSET"
    exit 0
fi

TMP=$(mktemp -d "$ROOT/.thread-to-tab-install.XXXXXX")
cleanup() {
    rm -f "$TMP/$ASSET" "$TMP/SHA256SUMS" "$TMP/thread-to-tab" "$TMP/LICENSE"
    rmdir "$TMP" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

download() {
    url=$1
    destination=$2
    if command -v curl >/dev/null 2>&1; then
        curl -fL --silent --show-error --retry 2 --connect-timeout 10 -o "$destination" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -O "$destination" "$url"
    else
        echo "thread-to-tab: curl or wget is required to download release assets" >&2
        return 1
    fi
}

download "$BASE_URL/$ASSET" "$TMP/$ASSET"
download "$BASE_URL/SHA256SUMS" "$TMP/SHA256SUMS"

EXPECTED=$(awk -v name="$ASSET" '$2 == name || $2 == ("*" name) { print $1; exit }' "$TMP/SHA256SUMS")
if [ -z "$EXPECTED" ]; then
    echo "thread-to-tab: checksum is missing for $ASSET" >&2
    exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "$TMP/$ASSET" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL=$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')
else
    echo "thread-to-tab: sha256sum or shasum is required" >&2
    exit 1
fi
if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "thread-to-tab: checksum verification failed for $ASSET" >&2
    exit 1
fi

tar -xzf "$TMP/$ASSET" -C "$TMP"
if [ ! -f "$TMP/thread-to-tab" ]; then
    echo "thread-to-tab: release archive does not contain thread-to-tab" >&2
    exit 1
fi
mkdir -p "$ROOT/bin"
mv "$TMP/thread-to-tab" "$ROOT/bin/thread-to-tab"
chmod 755 "$ROOT/bin/thread-to-tab"
printf 'thread-to-tab: installed %s\n' "$ASSET"
