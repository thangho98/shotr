#!/usr/bin/env bash
# Build a macOS .app bundle and a .dmg. Must run on macOS: the bundle needs
# `hdiutil`, and codesigning needs Apple's toolchain. Cross-compiling from Linux
# would need Apple's SDK, which is not redistributable — CI uses a real runner.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist/shotr.app"

echo "==> Building for cả Intel và Apple Silicon"
for t in x86_64-apple-darwin aarch64-apple-darwin; do
    rustup target add "$t" >/dev/null 2>&1 || true
    cargo build --release --target "$t"
done

echo "==> Joining into a universal binary"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/shotr" \
    target/x86_64-apple-darwin/release/shotr \
    target/aarch64-apple-darwin/release/shotr

sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
[ -f packaging/macos/shotr.icns ] && cp packaging/macos/shotr.icns "$APP/Contents/Resources/"

echo "==> Packing the .dmg"
hdiutil create -volname "shotr" -srcfolder "$APP" -ov -format UDZO \
    "dist/shotr-$VERSION-macos.dmg"
echo "==> Done: dist/shotr-$VERSION-macos.dmg"
echo
echo "Unsigned. macOS blocks the first launch — right-click > Open, or sign it with:"
echo "  codesign --deep --force --sign \"Developer ID Application: NAME\" $APP"
