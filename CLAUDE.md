# shotr

A screenshot beautifier for Linux, Windows and macOS: capture, drop the shot on
a nice background, annotate it, redact anything sensitive, export.

Written in Rust with `eframe`/`egui`. There is no web layer and no C build
dependency — a checkout plus a Rust toolchain is the whole setup.

## Commands

```bash
cargo test                   # 155 tests, all fast; no network, no GPU, no display
cargo clippy --all-targets   # must be zero warnings
cargo run -- --capture       # region picker
cargo run -- --open FILE     # straight to the editor
./install.sh                 # build release, install to ~/.local/bin (Linux)
./install.sh --uninstall

cargo check --target x86_64-pc-windows-gnu   # needs mingw-w64-gcc
```

Packaging lives in `packaging/` — see `packaging/README.md`.

## `examples/`

Not demos. Three tools that need the crate compiled but do not belong in the
test suite, because each writes files or needs a machine to inspect:

```bash
cargo run --release --example gen_icon -- OUTDIR   # a build step, not an example
cargo run --release --example render_demo -- IMG   # render matrix + export timing
cargo run --release --example ocr_probe -- IMG     # both OCR engines, side by side
```

`gen_icon` is load-bearing: `install.sh` and `build-linux.sh` both call it, so
the tray icon and the launcher icon come from the same code and cannot drift.

Keep this directory honest. A probe written to answer one question should be
deleted once the answer is written down — leaving it behind implies the question
is still open. `probe.rs` lived here until the xcap findings moved into
`capture.rs` and this file.

## Layout

```
src/
  main.rs           argument parsing and the one decision: daemon, picker or editor
  capture.rs        screen capture, multi-monitor stitching        (all platforms)
  winlist.rs        window listing/capture façade                  (all platforms)
  wl_windows.rs       └─ Wayland toplevel protocols                (Linux only)
  tray.rs           StatusNotifierItem over D-Bus                  (Linux only)
  daemon.rs         tray-only background mode                      (Linux only)
  ipc.rs            single-instance over a unix socket             (unix only)
  wallpaper.rs      current desktop wallpaper, per platform
  settings.rs       everything persisted, plus presets
  annotate.rs       annotation layers and their rasterisation
  export.rs         encoding and save dialogs
  history.rs        recent shots
  ocr/
    mod.rs          engine selection
    tesseract.rs      └─ subprocess backend, the only one that reads Vietnamese
    detect.rs       finding emails, cards, IPs, phone numbers to redact
  render/
    mod.rs          the compositing pipeline
    background.rs   mesh gradients and presets
    frame.rs        rounded rects, shadows, edge-colour detection
    watermark.rs    text or logo, placed or tiled
    text.rs         glyph blitting
  app/
    mod.rs          application state and the eframe entry point
    canvas.rs       the central image area and all pointer interaction
    sidebar.rs      the control panel and bottom bar
    icons.rs        tool glyphs, drawn rather than shipped
    theme.rs        the visual style, in one place
    ocr_job.rs      running recognition off the UI thread
```

## Things that will bite you

These are all load-bearing. Each one cost real debugging time.

**A Wayland client cannot hide its own window.** `set_visible(false)` is a
documented no-op. This is why capture runs as a *fresh process* that grabs the
screen before it opens anything, and why the tray daemon exists at all. On
Windows and macOS a window can hide itself, so `shotr` with no arguments just
captures there. Do not "simplify" this back into one process.

**One renderer for preview and export.** `Scene::scale` is what makes the
preview and the exported file identical code. Anything that draws must go
through `render/`, not be special-cased for one of the two. This is the main
reason not to move the UI to a webview.

**`xcap::Monitor` geometry is not in pixels.** On this compositor it reports
`10320x4320` for a screen that captures at `3440x1440`. The captured frame is
the only honest number; positions are corrected by the ratio between the two.
See `capture::scaled_origin`.

**`xcap::Window::all()` returns nothing on Wayland.** It works on Windows and
macOS. `winlist` picks the implementation; `wl_windows` speaks
`ext_foreign_toplevel_list_v1` and `ext_image_copy_capture_v1` directly.

**`ocrs` cannot produce Vietnamese.** Its recognition model has a fixed ASCII
alphabet — no `ă â đ ê ô ơ ư`, no tone marks — so it is not merely worse at
Vietnamese, it is incapable. Tesseract with `vie` is preferred whenever present.

**egui diverts ctrl+wheel.** With the zoom modifier held, `smooth_scroll_delta`
is zero and the wheel arrives as `zoom_delta()`. `zoom_with_keyboard` is turned
off in `theme::apply` so egui does not rescale the whole UI on ctrl+plus.

**Desktop entries need absolute paths.** The graphical session's `PATH` does not
include `~/.local/bin`, so a bare `Exec=shotr` resolves in a terminal and fails
silently from the launcher.

## Conventions

**Comments explain why, never what.** The code says what it does. A comment
earns its place by recording a constraint, a measurement, or a decision that is
not visible from the code — a protocol quirk, a number that came from an
experiment, a design road not taken. If a comment restates the line below it,
delete it.

**Tests assert behaviour, and say what breaks.** Every assertion carries a
message naming the consequence, not the expression. Prefer a test that encodes a
real failure that happened over one that covers a line. Tests must not need a
display, a network, or a GPU.

When a test fails, work out whether the test or the code is wrong before
changing either. Several tests here exist because the first version asserted the
wrong thing — a rotated flat rectangle *shrinks* in width, `cos(π/2)` in `f32` is
not zero — and the code was right all along.

**Zero clippy warnings.** Not "few". The moment warnings become normal they stop
being read.

**English everywhere in the repository.** Code, comments, commit messages,
scripts, CI and documentation are English without exception.

**The interface is bilingual, and English is the source language.** User-facing
strings are written in English at the call site and wrapped in `t("…")`;
`i18n::VI` holds the Vietnamese. Strings with values in them use
`tf("… {name} …", &[("name", value)])` — `t` cannot go inside `format!`, and
positional substitution would scramble a sentence whose word order differs
between the two languages.

Three tests guard this: the table must stay sorted (the lookup is a binary
search, so an unsorted row is a translation that silently never appears), every
row must be non-empty, and a test *reads the source tree* and fails if any
`t("…")` has no entry — something the compiler cannot check. A missing
translation falls back to English, never to a blank or a key.

Deep diagnostics (`wl_windows`, `tesseract`, protocol errors) stay untranslated
English: they always appear inside a translated wrapper, and they are read by
whoever is debugging, not by whoever is taking a screenshot.

**Verify on the machine, not from memory.** This codebase is full of places
where the documented behaviour and the actual behaviour differ. Before
concluding something is impossible, check whether it is the chosen *library*
that cannot do it rather than the platform — that mistake has been made twice
here, with tray icons and with window listing, and both turned out to be
possible.

## Licence

GPL-3.0-only. `src/main.rs` carries the notice the licence asks each source
release to include; keep it there. Adding a dependency under a licence that
cannot be combined with the GPL — anything proprietary, or Apache-2.0 in the
GPLv2 direction — is a licensing decision, not a routine `cargo add`.

## Platform support

| | Linux | Windows | macOS |
|---|---|---|---|
| Screen capture | xcap | xcap | xcap |
| Window capture | Wayland protocols | xcap | xcap |
| Tray daemon | yes | not needed | not needed |
| Single instance | unix socket | no-op | unix socket |
| OCR (Vietnamese) | tesseract | tesseract if installed | tesseract if installed |

`cargo check --target x86_64-pc-windows-gnu` covers Windows from Linux. macOS
cannot be cross-checked without Apple's SDK; CI builds it on a real runner.
