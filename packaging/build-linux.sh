#!/usr/bin/env bash
# Build a portable Linux tarball: binary, icons, desktop entry and an installer.
#
# Not an AppImage: shotr links only system libraries that any desktop already
# has, so a tarball plus `install.sh` gives the same "download and run" without
# the 40 MB of bundled runtime or the FUSE requirement.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
OUT="dist/shotr-$VERSION-linux-$ARCH"

echo "==> Building"
cargo build --release

echo "==> Laying out $OUT"
rm -rf "$OUT" && mkdir -p "$OUT"
cp target/release/shotr "$OUT/"
cp install.sh README.md "$OUT/" 2>/dev/null || cp install.sh "$OUT/"

# Icons are generated from the same code that draws the tray icon, so the
# launcher and the tray can never drift apart.
echo "==> Generating icons"
mkdir -p "$OUT/icons"
cargo run --release --example gen_icon -- "$OUT/icons" 2>/dev/null || \
    echo "    (skipped: the gen_icon example is missing)"

mkdir -p dist
tar -C dist -czf "dist/shotr-$VERSION-linux-$ARCH.tar.gz" "$(basename "$OUT")"
echo "==> Done: dist/shotr-$VERSION-linux-$ARCH.tar.gz"
