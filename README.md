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
bash packaging/install-macos.sh   # that .app into /Applications
```

Install it rather than running it out of `dist/`: the build script clears that
directory each time, and macOS ties screen-recording permission to where the app
lives. On macOS the `.app` is the only supported route — a bare binary borrows the
permission of whatever launched it.

Neither artefact is signed, so both systems warn on first launch. On macOS,
right-click the app and choose Open.

macOS will ask for **Screen Recording** the first time you capture; without it
nothing is written and the capture looks like it was cancelled. Grant it, then quit
shotr from the menu bar and start it again — macOS reads that permission once per
process. If you are building repeatedly, sign the bundle so the grant survives:
see `packaging/README.md`.

shotr lives in the notification area on Windows and in the menu bar on macOS,
the same as it does in the Linux tray. It takes no Dock icon on macOS while it
is sitting there.

## Usage

The tray menu is where you say what to capture:

| | |
|---|---|
| **Capture a region…** | drag a rectangle. On macOS this is the system overlay; elsewhere shotr freezes the screen first |
| **Capture a whole screen ▸** | all screens together, or one of them by name |
| **Capture a window…** | on macOS, click a window in the system overlay. On Linux and Windows, a submenu of open windows by title |
| **Open image…**, **Recent shots…**, **From clipboard** | work on an image you already have |
| **Preferences…** | language, where files go, export defaults, redaction, and the macOS permission |

That choice is made once, before anything is grabbed. The editor then works on
what it was given — it has no source dropdown, by design.

The same choices from a terminal or a keyboard shortcut:

```
shotr                       run in the system tray
shotr --capture             capture, then drag out a region
shotr --capture --full      capture everything, straight to the editor
shotr --capture --monitor N one monitor, straight to the editor
shotr --capture --window ID one window, straight to the editor
shotr --open [FILE]         open an existing image
shotr --clipboard           open whatever image is on the clipboard
shotr --history             recent shots, and the other ways in
shotr --settings            the Preferences window
```

Wayland gives no application a global key grab, so the shortcut has to come from
the desktop. On COSMIC: Settings → Keyboard → Shortcuts → Custom, with the
command `shotr --capture`. Running that while shotr is already in the tray pokes
the process that is up rather than starting a second one — on every platform.

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
one sitting **behind another still comes out whole**. On macOS the region and
window pickers are the system's own — the same overlay `Cmd-Shift-4` gives you, so
it covers the menu bar and the Dock and lets space switch between region and
window.

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
cargo test                   # 163 tests; no network, no GPU, no display
cargo clippy --all-targets   # must be zero warnings
```

`rust-toolchain.toml` pins the compiler, so rustup fetches that exact version
the first time you build here and CI uses the same one — "zero warnings" means
nothing if the machine judging it keeps changing. Linux additionally needs a few
system headers; see [packaging/README.md](packaging/README.md).

Conventions and the things that will bite you are in [CLAUDE.md](CLAUDE.md).
Packaging is in [packaging/README.md](packaging/README.md).

## Licence

[GNU General Public License v3.0](LICENSE).

You may use, study, change and share this program. If you distribute a modified
version, you must release its source under the same licence — a fork that ships
to other people has to stay open.

Two limits worth knowing: the obligation is triggered by *distribution*, so
someone may fork it privately and never publish, and the GPL — unlike the AGPL —
does not treat running software over a network as distribution. Neither matters
much for a desktop screenshot tool, but say so rather than imply more reach than
the licence has.
