#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BINARY=${THREAD_TO_TAB_TEST_BINARY:-"$ROOT/target/release/thread-to-tab"}
VERSION=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$ROOT/herdr-plugin.toml")
test -x "$BINARY" || {
    echo "release binary is missing: $BINARY" >&2
    exit 1
}

case "${THREAD_TO_TAB_TEST_TARGET:-$(uname -s):$(uname -m)}" in
    Darwin:arm64|Darwin:aarch64|aarch64-apple-darwin)
        TARGET=aarch64-apple-darwin
        TEST_OS=Darwin
        TEST_ARCH=arm64
        ;;
    Darwin:x86_64|Darwin:amd64|x86_64-apple-darwin)
        TARGET=x86_64-apple-darwin
        TEST_OS=Darwin
        TEST_ARCH=x86_64
        ;;
    Linux:aarch64|Linux:arm64|aarch64-unknown-linux-gnu)
        TARGET=aarch64-unknown-linux-gnu
        TEST_OS=Linux
        TEST_ARCH=aarch64
        ;;
    Linux:x86_64|Linux:amd64|x86_64-unknown-linux-gnu)
        TARGET=x86_64-unknown-linux-gnu
        TEST_OS=Linux
        TEST_ARCH=x86_64
        ;;
    *) echo "unsupported smoke-test target" >&2; exit 1 ;;
esac

fixture=$(mktemp -d "$ROOT/.local-release-smoke.XXXXXX")
package="$fixture/package"
checkout="$fixture/checkout"
mkdir "$package" "$checkout" "$checkout/scripts"
cp "$ROOT/scripts/install-binary.sh" "$checkout/scripts/install-binary.sh"
cp "$ROOT/herdr-plugin.toml" "$checkout/herdr-plugin.toml"
asset="thread-to-tab-v${VERSION}-${TARGET}.tar.gz"
cleanup() {
    rm -f "$package/thread-to-tab" "$package/LICENSE"
    rmdir "$package" 2>/dev/null || true
    rm -f "$checkout/bin/thread-to-tab" "$checkout/scripts/install-binary.sh" "$checkout/herdr-plugin.toml"
    rmdir "$checkout/bin" "$checkout/scripts" "$checkout" 2>/dev/null || true
    rm -f "$fixture/$asset" "$fixture/SHA256SUMS"
    rmdir "$fixture" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

cp "$BINARY" "$package/thread-to-tab"
cp "$ROOT/LICENSE" "$package/LICENSE"
tar -czf "$fixture/$asset" -C "$package" thread-to-tab LICENSE
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$fixture" && sha256sum "$asset" >SHA256SUMS)
else
    (cd "$fixture" && shasum -a 256 "$asset" >SHA256SUMS)
fi

THREAD_TO_TAB_BASE_URL="file://$fixture" \
THREAD_TO_TAB_OS="$TEST_OS" \
THREAD_TO_TAB_ARCH="$TEST_ARCH" \
"$checkout/scripts/install-binary.sh" >/dev/null
test -x "$checkout/bin/thread-to-tab"
if "$checkout/bin/thread-to-tab" >/dev/null 2>&1; then
    echo "installed binary unexpectedly accepted missing arguments" >&2
    exit 1
fi
printf 'local release smoke test passed\n'
