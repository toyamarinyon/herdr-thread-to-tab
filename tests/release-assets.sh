#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fixture=$(mktemp -d "$ROOT/.release-assets-test.XXXXXX")
package="$fixture/package"
mkdir "$package"
cleanup() {
    rm -f "$package/thread-to-tab" "$package/LICENSE"
    rmdir "$package" 2>/dev/null || true
    rm -f "$fixture"/thread-to-tab-v0.1.0-*.tar.gz "$fixture/SHA256SUMS"
    rmdir "$fixture" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

printf '#!/bin/sh\nexit 0\n' >"$package/thread-to-tab"
chmod 755 "$package/thread-to-tab"
cp "$ROOT/LICENSE" "$package/LICENSE"
for target in \
    aarch64-apple-darwin \
    x86_64-apple-darwin \
    aarch64-unknown-linux-gnu \
    x86_64-unknown-linux-gnu
do
    tar -czf "$fixture/thread-to-tab-v0.1.0-${target}.tar.gz" -C "$package" thread-to-tab LICENSE
done

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$fixture" && sha256sum thread-to-tab-v0.1.0-*.tar.gz >SHA256SUMS)
else
    (cd "$fixture" && shasum -a 256 thread-to-tab-v0.1.0-*.tar.gz >SHA256SUMS)
fi
"$ROOT/scripts/verify-release-assets.sh" 0.1.0 "$fixture" >/dev/null

printf '0' >>"$fixture/thread-to-tab-v0.1.0-aarch64-apple-darwin.tar.gz"
if "$ROOT/scripts/verify-release-assets.sh" 0.1.0 "$fixture" >/dev/null 2>&1; then
    echo "corrupt release asset unexpectedly succeeded" >&2
    exit 1
fi
printf 'release asset tests passed\n'
