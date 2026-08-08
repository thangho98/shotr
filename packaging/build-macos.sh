#!/usr/bin/env bash
# Build a macOS .app bundle and a .dmg. Must run on macOS: the bundle needs
# `hdiutil`, and codesigning needs Apple's toolchain. Cross-compiling from Linux
# would need Apple's SDK, which is not redistributable — CI uses a real runner.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist/shotr.app"
HOST=$(rustc -vV | awk '/^host:/ {print $2}')

echo "==> Building for both Intel and Apple Silicon"
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

# The permission dialog is drawn by the system before the app runs, so its text
# cannot go through `t()`. These carry the Vietnamese; Info.plist has English.
cp -R packaging/macos/*.lproj "$APP/Contents/Resources/"

# The icon comes from the same function that draws the tray icon, so the two
# cannot drift. `--target $HOST` reuses the artefacts the loop above already
# built; without it cargo compiles the whole dependency tree a third time.
echo "==> Generating the icon"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/iconset.iconset"
cargo run --release --quiet --target "$HOST" --example gen_icon -- "$TMP/png" >/dev/null
# iconutil accepts only this naming, and rejects a directory holding anything
# else — hence copying out of a separate directory rather than renaming in place.
while read -r px name; do
    cp "$TMP/png/shotr-$px.png" "$TMP/iconset.iconset/icon_$name.png"
done <<'SIZES'
16 16x16
32 16x16@2x
32 32x32
64 32x32@2x
128 128x128
256 128x128@2x
256 256x256
512 256x256@2x
512 512x512
1024 512x512@2x
SIZES
iconutil -c icns "$TMP/iconset.iconset" -o "$APP/Contents/Resources/shotr.icns"

# macOS ties a TCC grant — Screen Recording, Files and Folders — to the code
# signature, and the ad-hoc signature the linker leaves behind is derived from
# the binary's own bytes. Every rebuild therefore produces a new identity, looks
# like a brand new app, and asks for every permission again. Signing with a
# stable certificate fixes that; a self-signed one is enough, because what TCC
# remembers is the certificate, not the contents. See packaging/README.md.
if [ -n "${SHOTR_SIGN_IDENTITY:-}" ]; then
    echo "==> Signing as $SHOTR_SIGN_IDENTITY"
    codesign --force --sign "$SHOTR_SIGN_IDENTITY" "$APP"
else
    echo "==> Ad-hoc signature: macOS will re-ask for permissions after every build."
    echo "    Set SHOTR_SIGN_IDENTITY to a certificate name to stop that."
fi

echo "==> Packing the .dmg"
hdiutil create -volname "shotr" -srcfolder "$APP" -ov -format UDZO \
    "dist/shotr-$VERSION-macos.dmg"
echo "==> Done: dist/shotr-$VERSION-macos.dmg"
echo

# A self-signed certificate keeps macOS permissions across rebuilds, which is
# what it is for, but Gatekeeper still refuses a download that no authority it
# knows has vouched for. Only a Developer ID gets past that.
if [ -n "${SHOTR_SIGN_IDENTITY:-}" ]; then
    echo "Signed as $SHOTR_SIGN_IDENTITY. That is enough to keep permissions"
    echo "between builds, but not to ship: unless this is a Developer ID, macOS"
    echo "still blocks the .dmg on another machine. Right-click > Open there."
    exit 0
fi

echo "Unsigned. macOS blocks the first launch — right-click > Open, or sign it with:"
echo "  codesign --deep --force --sign \"Developer ID Application: NAME\" $APP"
