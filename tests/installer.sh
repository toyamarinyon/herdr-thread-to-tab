#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
INSTALLER="$ROOT/scripts/install-binary.sh"
VERSION=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$ROOT/herdr-plugin.toml")
LINUX_ASSET="thread-to-tab-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz"

asset=$(THREAD_TO_TAB_OS=Darwin THREAD_TO_TAB_ARCH=arm64 "$INSTALLER" --print-asset)
test "$asset" = "thread-to-tab-v${VERSION}-aarch64-apple-darwin.tar.gz"
asset=$(THREAD_TO_TAB_OS=Linux THREAD_TO_TAB_ARCH=x86_64 "$INSTALLER" --print-asset)
test "$asset" = "$LINUX_ASSET"

if THREAD_TO_TAB_OS=Plan9 THREAD_TO_TAB_ARCH=mips "$INSTALLER" --print-asset >/dev/null 2>&1; then
    echo "unsupported target unexpectedly succeeded" >&2
    exit 1
fi

fixture=$(mktemp -d "$ROOT/.installer-test.XXXXXX")
checkout="$fixture/checkout"
mkdir -p "$checkout/scripts"
cp "$INSTALLER" "$checkout/scripts/install-binary.sh"
cp "$ROOT/herdr-plugin.toml" "$checkout/herdr-plugin.toml"
cleanup() {
    rm -f "$fixture/thread-to-tab" "$fixture/LICENSE" "$fixture/$LINUX_ASSET" "$fixture/SHA256SUMS" "$fixture/bin/thread-to-tab"
    rmdir "$fixture/bin" 2>/dev/null || true
    rm -f "$checkout/bin/thread-to-tab" "$checkout/scripts/install-binary.sh" "$checkout/herdr-plugin.toml"
    rmdir "$checkout/bin" "$checkout/scripts" "$checkout" 2>/dev/null || true
    rmdir "$fixture" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM
printf '#!/bin/sh\nexit 0\n' >"$fixture/thread-to-tab"
cp "$ROOT/LICENSE" "$fixture/LICENSE"
tar -czf "$fixture/$LINUX_ASSET" -C "$fixture" thread-to-tab LICENSE
printf '%064d  %s\n' 0 "$LINUX_ASSET" >"$fixture/SHA256SUMS"
if THREAD_TO_TAB_OS=Linux THREAD_TO_TAB_ARCH=x86_64 THREAD_TO_TAB_BASE_URL="file://$fixture" "$checkout/scripts/install-binary.sh" >/dev/null 2>&1; then
    echo "invalid checksum unexpectedly succeeded" >&2
    exit 1
fi
test ! -e "$checkout/bin/thread-to-tab"

if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$fixture/$LINUX_ASSET" | awk '{print $1}')
else
    hash=$(shasum -a 256 "$fixture/$LINUX_ASSET" | awk '{print $1}')
fi
printf '%s  %s\n' "$hash" "$LINUX_ASSET" >"$fixture/SHA256SUMS"
THREAD_TO_TAB_OS=Linux THREAD_TO_TAB_ARCH=x86_64 THREAD_TO_TAB_BASE_URL="file://$fixture" "$checkout/scripts/install-binary.sh" >/dev/null
test -x "$checkout/bin/thread-to-tab"
printf 'installer tests passed\n'
