# shotr

A screenshot beautifier for Linux, Windows and macOS: capture, drop the shot on
a nice background, annotate it, redact anything sensitive, export.

Written in Rust with `eframe`/`egui`. There is no web layer and no C build
dependency — a checkout plus a Rust toolchain is the whole setup.

## Commands

```bash
cargo test                   # 163 tests, all fast; no network, no GPU, no display
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

`gen_icon` is load-bearing: `install.sh`, `build-linux.sh` and `build-macos.sh`
all call it, so the tray icon, the launcher icon and the macOS `.icns` come from
the same code and cannot drift. It draws through `render::icon`, not `tray` —
`tray` is Linux-only, and a module the other two platforms cannot compile cannot
give them an icon.

Keep this directory honest. A probe written to answer one question should be
deleted once the answer is written down — leaving it behind implies the question
is still open. `probe.rs` lived here until the xcap findings moved into
`capture.rs` and this file.

## Layout

```
src/
  main.rs           argument parsing and the one decision: daemon, picker,
                    one window, or straight to the editor
  capture.rs        screen capture, multi-monitor stitching        (all platforms)
  winlist.rs        window listing/capture façade                  (all platforms)
  wl_windows.rs       └─ Wayland toplevel protocols                (Linux only)
  tray/
    mod.rs          tray façade and the command set                (all platforms)
    sni.rs            └─ StatusNotifierItem over D-Bus             (Linux only)
    native.rs         └─ tray-icon on a winit loop                 (Win/macOS)
  daemon.rs         tray-only background mode                      (all platforms)
  ipc.rs            single instance: unix socket, or a named pipe  (all platforms)
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
    icon.rs         the app icon, drawn rather than shipped     (all platforms)
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
screen before it opens anything, and why the tray daemon exists at all. Windows
and macOS *can* hide a window, so they do not need the trick — they use it
anyway, because one capture path known to work everywhere beats two. Do not
"simplify" this back into one process.

**Fullscreen on macOS means a Space of its own.** `with_fullscreen(true)` maps to
winit's `Fullscreen::Borderless`, which on macOS is `toggleFullScreen:` — the
picker slides off to a new Space with the animation to match, and a screenshot
overlay that takes a second to arrive is not an overlay. Worse, it is invisible
from anywhere else: with the picker up, `screencapture` of every display showed
only wallpaper, because each display captures its *active* Space. So macOS
positions the picker by hand instead, over the monitor holding the pointer. The
menu bar and the Dock still float above it — they sit at window levels no
ordinary window can ask for — and that is the one part of the screen not covered.

**Only macOS needs the pointer's monitor.** Windows and Wayland put a borderless
fullscreen surface on the monitor that already has focus, which is the wanted
behaviour and costs nothing; `capture::monitor_under_cursor` is `cfg(macos)` for
that reason and not because the others were forgotten. On macOS the pointer comes
from `CGEventCreate`/`CGEventGetLocation`, declared by hand the way `ipc` declares
its Win32 calls. It agrees with `xcap::Monitor::from_point`, which is
`CGGetDisplaysWithPoint` underneath, and with `monitor_bounds`, which is
`CGDisplayBounds` — all three are logical points about one origin, and that is
also the unit a window position is given in. The warning below about `xcap`
geometry is about the *Linux* backend; it does not apply to these.

**`crop_imm` clamps, and a clamped miss is a 0×0 image.** The editor keeps the
whole desktop snapshot, not only the image it is editing: `--capture --full
--monitor N` cuts one screen out of it, and "Back to selection" hands the picker
something to select from. `Start::Editor` used to leave it as the 640×360 startup
placeholder, so every monitor rectangle fell outside — and clamping turned each
miss into an empty image. A zero-sized texture ends the process. Hence both
`Start::Editor` seeding `desktop_full`, and `cut_monitor` falling back to the
whole desktop for any rectangle not wholly inside, which is the rule an unknown
monitor index already followed.

**The windowed Select screen is reachable only from "Back to selection".** The
picker draws no sidebar at all, so that button is the single door to History,
"Open file…" and "From clipboard" — remove it and three features leave with it,
silently, since nothing stops compiling. It was removed once for exactly that
reason and put straight back.

**The tray menu is where you choose what to capture, and the only place.** A
region, one whole screen (or all of them), or one window — decided before
anything is grabbed, because that decision *is* the capture. The editor shows
what it was handed and offers no way to change it: a source dropdown there
re-opens a question already answered, and answering it a second time meant
re-cutting an image the editor might not even hold. `tray::Command` is the whole
vocabulary; every entry maps to one `shotr --capture …` invocation.

**`xcap::Window::all()` is not a list of windows.** On macOS it is
`CGWindowListCopyWindowInfo` passed through with no filter whatsoever — every
layer the window server composites. Unfiltered, the menu offered one entry per
menu bar icon, the menu bar itself, the recording indicator, a 64×64 app badge
and a "Gesture Blocking Overlay" for each window parked in Stage Manager, and a
1×1 window at (1e9, 1e9). `winlist::worth_offering` is the filter, and it is
geometric wherever it can be, because names are localised and process names are
not.

**A window in Stage Manager is captured as its tilted thumbnail.** The window
server reports a parked window at the *tile's* geometry — Slack, really
1440×900, came back as 128×169 — and hands over the perspective-warped preview
Stage Manager draws. `screencapture -l<id>`, Apple's own tool, returns exactly
the same skewed 256×338 image, which is how this was settled: nothing reachable
through `CGWindowListCreateImage` sees the real window. So `winlist::raise`
brings the application forward first and waits for it to arrive, which is the
only lever there is. It runs for every window, not only parked ones, because a
parked window cannot be told apart from a small one — all we can read is a size,
and 128×169 is a size a window is allowed to be.

**`ActivateAllWindows` alone will not do it.** macOS lets one application
activate another only when the *asking* one is already frontmost, and the
process doing this has no window at all — it is about to take a screenshot.
Without `ActivateIgnoringOtherApps` beside it, the call is accepted and quietly
does nothing: Slack stayed at 128×152 in the strip and was photographed there.
That flag is deprecated and has no replacement for this case.

**A tray menu that lists windows has to be rebuilt, and only Linux gets that for
free.** `ksni` asks for the menu each time one opens, so `window_items` is always
current. `tray-icon` hands the menu to the system, which shows it without telling
us — a list built at startup would name windows that closed hours ago.

**Never rebuild that menu on the click.** `set_menu` is
`NSStatusItem.setMenu:`, and tray-icon's `mouseDown:` reports the click and
*then* opens the menu, in that order and in one call. Rebuilding when the event
arrives therefore replaces the menu the system is already showing, and macOS
closes it — the menu vanished the instant it was clicked, which is a far worse
bug than a stale entry. `native.rs` rebuilds on `Enter` instead, which fires
while the pointer is still travelling and no menu is up, and skips it when a
click arrived in the same batch. `Move` is ignored outright: it repeats for as
long as the pointer rests on the icon. Rebuilding mints new `MenuId`s, so
`actions` must be replaced in the same breath or every entry does the wrong
thing.

**A window identifier survives the trip between processes.** The tray daemon
lists windows, but a *different, freshly spawned* process captures the one that
was picked, so the identifier goes over the command line. That works because both
backends hand out one meant to be shared: `ext_foreign_toplevel_list_v1` says so
in the protocol, and xcap's is the system's own window id. An index into the list
would not survive — the new process enumerates its own.

**The tray needs an event loop everywhere except Linux.** `ksni` talks D-Bus on
its own thread, so the Linux daemon is a plain `loop { try_recv; sleep }`.
`tray-icon` cannot work that way: it needs a platform event loop on the thread
that created the icon, and on macOS that thread must be the main one, with the
icon built *after* NSApplication is up — which is why `native.rs` builds it in
winit's `resumed` and not before `run_app`. This is why `tray::run` owns the
thread instead of handing back a handle.

**Do not reach for `tray-icon` on Linux.** It hard-requires GTK3, libxdo and
libappindicator there, which is the C build dependency this project does
without. That is a property of that crate, not of Linux tray icons — `ksni`
needs none of them. The `tray/` façade exists to keep the two apart.

**macOS: set the activation policy, not `LSUIElement`.** The daemon has no
window and would otherwise take a Dock icon and a menu bar. `LSUIElement` in
`Info.plist` would fix that for the *whole bundle* — including the editor, which
does want both. winit's `ActivationPolicy::Accessory` applies to the one process
that asked for it.

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

**egui's bundled font stops short of Vietnamese.** It has Latin Extended-A, so
`ă â đ ê ô ơ ư` draw, but not Latin Extended Additional (U+1EA0–U+1EF9), which
is where every tone mark on those letters lives — `ủ ố ụ ổ ớ ề` came out as
empty boxes. `render::text::load_system_font` is what replaces it, and its
`FONT_CANDIDATES` held Linux paths only for a while: the lookup returned `None`
on macOS and Windows, the interface fell back to the bundled font, and nothing
said so. One list covers all three platforms — a path belonging to another
system costs a failed `read` and nothing more.

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
| Tray daemon | ksni (SNI over D-Bus) | tray-icon on winit | tray-icon on winit |
| Single instance | unix socket | named pipe | unix socket |
| OCR (Vietnamese) | tesseract | tesseract if installed | tesseract if installed |

`cargo check --target x86_64-pc-windows-gnu` covers Windows from Linux. macOS
cannot be cross-checked without Apple's SDK; CI builds it on a real runner.
