#!/usr/bin/env bash
# Install shotr into ~/.local (user scope, no sudo needed).
#   ./install.sh            install
#   ./install.sh --uninstall remove
set -euo pipefail

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
SIZES=(32 48 64 128 256 512)

uninstall() {
    rm -f "$BIN_DIR/shotr" "$APP_DIR/shotr.desktop"
    for s in "${SIZES[@]}"; do
        rm -f "$ICON_ROOT/${s}x${s}/apps/shotr.png"
    done
    command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" || true
    echo "shotr removed."
    echo "Settings and history remain in ~/.config/shotr — delete them by hand if you want."
}

if [[ "${1:-}" == "--uninstall" ]]; then
    uninstall
    exit 0
fi

cd "$(dirname "$0")"

echo "==> Build release"
cargo build --release

echo "==> Generating icons"
TMP_ICONS="$(mktemp -d)"
trap 'rm -rf "$TMP_ICONS"' EXIT
cargo run --release --quiet --example gen_icon -- "$TMP_ICONS" >/dev/null

echo "==> Installing the binary into $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 755 target/release/shotr "$BIN_DIR/shotr"

echo "==> Installing icons into $ICON_ROOT"
for s in "${SIZES[@]}"; do
    mkdir -p "$ICON_ROOT/${s}x${s}/apps"
    install -m 644 "$TMP_ICONS/shotr-${s}.png" "$ICON_ROOT/${s}x${s}/apps/shotr.png"
done

echo "==> Installing the desktop entry into $APP_DIR"
mkdir -p "$APP_DIR"
# Desktop entries localise in place: `Key[vi]=` is picked up automatically
# when the session language is Vietnamese, so the launcher matches the app.
cat > "$APP_DIR/shotr.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=shotr
GenericName=Screenshot
GenericName[vi]=Ảnh màn hình
Comment=Capture and beautify screenshots (runs in the system tray)
Comment[vi]=Chụp và làm đẹp ảnh màn hình (chạy ở khay hệ thống)
Exec=$BIN_DIR/shotr
Icon=shotr
Terminal=false
Categories=Graphics;2DGraphics;RasterGraphics;
Keywords=screenshot;capture;screen;chup;man hinh;anh;
StartupNotify=true
StartupWMClass=shotr
Actions=Capture;

[Desktop Action Capture]
Name=Capture now
Name[vi]=Chụp ngay
Exec=$BIN_DIR/shotr --capture
DESKTOP

command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$ICON_ROOT" 2>/dev/null || true

echo
echo "Done. Try: shotr"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "NOTE: $BIN_DIR is not on your PATH — add it in your shell rc." ;;
esac

# The desktop session's PATH comes from systemd, not from your shell rc, so a
# bare `Exec=shotr` can resolve in a terminal and fail silently in the launcher.
# The entry above uses an absolute path; this only warns about the shell.
case ":${PATH}:" in
    *":$BIN_DIR:"*) ;;
    *) : ;;
esac
echo
echo "Bind a shortcut (Wayland gives no app a global key grab):"
echo "  COSMIC Settings → Keyboard → Shortcuts → Custom"
echo "  Command: shotr --capture"
