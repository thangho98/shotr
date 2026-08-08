#!/usr/bin/env bash
# Install shotr.app into /Applications.
#   bash packaging/install-macos.sh             install
#   bash packaging/install-macos.sh --uninstall remove
#
# Why this exists: `build-macos.sh` starts with `rm -rf dist/shotr.app`, so the
# bundle it produces is a build artefact, not somewhere to launch the app from.
#
# /Applications and not ~/Applications, even though the Linux installer keeps to
# ~/.local: on macOS /Applications is group-writable by admins, so the user
# folder buys no privilege back, and Finder's sidebar "Applications" *is*
# /Applications — an app in ~/Applications is invisible exactly where people
# look for it. ~/Applications is only the fallback for a non-admin account.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -w /Applications ]; then
    DEST="/Applications"
else
    DEST="$HOME/Applications"
fi
APP="$DEST/shotr.app"

if [[ "${1:-}" == "--uninstall" ]]; then
    # Earlier versions installed to ~/Applications, so clear both.
    rm -rf "$APP" "$HOME/Applications/shotr.app"
    echo "shotr removed."
    echo "Settings and history remain in ~/Library/Application Support/shotr —"
    echo "delete them by hand if you want them gone."
    exit 0
fi

if [ ! -d dist/shotr.app ]; then
    echo "dist/shotr.app is missing. Build it first:" >&2
    echo "    bash packaging/build-macos.sh" >&2
    exit 1
fi

mkdir -p "$DEST"
rm -rf "$APP"
cp -R dist/shotr.app "$APP"

# Two copies means two Spotlight hits for one app, and permission granted to
# whichever one you did not just launch.
if [ "$DEST" != "$HOME/Applications" ] && [ -d "$HOME/Applications/shotr.app" ]; then
    rm -rf "$HOME/Applications/shotr.app"
    echo "==> Removed the older copy in ~/Applications"
fi

echo "==> Installed to $APP"
echo
echo "Launch it from Spotlight (cmd+space, \"shotr\"). It has no Dock icon by"
echo "design — it lives in the menu bar."
echo
echo "To start it at login: System Settings > General > Login Items > + ,"
echo "then pick $APP."

# TCC keys a permission grant to the code signature *and* the path, so the copy
# above is a different app as far as Screen Recording is concerned. Granting it
# here rather than in dist/ is the point: dist/ gets deleted on the next build.
if codesign -dv "$APP" 2>&1 | grep -q "Signature=adhoc"; then
    echo
    echo "NOTE: this build is ad-hoc signed, so macOS will ask for Screen"
    echo "Recording again here, and again after every rebuild. To grant it once"
    echo "and be done, see \"Developing on macOS\" in packaging/README.md."
fi
