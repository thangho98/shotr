# shotr

Take a screenshot and make it presentable: drop it on a gradient, round the
corners, add a shadow, annotate it, redact anything sensitive, export.

Written in Rust. No webview, no C build dependency — a Rust toolchain is the
whole setup. Everything runs locally; no image ever leaves the machine.

Runs on **Linux, Windows and macOS**. The interface is available in English and
Vietnamese — switch with the EN/VI buttons at the top of the sidebar.

## Install

### Linux

```bash
./install.sh              # build and install into ~/.local/bin
./install.sh --uninstall
```

Then run `shotr`. It lives in the system tray — click the icon to capture.

### Windows and macOS

Grab a build from Releases, or make one:

```bash
bash packaging/build-windows.sh   # .zip, plus setup.exe when NSIS is present
bash packaging/build-macos.sh     # universal .app inside a .dmg
```

Neither artefact is signed, so both systems warn on first launch. On macOS,
right-click the app and choose Open.

## Usage

```
shotr                       run in the system tray (Linux)
shotr --capture             capture, then drag out a region
shotr --capture --full      capture everything, straight to the editor
shotr --capture --monitor N open on monitor N
shotr --open [FILE]         open an existing image
```

Wayland gives no application a global key grab, so the shortcut has to come from
the desktop. On COSMIC: Settings → Keyboard → Shortcuts → Custom, with the
command `shotr --capture`.

### Editor controls

| | |
|---|---|
| Ctrl + wheel | zoom about the pointer |
| Wheel · Shift + wheel | scroll vertically · horizontally |
| Middle-drag | pan |
| Ctrl + `0` · Ctrl + `1` | fit · 100% |
| Ctrl + C · Ctrl + S | copy · save |
| Ctrl + Z · Ctrl + Shift + Z | undo · redo |
| Double-click the image | copy and close |
| Space (picker) | switch between region and window |

## What it does

**Capture** — the whole desktop, a dragged region, or a single window. With
several monitors it captures once and cuts each screen out of that one snapshot,
so every view shows the same instant. A window is copied from its own buffer, so
one sitting **behind another still comes out whole**.

**Beautify** — 19 gradient presets, a background generated from the image's own
colours, the desktop wallpaper, or a colour or image of your own. Padding, an
inset frame that detects the screenshot's own edge colour, corner radius, drop
shadow, 13 aspect presets plus a custom size.

**Annotate** — arrows, boxes, ellipses, text typed directly on the image (with
input-method support for Vietnamese), blur, and paint that runs from translucent
marker to solid cover.

**Redact** — on-device text recognition finds email addresses, card numbers
(Luhn-checked), IP addresses, API keys and phone numbers. Hide them in one
click, or pick words by hand.

**Watermark** — text or a logo, anchored on a nine-square grid or tiled across
the image, with size, opacity and rotation.

**Export** — PNG, JPEG, WebP. Copy to the clipboard, save, or save under a
filename template.

## Vietnamese text recognition

Needs **Tesseract with the `vie` pack**:

```bash
sudo pacman -S tesseract tesseract-data-vie tesseract-data-eng   # Arch
sudo apt install tesseract-ocr tesseract-ocr-vie                 # Debian/Ubuntu
brew install tesseract tesseract-lang                            # macOS
```

Without it shotr falls back to the pure-Rust `ocrs` engine, whose recognition
model has a fixed ASCII alphabet and **no Vietnamese diacritics at all** — so
`Tiếng Việt` comes back as `Ti?ng Vi?t`. The sidebar always says which engine
read the image.

## Development

```bash
cargo test                   # 155 tests; no network, no GPU, no display
cargo clippy --all-targets   # must be zero warnings
```

Conventions and the things that will bite you are in [CLAUDE.md](CLAUDE.md).
Packaging is in [packaging/README.md](packaging/README.md).

## Licence

Not chosen yet.
