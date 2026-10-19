#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/gray-clean.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/gray-clean" ;;
    *) rel="x86_64-apple-darwin/gray-clean" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/gray-clean" ;;
    *) rel="x86_64-unknown-linux-musl/gray-clean" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/gray-clean.exe" "$here/target/release/gray-clean" "$here/target/debug/gray-clean.exe" "$here/target/debug/gray-clean"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "gray-clean binary not found in $here" >&2
exit 1
