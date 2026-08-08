#!/usr/bin/env bash
# Build the Windows binary and a zip. Runs either on Windows (msvc) or on Linux
# with mingw-w64 installed, which is how it is checked during development.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGET="${TARGET:-x86_64-pc-windows-gnu}"
OUT="dist/shotr-$VERSION-windows-x64"

echo "==> Building for $TARGET"
rustup target add "$TARGET" >/dev/null 2>&1 || true
cargo build --release --target "$TARGET"

rm -rf "$OUT" && mkdir -p "$OUT"
cp "target/$TARGET/release/shotr.exe" "$OUT/"
cp packaging/windows/README.txt "$OUT/" 2>/dev/null || true

mkdir -p dist
# Windows ships no `zip`, and neither does the Git Bash this script runs under
# there — the first CI build got all the way through an nine-minute compile
# before dying on it. PowerShell is the archiver that is always present on
# Windows, as `zip` is on Linux, so take whichever this machine has. Both are
# given the directory rather than its contents, so either way the archive
# unpacks into one folder instead of scattering files.
if command -v zip >/dev/null; then
    (cd dist && zip -qr "shotr-$VERSION-windows-x64.zip" "$(basename "$OUT")")
else
    powershell -NoProfile -Command \
        "Compress-Archive -Force -Path '$OUT' -DestinationPath 'dist/shotr-$VERSION-windows-x64.zip'"
fi
echo "==> Done: dist/shotr-$VERSION-windows-x64.zip"

# The installer needs NSIS. It is optional so the zip is always produced.
if command -v makensis >/dev/null; then
    echo "==> Building the NSIS installer"
    # NSIS resolves relative paths against the .nsi file, not the working
    # directory — `OutFile` in there climbs two levels for the same reason.
    # `../$OUT` looked right and pointed at packaging/dist, so `File` found
    # nothing and no installer has ever been produced.
    makensis -DVERSION="$VERSION" -DSOURCE="../../$OUT" packaging/windows/shotr.nsi
    echo "==> Done: dist/shotr-$VERSION-setup.exe"
else
    echo "==> Skipping the installer: makensis not found (package mingw-w64-nsis / nsis)"
fi
