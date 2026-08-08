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

# The tarball is the artefact that always builds; .deb and .rpm need tools this
# machine may not have, and a missing one must not fail a release — the same
# bargain build-windows.sh strikes with NSIS.
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

echo "==> Staging a filesystem tree"
install -Dm755 target/release/shotr "$STAGE/tree/usr/bin/shotr"
# One desktop entry, rendered three ways. install.sh points Exec at ~/.local;
# a distribution package points it at /usr/bin. Keeping the text itself in one
# file is what stops the launcher entry drifting between the two.
mkdir -p "$STAGE/tree/usr/share/applications"
sed 's|@EXEC@|/usr/bin/shotr|g' packaging/linux/shotr.desktop.in \
    > "$STAGE/tree/usr/share/applications/shotr.desktop"
for s in 32 48 64 128 256 512; do
    install -Dm644 "$OUT/icons/shotr-$s.png" \
        "$STAGE/tree/usr/share/icons/hicolor/${s}x${s}/apps/shotr.png"
done

case "$ARCH" in
    x86_64) DEB_ARCH=amd64 ;;
    aarch64) DEB_ARCH=arm64 ;;
    *) DEB_ARCH=$ARCH ;;
esac

if command -v dpkg-deb >/dev/null; then
    echo "==> Building the .deb"
    DEB="$STAGE/deb"
    mkdir -p "$DEB/DEBIAN"
    cp -a "$STAGE/tree/usr" "$DEB/"

    # dpkg-shlibdeps reads the binary's NEEDED entries and names the package
    # owning each one. Writing Depends by hand would have been wrong since the
    # day xcap started linking pipewire, and wrong silently: the package
    # installs and the program then fails to start. It insists on a debian/
    # directory, hence the scratch one — it must not end up inside the package.
    SHLIB="$STAGE/shlibdeps"
    mkdir -p "$SHLIB/debian"
    printf 'Source: shotr\n\nPackage: shotr\nArchitecture: any\n' > "$SHLIB/debian/control"
    cp target/release/shotr "$SHLIB/shotr"
    DEPS=$(cd "$SHLIB" && dpkg-shlibdeps -O --ignore-missing-info ./shotr 2>/dev/null \
        | sed 's/^shlibs:Depends=//') || DEPS=""

    {
        echo "Package: shotr"
        echo "Version: $VERSION"
        echo "Architecture: $DEB_ARCH"
        echo "Section: graphics"
        echo "Priority: optional"
        echo "Maintainer: shotr <noreply@github.com>"
        echo "Homepage: https://github.com/thangho98/shotr"
        [ -n "$DEPS" ] && echo "Depends: $DEPS"
        echo "Description: Capture a screenshot and make it presentable"
        echo " Drop it on a gradient, round the corners, add a shadow, annotate"
        echo " it, redact anything sensitive, export. Runs entirely on the"
        echo " machine; no image ever leaves it."
    } > "$DEB/DEBIAN/control"

    dpkg-deb --build --root-owner-group "$DEB" \
        "dist/shotr_${VERSION}_${DEB_ARCH}.deb" >/dev/null
    echo "==> Done: dist/shotr_${VERSION}_${DEB_ARCH}.deb"
else
    echo "==> Skipping the .deb: dpkg-deb not found (package dpkg)"
fi

if command -v rpmbuild >/dev/null; then
    echo "==> Building the .rpm"
    # $STAGE comes from `mktemp -d`, which is already absolute — prefixing $PWD
    # produced /home/runner/work/shotr/shotr//tmp/tmp.XXXX and rpmbuild failed
    # on a path that reads almost right. Only _rpmdir is relative to here.
    rpmbuild -bb \
        --define "_topdir $STAGE/rpm" \
        --define "_rpmdir $PWD/dist" \
        --define "_rpmfilename shotr-$VERSION.$ARCH.rpm" \
        --define "_shotr_version $VERSION" \
        --define "_shotr_stage $STAGE/tree" \
        --buildroot "$STAGE/buildroot" \
        packaging/linux/shotr.spec >/dev/null
    echo "==> Done: dist/shotr-$VERSION.$ARCH.rpm"
else
    echo "==> Skipping the .rpm: rpmbuild not found (package rpm / rpm-build)"
fi
