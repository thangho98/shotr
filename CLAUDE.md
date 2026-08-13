# shotr

A screenshot beautifier for Linux, Windows and macOS: capture, drop the shot on
a nice background, annotate it, redact anything sensitive, export.

Written in Rust with `eframe`/`egui`. There is no web layer and no C build
dependency — a checkout plus a Rust toolchain is the whole setup.

## Commands

```bash
cargo test                   # 175 tests, all fast; no network, no GPU, no display
cargo clippy --all-targets   # must be zero warnings
cargo run -- --capture       # region picker
cargo run -- --open FILE     # straight to the editor
cargo run -- --settings      # the Preferences window
cargo run -- --history       # the hub: recent shots, open, paste
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
`capture/` and this file.

## Layout

```
src/
  main.rs           argument parsing and the one decision: daemon, Preferences,
                    picker, one window, the hub, or straight to the editor
  capture/
    mod.rs          capture façade, multi-monitor stitching        (all platforms)
    xcap.rs           └─ xcap monitors                             (Linux/Windows)
    macos.rs          └─ /usr/sbin/screencapture                   (macOS only)
  winlist.rs        window listing/capture façade                  (all platforms)
  wl_windows.rs       └─ Wayland toplevel protocols                (Linux only)
  tray/
    mod.rs          tray façade, the command set, and its arguments
    sni.rs            └─ StatusNotifierItem over D-Bus             (Linux only)
    native.rs         └─ tray-icon on a winit loop                 (Win/macOS)
  daemon.rs         tray-only background mode                      (all platforms)
  ipc.rs            single instance: unix socket, or a named pipe  (all platforms)
  hotkey/
    mod.rs          hotkey text, the bindable actions, candidates   (all platforms)
    macos.rs          └─ Carbon registration, and Apple's own       (macOS only)
  notify.rs         one line from a process with no window                (all platforms)
  wallpaper.rs      current desktop wallpaper, per platform
  settings/
    mod.rs          config paths, presets, load/save
    prefs.rs          └─ application behaviour, edited in Preferences
    style.rs          └─ the look of one shot; this is what a Preset stores
    ratio.rs          └─ output shape and the social-media sizes
    watermark.rs      └─ watermark position and lettering
  prefs_ui/
    mod.rs          the Preferences window: `shotr --settings`
    shell.rs          └─ its own window: frame, title bar, the nav card that
                         hangs off the left edge, and the status bar
    icons.rs          └─ the six nav glyphs, drawn rather than shipped
    permission.rs     └─ the screen recording grant                (macOS only)
    sections.rs     general, export and redaction policy
    shortcuts.rs      └─ binding capture hotkeys                    (macOS only)
    about.rs        version, and the editor's fixed keys
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
    shell.rs        the editor's own window: frame, overhanging sidebar card,
                    tool pill, top bar, status bar, drag and resize
    controls.rs       └─ the controls the tool pill's options row is built from
    canvas.rs       the central image area and all pointer interaction
    sidebar.rs      what goes inside the sidebar card
    icons.rs        tool and chrome glyphs, drawn rather than shipped
    theme.rs        the visual style, in one place
    ocr_job.rs      running recognition off the UI thread
```

## Things that will bite you

These are all load-bearing. Each one cost real debugging time.

**The editor has no system titlebar, and that costs three settings that have to
agree.** It draws its own rounded frame with a sidebar card standing proud of
it, which only works if the window is *both* undecorated and transparent —
otherwise the corners and the shadow composite against an opaque rectangle and
the whole effect reads as a grey box with a grey box on it. Transparency can
only be asked for at creation (`with_transparent(true)` in `main.rs`), so both
viewport branches ask for it even though only one of them is the editor; the
picker becomes the editor without the window being recreated. And
`leave_fullscreen` must send `Decorations(false)`, not `true` — turning them
back on there puts a system titlebar above the card's own.

The third setting is `clear_color`, and it is the one that is *not* uniform:
transparent for the editor, `panel_fill` for the fullscreen picker. A
transparent clear under the picker is a hole straight through to the desktop
being selected on, anywhere the shot does not reach.

Preferences pays the same three costs, in `prefs_ui/shell.rs`, and it is a
*different* card: shorter than the body it sits beside and hanging off the
frame's left edge, where the editor's is taller than the frame and merely
overlaps it. The two are not one widget with a parameter. What they do share is
`theme::card_surface` and `app::shell::resize_bands` — the second takes which
edges to leave alone, because in both windows a card is sitting on one and wants
the clicks. Preferences gives up west resizing entirely for that reason.

**There is no z-order between egui panels, so the overhang cannot be panels.**
A `SidePanel` cannot overlap a `CentralPanel`, and the whole design is a card
that overlaps the frame by 20px and hangs 16px off it top and bottom. `shell.rs`
is therefore one painter and a set of explicit rectangles, painted back to
front, with `Ui::new_child` for anything that needs layout. Within one layer
egui gives the click to the *last* widget that claimed the spot, which is what
the drag bands (allocated before the buttons on them) and the resize bands
(allocated after everything) rely on.

**`Ui::new_child` without an `id_salt` gives every child the same id.** Read
`ui.rs:269`: the salt defaults to `Id::from("child")`, so five sibling children
share one `stable_id` and are told apart only by `next_auto_id_salt` — a
*counter*. Skip one conditionally, as the shell does when the tool pill will not
fit, and every later child silently changes identity: scroll offsets jump back
to the top and any open combo box shuts. Give every `new_child` an explicit
`.id_salt("…")`. Nothing warns about this.

**`viewport().maximized` is not always reported.** It is an `Option`, and a
compositor that never fills it in makes `unwrap_or(false)` a one-way door:
every double click on the titlebar sends `Maximized(true)` and the window can
never come back down. `shell` keeps its own `maximised` flag and only re-syncs
it from the viewport when the viewport actually has an opinion.

**A Wayland client cannot hide its own window.** `set_visible(false)` is a
documented no-op. This is why capture runs as a *fresh process* that grabs the
screen before it opens anything, and why the tray daemon exists at all. Windows
and macOS *can* hide a window, so they do not need the trick — they use it
anyway, because one capture path known to work everywhere beats two. Do not
"simplify" this back into one process.

**macOS has no shotr picker, and that is deliberate.** Region and window capture
there run `/usr/sbin/screencapture -i`, so the crosshair, the live dimensions,
space to switch to window mode and escape to cancel are all Apple's UI. Shottr
and Xnapper both do exactly this — measured with `pgrep -x screencapture` while
their pickers were open — and `otool -L` on Xnapper shows *no* reference to
ScreenCaptureKit, so the system tool alone is enough for a shipping beautifier.

That retired a pile of workarounds: a hand-positioned picker window (because
`with_fullscreen` on macOS is `toggleFullScreen:`, which throws the window into a
Space of its own), `capture::monitor_under_cursor` and its `CGEventCreate` FFI,
`monitor_bounds`, and the menu bar and Dock never being covered. Apple's overlay
covers them.

**Two capture paths on macOS is a decision, not drift.** The rule elsewhere in
this file is that one path everywhere beats two. The exception exists because
xcap reaches macOS through `CGWindowListCreateImage`, which Apple *obsoleted* in
15.0 — it still links only because `objc2-core-graphics` redeclares it in Rust
and dodges the C availability attribute. Do not "simplify" macOS back onto xcap.

**TCC attributes screen recording to the responsible process.** A child
`screencapture` inherits the grant given to `shotr.app`, which is what makes the
whole approach work. This was misread once: `screencapture -i` run from a shell
without the grant fails with `could not create image from rect`, and that was
briefly taken to mean the approach was impossible. It meant the *shell* had no
grant. Permission has to be given to the bundle in `/Applications`; a binary run
from a terminal borrows the terminal's grant instead.

**No `-r` on `screencapture`.** Measured: it changes the dpi metadata only, never
a pixel, and `export` re-encodes so the source dpi never reaches the output file.
It looks tidy to add. It does nothing.

**No `-x` on `screencapture`.** It silences the shutter, and the shutter is the
only feedback a capture gives: `--capture --copy` opens no window at all, and
even the ordinary path fires before the editor appears. macOS plays it for every
other screenshot on the machine, so its absence reads as the hotkey having
missed.

**Space turns a region capture into a window capture, so the region command line
has to carry `-o` too.** It did not, and the shot came back with the window's own
shadow baked in as a semi-transparent border — measured at 112/76/112/148px
around a 3492×2258 window — which `render` composited over the background and
then cast its *own* shadow around. It read as "a huge drop shadow underneath,
only when capturing a window", and the culprit was the *region* command, the one
the hotkey is bound to. `-o` is honoured under `-i -W`; it was simply absent
elsewhere. Both flags are byte-for-byte no-ops on a rectangle, measured, so both
interactive sources now pass both.

**A transparent edge is not a black edge, and `border_color` used to disagree.**
`screencapture -o` masks the shot to the window's own shape, so every pixel
outside it is `(0, 0, 0, 0)` — and shotr's own editor window, being undecorated
and transparent, carries a 16px ring of exactly that. `render::frame::border_color`
sampled that ring, found every sample in perfect agreement, averaged them to
`(0, 0, 0)` and then *forced alpha to 255*: the inset frame came out opaque black
around every window capture. It reads as the capture having a black border baked
in, which sends you to `capture/` — where nothing is wrong, and a `screencapture
-l<id> -o` of the same window settles it in seconds by coming back correctly
transparent. Only fully opaque pixels vote now, and the majority is still
measured against the whole ring, so a mostly-transparent edge fails the 60% test
without needing a threshold of its own. `inset_only_if_detected` then drops the
frame, and the background shows through the margin — which is what a transparent
window is supposed to look like.

The inset frame is a *filled* rounded rectangle the shot then sits on top of, not
a ring, which is why that black was not 17px of trim: every transparent pixel
anywhere in the shot showed it. iPhone Mirroring is the case that makes this
obvious — a phone outline inside a 444×972 window, so the black filled all four
corners, and it reads as the capture having come back black. Any window whose
shape is not its rectangle does the same thing.

**A shadow is cast by what is painted, which is not always the rectangle.**
Dropping that black frame left a second, quieter bug behind: `shadow_layer` fills
`Placement` and blurs it, so a shape-masked window cast a *rectangle's* shadow
across its own transparent corners — a grey smudge on the background that reads
as dirt rather than as a shadow. It only shows up on a window that is not a
rectangle, so an ordinary screenshot never reveals it. `shadow_layer` therefore
takes an optional mask whose alpha narrows the silhouette, and `render_detailed`
passes the shot when `inset == 0`. With a frame the rectangle really is the
caster — the frame is solid — so the mask must *not* be passed there.

**`crop_imm` clamps, and a clamped miss is a 0×0 image.** The editor keeps the
whole desktop snapshot, not only the image it is editing: `--capture --full
--monitor N` cuts one screen out of it, and "Back to selection" hands the picker
something to select from. `Start::Editor` used to leave it as the 640×360 startup
placeholder, so every monitor rectangle fell outside — and clamping turned each
miss into an empty image. A zero-sized texture ends the process. Hence both
`Start::Editor` seeding `desktop_full`, and `cut_monitor` falling back to the
whole desktop for any rectangle not wholly inside, which is the rule an unknown
monitor index already followed.

That fallback is also what makes macOS need no special case: there `monitor_views`
is empty, because Apple's overlay returns a finished image rather than a desktop to
slice, so every rectangle misses and `apply_source` becomes a no-op on its own.

**History, "Open image…" and "From clipboard" live on the tray menu.** They used
to hang off the windowed Select screen, reachable only by capturing first and
then pressing "Back to selection" — and that button was removed once for looking
redundant, taking three features with it silently, since nothing stops compiling.
macOS no longer passes through that screen at all, so the tray is now the door:
`--history` opens it as a hub, `--clipboard` goes straight to the editor. On Linux
and Windows the Select screen still exists and keeps its buttons too.

**The tray menu is where you choose what to capture.** A region, one whole screen
(or all of them), or one window — decided before anything is grabbed, because
that decision *is* the capture. The editor shows what it was handed and offers no
way to change it: a source dropdown there re-opens a question already answered.
`tray::Command` is the whole vocabulary and `Command::args` is the single mapping
from an entry to a `shotr …` invocation, so a new variant that forgets one fails a
test rather than silently launching the daemon.

macOS is the exception for windows only: its entry runs `screencapture -i -W` and
lets Apple's overlay name them, because a menu of windows there would ask the same
question twice.

**`xcap::Window::all()` is not a list of windows.** On Windows it is worth
filtering — `winlist::worth_offering` drops our own window and anything the size
of an icon. It used to matter far more on macOS, where the same call is
`CGWindowListCopyWindowInfo` passed through unfiltered: every layer the window
server composites, including one entry per menu bar icon, the menu bar itself, the
recording indicator, a 64×64 app badge, a "Gesture Blocking Overlay" per window
parked in Stage Manager, and a 1×1 window at (1e9, 1e9). macOS no longer goes
through xcap, which retired that filter *and* the Stage Manager workaround below.

**Stage Manager used to hand over a tilted thumbnail, and that is gone with
xcap.** Kept because it explains why macOS window capture is Apple's job now: the
window server reported a parked window at the *tile's* geometry — Slack, really
1440×900, came back as 128×169 — and handed over the perspective-warped preview.
`screencapture -l<id>` returned the same skewed image, so nothing reachable
through `CGWindowListCreateImage` saw the real window. The old fix raised the
application first and waited, using `ActivateIgnoringOtherApps` beside
`ActivateAllWindows` because macOS lets one app activate another only when the
asking one is frontmost. If macOS window capture is ever reopened, start here.

**A tray menu that lists windows has to be rebuilt, and only Linux gets that for
free.** `ksni` asks for the menu each time one opens, so `window_items` is always
current. `tray-icon` hands the menu to the system, which shows it without telling
us — a list built at startup would name windows that closed hours ago. Windows
still needs this. macOS no longer does, since every entry in its menu is static,
but it runs the rebuild anyway: one path that is correct everywhere beats a `cfg`
that has to be re-reasoned about, and the rebuild is two cheap system calls.

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

**macOS: shotr is a menu-bar app in every process, and the Dock is why.** The
daemon has no window and would otherwise take a Dock icon and a menu bar. The
editor was left `Regular` on the reasoning that a window *wants* both — and it
does, but that is not all it buys. Becoming a regular app registers the bundle
with LaunchServices as one that was used, so the Dock's "recent applications"
section keeps the tile long after the process has gone. Measured on 26.5:
`defaults read com.apple.dock recent-apps` held `dev.shotr.app` with no shotr
process running, and since every capture is a *fresh process* the tile came
back after each one. It reads as the app failing to quit, which sends you
looking at the editor's exit path — where nothing is wrong, and `ps` says so.

So `app::native_options` asks for `Accessory` too, and every window goes
through it: editor, Preferences and pin alike. A test reads the source and
counts `run_native` calls against it, because a fourth window that builds its
own options still compiles and the only symptom is a tile that outlives the
process. Windows still come to the front and take the keyboard — winit's
`activate_ignoring_other_apps` defaults to true and runs straight after the
policy is set. What is given up is cmd+tab and the system menu bar, neither of
which was carrying much: the editor draws its own title bar and reads its own
shortcuts.

Setting it at runtime rather than with `LSUIElement` in `Info.plist` still
matters, even though that key would now say the same thing for the whole
bundle: it only applies to a bundle, and a `cargo run` build has none.

**One renderer for preview and export.** `Scene::scale` is what makes the
preview and the exported file identical code. Anything that draws must go
through `render/`, not be special-cased for one of the two. This is the main
reason not to move the UI to a webview.

**Reported monitor geometry is not in captured pixels, and the correction is
global.** On this compositor `xcap::Monitor` reports `10320x4320` for a screen
that captures at `3440x1440`; on macOS `CGDisplayBounds` gives logical points
against a backing-pixel capture. Either way the captured frame is the only honest
number, so `MonitorShot` carries the reported rectangle *uncorrected* and
`capture::layout` scales everything at once.

It has to be one shared factor, not one per monitor. Scaling each monitor by its
own ratio put a Retina laptop and a 1× panel in different coordinate spaces:
measured on a real Mac, an ultrawide physically 1.91× the laptop's width came out
at 0.96×, and side by side the two rectangles overlapped by 1800px. `S =
max(scale)` wins over `min` because upscaling the coarser monitor only makes it
soft while downscaling the finer one destroys detail that exists — at the cost of
a bigger canvas (6880×5218 against 4529×3778 on that machine), and `desktop_full`
is held for the whole session. A uniform-scale desktop resamples nothing, which is
what keeps Linux and Windows byte-identical; there is a test for exactly that.

**`xcap::Window::all()` returns nothing on Wayland.** It works on Windows.
`winlist` picks the implementation: `wl_windows` speaks
`ext_foreign_toplevel_list_v1` and `ext_image_copy_capture_v1` directly, and macOS
hands the whole question to `screencapture -i -W`.

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

**egui keeps two styles, and `set_global_style` only writes to one of them.**
There is a `dark_style` and a `light_style`, and `Options::theme_preference`
picks between them — defaulting to `System`. At startup the system theme has
not been delivered yet, so the style shotr installs lands in `dark_style`; the
moment the desktop reports itself light, egui switches to `light_style`, which
is stock egui and which nothing here has ever touched. The result is *half* a
theme: panels, canvas, folds and everything painted by hand stay dark, while
every button, text field and combo box turns white. It reads as a rendering
fault rather than a theme one, and on a Mac set to "Auto" it appears at sunrise
and goes away at sunset. `theme::apply` therefore pins
`ThemePreference::Dark` *before* it touches the style. shotr is dark on purpose
— the shot is meant to be the only bright thing on screen.

**`ScrollStyle::default()` is `floating()`, and half of it survives turning
`floating` off.** In particular `foreground_color: true`, which paints the
handle in `fg_stroke.color` — the *text* ink. So a solid scroll bar built by
taking the default and setting `floating = false` came out as a near-white
stripe down the edge of every panel, brighter than anything it sat beside, with
nothing in the field name to say why. `theme::apply` starts from
`ScrollStyle::solid()` instead, which puts the handle back on `bg_fill`, one
step off the panel. The width and the two margins are `theme::SCROLL_*`, because
the sidebar's swatch grid has to fit five columns into what is left and a test
checks it against those constants rather than against copies of them.

**A `ScrollArea` shrinks to its widest row, not to the pane.** `auto_shrink`
defaults to true on both axes, so the bar tracks the content — and a section
made of short rows put it 46px in from the edge, nowhere near the controls it
lines up with everywhere else. Preferences passes `auto_shrink([false, false])`
for exactly that reason. It reads as a scrollbar in the wrong place rather than
as a layout setting, which is what makes it hard to find.

**egui diverts ctrl+wheel.** With the zoom modifier held, `smooth_scroll_delta`
is zero and the wheel arrives as `zoom_delta()`. `zoom_with_keyboard` is turned
off in `theme::apply` so egui does not rescale the whole UI on ctrl+plus.

**`PREVIEW_MAX_W` is a texel budget, and egui lays out in points.** A fixed 1000
is one texel per point, which is right only at 1×. On a Retina Mac the canvas
asks for roughly twice that many device pixels, the GPU magnifies the shortfall,
and the editor goes visibly soft while the exported file stays pixel-perfect —
reported as "the image is blurry", which sends you looking at `render/` where
nothing is wrong. `fit_preview_to_display` multiplies the budget by
`pixels_per_point` and re-runs every frame, because dragging the window to a
display with another scale changes the answer. The cost is bounded: the whole
pipeline renders 8.6Mpx in ~100ms, so a 4× larger preview is still cheap.

**A keyboard shortcut needs the modifiers off the *event*, never off
`InputState::modifiers`.** That field is the state left at the *end* of the
frame, and a quick tap delivers the press and the release together — by the time
it is read the modifier is already gone, the frame reports `Modifiers::NONE`, and
the shortcut silently never fires. This is why `app::shortcut` scans
`i.events`. Note how it presented: a release build coalesced Ctrl+C into one
frame and did nothing, while a debug build was slow enough to split it over two
and worked perfectly. Identical source, opposite behaviour, no error either way —
it looked like a signing or install fault for an afternoon.

**Ctrl and ⌘ reach the editor by two different routes, and it needs both.**
`Modifiers::command` is ⌘ on macOS and Ctrl elsewhere, so matching it alone left
the physical Ctrl key doing nothing on a Mac. And ⌘C does not arrive as a key
press *at all*: egui turns the platform copy chord into `Event::Copy` and
delivers only that, plus a release once the chord is over. Hence `editor_modifier` accepting either
modifier, and `copy_requested` also watching for `Event::Copy`. Both were
measured by logging every event the editor received. Window shortcuts only; the
global capture hotkeys are spelled out in full (`Cmd+Shift+4`) and mean exactly
what they say.

That swallowing is wider than it looks: `egui_winit::is_copy_command` matches
`command && C` and **never looks at shift**, then returns before pushing the
press. So `{mod}+Shift+C` — "copy the shot as captured" — arrives as the very
same bare `Event::Copy` as `{mod}+C`, on every platform, carrying no modifiers
to tell them apart. `copy_requested` therefore falls back to the frame's own
shift state, which is the end-of-frame value this file warns about everywhere
else. It is sound only because shift in a deliberate three-key chord is held
across many frames, where the modifier in a quick `{mod}+C` tap is not; a real
key press, when one reaches us at all, is believed over it.

Which makes this the one shortcut a synthetic keystroke cannot test.
`osascript … keystroke "c" using {command down, shift down}` posts the whole
chord faster than egui runs a frame, so shift is already up by the time the
frame is read and the press takes the *unshifted* branch — measured: it copied
the beautified image and closed the window. Post the modifiers as separate
CGEvents and hold them for a few hundred milliseconds, the way a hand does, and
it takes the right one. A failure here means the test harness, not the code.

Accepting both keys is not licence to *name* both. Every label used to read
`Ctrl+…` on all three platforms, which on a Mac points at the key that is not
under the reader's thumb — reported as the Copy and Save buttons being wrong.
`app::MOD_LABEL` is the one place that decides, and `prefs_ui::about` fills a
`{mod}` placeholder from it. Spelled "Cmd", not "⌘": the same word `hotkey`
already writes into prefs.json, and no glyph to go missing from a system font.

**A texture set *after* it is painted still lands in the same frame, and the
editor depends on it.** The preview bitmap is rebuilt near the top of
`ShotrApp::ui`, but pointer input is handled much later, inside the central
panel — so a shape finished by that frame's mouse-up is in neither picture: the
draft that was drawing it has been taken, and the bitmap does not have it yet.
The annotation blinked out for exactly one frame on every mouse release.
`edit_central` therefore re-renders straight after handling input, which works
because `Painter::image` records only the texture *id* and egui uploads that
id's delta before it draws the frame's shapes. A test drives a real `Context` to
pin that, because an egui upgrade could take it away with no error anywhere.

**`ab_glyph`'s `PxScale` is not a font size.** `ScaleFont::scale_factor`
divides it by the font's *line height* — ascent − descent + line gap — while
egui, and everyone else who says "font size", divides by the em square. The
same number is therefore about 15% smaller through `render::text` than through
egui, measured on SF Pro with a real `Context`. Both engines draw labels here:
egui under the pointer and for the selection frame, `ab_glyph` into the picture.
So a label followed the mouse at one size, shrank the moment it settled, and sat
loose inside a frame that had fitted it a frame earlier — reported as the frame
being too big, which sends you looking at `selection_rect` where nothing is
wrong. `text::px_scale` converts, and a test lays the same string out both ways
and fails if they drift apart.

**The tool pill is one capsule that grows downward, and its width is fixed.**
The options belonging to a tool used to sit to the *right* of the buttons, so
the bar changed width whenever the tool changed and every button slid sideways
with it. They now live under a hairline inside the same capsule, which opens
downward when a tool is chosen. The closed state is exactly the tool row, which
only Select-with-nothing-selected ever reaches.

The capsule follows the row it is holding, and this is worth stating because the
first attempt got it backwards: it was sized to the *widest* row on the reasoning
that following the current one would put the buttons back to sliding about.
It does not. The capsule and the tool row are both centred on the canvas, so
they share a centre line — the capsule's *edges* move and the buttons do not.
Sizing to the widest row only left every short row swimming in empty glass,
which is what "the padding is uneven" meant. The inline padding also has to sit
*around* the row rather than inside its measured width, or the whole row lands
half the padding off centre.

The row's width is measured by `row_items` and drawn by `tool_options`, which
are different code. They have to be, because egui cannot build a row from data
when every control needs a different `&mut`. The backdrop is painted before the
contents, so a control added to one and not the other would show as a row
overflowing its own glass — hence the `debug_assert` comparing the two after
every layout. Nothing else catches it.

**A control drawn without something behind it is worse than a missing one.**
The options row's design specifies a font dropdown and B/I toggles. Those need a
font *file* per face registered in both egui and `ab_glyph` — one nominal size
already meant two different sizes to those two engines, and weight would be the
same trap again — so they are not drawn at all rather than drawn dead. Paint's
"Width" went the same way: it means a brush, and paint here is a rectangular
marker.

**A button is a group of forms, not a tool.** Seven buttons cover ten tools:
the shape key holds a rectangle, a rounded rectangle, an ellipse and a line,
and pressing it again steps between them. `GROUPS` is the whole vocabulary —
key, and the stops behind it — and the button's icon follows whichever stop is
in hand, because with four shapes on one key that icon is the only thing saying
what a press will draw.

The stops are read back off the dials rather than kept as an index, since the
options row can move the same dials and a remembered index would go stale the
moment it did. The remembered index is only where a button *returns* to when it
is reached from somewhere else, which is what makes a shared key cheap: coming
back to the ellipse you were just using costs one press, not three.

**A tool key pressed again steps through that tool's forms, and the ghost is
what says so.** The stops are read back off the dials rather than kept as an
index — the options row can move the same dials, and a remembered index would
go stale the moment it did. Shift steps back, so overshooting a four-stop cycle
costs one press rather than three. Every stop sets a *dial*, never the tool: a
key that quietly changed which tool was in hand would make the button under it
lie, and there is a test that says so.

The label sizes are a fraction of the shot's height rather than pixels,
because the same preset has to mean the same thing on a 1280px shot and a
3440px one. The options row still shows and edits pixels.

**The ghost is the only thing that reports a cycle step, so it cannot be a
third drawing path.** It goes through `paint_layer_preview` with a synthetic
layer built by `new_layer`, which is what stops it drifting from what the
exporter bakes. It carries no rim: a translucent white ring under translucent
ink turns the whole thing muddy, and the ghost's job is to say which shape and
what colour, not to be a faithful copy.

**Two selection marks, on purpose.** A shape with a silhouette of its own gets
traced — the arrow, whose outline is already computed — and everything else
gets the dashed box. Tracing is only worth it when there is an edge to follow,
and a blur has none. The traced arrow gets one handle at its tail rather than a
rotate knob, because one drag sets both its size and its direction; those are
the only two numbers it has.

**There are two arrows, and that is the point.** `Tool::Arrow` is one solid
silhouette with locked proportions — a drag says where it points and how big it
is, never how fat — which is the mark a marker pen makes. `Tool::Line` is the
stroked pointer shotr drew before, with a settable width and an open, solid or
dashed head. Neither replaces the other.

Three things follow from the arrow being a polygon. Its outline is *generated*
from one spine with a signed bend rather than digitised from an SVG: three
fixed forms need three outlines, and one bezier gives all three with the
straight one being bend zero — hand-listed points would be the same shapes with
a transcription error waiting in them. It is not turnable, because its
direction *is* its geometry and a rotate knob would be a second control for the
same thing. And it has no stroke slider, for the same reason.

The stand-in cannot draw it the easy way: epaint's `convex_polygon` means what
it says, and the arrow is concave at the notch where the head meets the shaft.
What saves it is that the outline is two walks of the same spine, so point `i`
pairs with `n-1-i` and the shape falls into quads that are each convex, share
edges, and leave no seam in one colour. The rim is that strip stroked
underneath — the seams are covered by the fill on top, leaving only the outer
boundary.

**Annotations are composited onto the finished canvas, not baked into the
screenshot.** They used to go on first, so Balance saw them and the frame
clipped them like real content — which made the picture's edge a wall you could
not draw past, and an arrow pointing *at* the shot from the background
impossible. `render_detailed` now draws them after the shot is placed, using the
same affine `Geometry::shot_to_canvas` describes.

Their coordinates stay in **shot pixels**, which is the part worth keeping: a
mark still travels with the picture when the padding or the ratio changes.
Pinning them to the canvas instead would let them reach the background too, and
would slide every one of them off its subject the moment the layout was
adjusted. Two tests hold both halves down.

Balance no longer sees them, which is right — where the subject sits is a
property of the shot, not of what was drawn on top of it.

**A freehand mark is a path, and it is the one layer that is not two corners.**
`bounds`, `centre`, `translate` and hit-testing all need a branch for it —
`b` never moves off `a`, so anything reading the corners measures a dot where
the stroke started. It rasterises as a union of capsules, one per segment, the
same way the dashed shaft does, which is what gives it round ends and round
joints for nothing. Points are only added when the pointer has actually moved:
a stationary press otherwise piles hundreds of identical points into the layer,
and every one is a segment the distance field walks per pixel.

**A badge carries its own number.** The counter is `max(existing) + 1` rather
than a running total beside the layers, so undoing a badge cannot make the next
one skip — and deleting the third of five leaves the gap where the reader can
see it, which is the honest answer.

**The rim and the shadow come off the same distance field, in three passes.**
A red arrow on a red part of the picture disappears, which is what the white
rim is for, and a rim with nothing under it reads as a sticker that has not
been cut out, which is what the shadow is for. Both fall out of `rasterise`
walking the box three times off one `sdf` — shadow, then rim, then ink. That is
what keeps them exactly concentric: a rim built by drawing the shape twice at
two widths drifts at the corners, where the two outlines are not parallel.

Two things follow. The shadow is *feathered*, not blurred — coverage falling
from 1 at the shape's edge to 0 at its reach — because a real gaussian would
mean an allocation and a convolution per layer, and the falloff is what the eye
reads anyway. And text cannot use any of it: a glyph here is blitted coverage
with no edge to offset, so its rim is eight offset copies. Four leaves visible
notches on a diagonal stroke; eight does not.

The vector stand-in draws the rim and **not** the shadow. egui cannot blur or
feather, and an unblurred approximation would be a fifth place for the stand-in
and the bake to disagree — this file already lists four.

**Nothing that follows the pointer may set `dirty`.** One preview render costs
275ms at the default look and up to 2s with a big shadow — measured with
`examples/render_demo` on an 8.6Mpx shot — so a gesture that re-bakes the bitmap
per frame is a slideshow, and turning a shape was exactly that. The way out is
the same one moving already used: `detached_layer` lifts the shape out of the
bitmap for the length of the gesture and the vector overlay draws it, so nothing
re-renders until the button comes up. `detached_wanted` is the single place that
decides, which is why the rule is easy to break — a new gesture has to be added
to it, and forgetting looks like a performance problem rather than a missing
line.

That overlay is a *stand-in*, and every tool it stands in for has to be drawn
there too. Rotation caught two that were not: the highlight went through
`rect_filled`, which cannot tilt, and a label through `Painter::text`, which has
no angle — both stayed upright under a dashed frame that turned. `TextShape` has
an `angle`, and it pivots about `pos`, which is where `Layer::centre` puts a
label's origin, so the two agree with no correction. And the stand-in must match
the final pixel for pixel: the paint tool was previewed at a fixed 35% alpha
while the renderer honours the layer's own, so every stroke faded in and then
jumped solid on release.

The arrow is the tool that shows this up worst, because *three* things have to
agree. `rasterise` unions three capsules, so every end and the joint at the tip
is round, while `line_segment` has butt caps and no joint — the arrow snapped
from soft to hard on mouse-up until the overlay grew a circle at each of the
four endpoints. And the head's minimum length was written `max(10.0)` in the
pixels being drawn into, which is 10 preview pixels in the preview and 10
export pixels in the export: the same arrow had two different heads, and no
overlay could have matched both. It is `max(10.0 * scale)` now, and a test
renders one arrow at two scales and fails if the head does not scale with it.

**Desktop entries need absolute paths.** The graphical session's `PATH` does not
include `~/.local/bin`, so a bare `Exec=shotr` resolves in a terminal and fails
silently from the launcher.

**Registering a global hotkey tells you nothing about whether it was free.**
`register` returns `Ok` for a combination another application already holds —
measured against `⌘⇧5` while macOS owned it and `⌘⇧4` while Shottr did. And when
two receivers hold one combination, **both fire**: one press, two actions, no
error anywhere. Apple's own shortcuts can be read from
`com.apple.symbolichotkeys`; nothing can enumerate a third party's. That is why
`hotkey` has no `is_available()`, and why the interface says "macOS is not using
this" rather than "free" — the difference is not pedantry, because on the
machine this was measured on `⌘⇧4` read as disabled in the plist while Shottr
held it. One more: a press delivers `Pressed` *and* `Released`, so a handler
that does not filter captures twice.

**Selection is invisible unless the editor says so, and it said the wrong
thing.** Clicking a shape with the Select tool selects it, dragging moves it and
Backspace deletes it — all of which worked, and none of which was discoverable:
the status line for Select advertised "double-click the image to copy and
close", the delete control had moved into the `⋯` menu, and the only feedback
was a thin outline. Reported as annotations not being selectable at all. The
line now describes selecting, and the tool bar grows a delete button while
something is selected — which is also the only confirmation that anything *is*
selected. Select sits on the key left of `1` rather than on `7`: it is returned
to far more often than any one drawing tool.

**The selection is a dashed box, and two cleverer versions were tried first.**
Tracing each shape's own silhouette worked for a rectangle and fell apart on an
arrow: the rails ran too close to the shaft and blended into one pink line,
while the head was left as loose strokes joined to nothing. Drawing a fatter
copy of the shape *under* the ink traces any silhouette for free — but it means
redrawing the ink on top, and the vector stand-in is not faithful for every
tool, so a selected Blur would show as a flat box instead of blurred pixels. A
dashed rectangle says "this one is selected" without pretending to be part of
the picture, and it is the same mark for every tool.

**Rotation was cheap because every shape is a distance field.** `rasterise`
samples an SDF per pixel, so turning a shape is nothing but turning the *sample
point* back into the shape's upright frame and widening the box that gets swept
— `rasterise_turned`. No shape needed geometry of its own rewritten, which is
the whole reason an arrow, a rectangle and an ellipse all learned to rotate in
one change.

Two do not go through it. **Paint** blends rather than stamping, so it sweeps
the turned box itself and tests each pixel against the upright rectangle.
**Text** blits glyphs: it renders onto a transparent scratch image and hands
that to `render::watermark::rotate`, the same rotation the wordmark uses.

That last one hides a trap worth knowing about. A label turns about the point it
was *placed* at, but `rotate` turns the scratch image about the *image's*
centre, so the result has to be shifted back by following where the origin
ended up. Get it wrong and the glyphs still tilt perfectly — they simply sit
somewhere else, which shows up only as the selection frame no longer agreeing
with the text inside it. A test pins the label's ink to a constant distance
from its anchor through a full turn.

**Blur and Fill deliberately cannot be turned.** They cover information rather
than decorate it, and there is nothing to gain from a tilted redaction box
against a good deal of code that could go wrong. `Layer::turnable` is the one
place that says so, and the editor offers no handle for them.

**`Layer::b` is unused for text, and that made labels almost unselectable.**
Every other annotation is two corners, so `Layer::bounds` can answer "what area
is this?" from the struct alone. A label is an origin and a string: `b` is left
equal to `a`, so `bounds` returned a square of `2 × font_size` around the point
first clicked — the *start* of the text. A 21-character label at size 34 is
~280px wide and only its first 68 could be clicked. Hit-testing therefore
measures the string instead, which is why it needs a `Painter` passed in.
`font_size` is already in shot pixels, so laying the text out at that size gives
an extent in the units the layer is stored in, with no conversion.

**Moving takes the layer out of the bitmap rather than re-rendering per frame.**
It used to translate only on mouse-up, so the outline travelled and the shape
jumped at the end. For the length of the drag the layer is dropped from the
preview — [`ShotrApp::layers_except`] — and drawn as a vector at the pointer:
two renders per gesture instead of one per frame, which was measured at ~7fps.
Note the index it skips counts *annotations*, not the combined list: redaction
boxes are prepended, so filtering the finished vector by the same number would
drop a redaction and leave the dragged shape behind.

That gesture is now refused on top of a shape. Poking at an annotation twice is
the most natural thing to try, and it used to copy and shut the window before
the outline could be noticed; on bare canvas it still copies and closes.

**A status message must expire, because it shares a line with the help.** The
status line carries both news ("Saved: …") and the explanation of the tool in
hand. `open_image` sets a message and nothing ever cleared it, so the help never
appeared for the whole session. Messages now hold the line for
[`STATUS_SECONDS`] and then hand it back; the expiry is driven by comparing the
text rather than by touching all twenty assignment sites.

**A capture that opens no window has to say so.** `--capture --copy` renders to
the clipboard and exits, which looks exactly like a hotkey that never fired —
and was diagnosed as one, twice, before the notification existed. `main::notify`
is not decoration: it is the only evidence the path produces, and without it the
feature cannot be told apart from a broken binding.

**Register global hotkeys before the winit loop starts, not from inside it.**
The status item is the opposite way round — macOS refuses one until
NSApplication is up, which is why `tray/native.rs` builds it in `resumed` — and
copying that habit for hotkeys is what broke them: created on the first tick
inside `about_to_wait`, every `register` returned `Ok` and no event ever
arrived. Moving the manager to `daemon::run`, before `tray::run` takes the main
thread, is what made presses land. Two orderings, two behaviours, no error
message in either — the only way to tell them apart is to press a key and watch.

Note the debugging trap underneath: a synthesised keystroke does not do it.
`osascript … keystroke` produced nothing even once the code was right, so a
press has to be a real one.

**Carbon hotkeys need no Accessibility grant, unlike an event tap.** Measured
from an `.app` with `AXIsProcessTrusted() = false`: the key still fired and no
prompt appeared. Screen recording remains the only permission macOS asks shotr
for. Measure this sort of thing from a bundle, never from a terminal — a bare
binary inherits the responsible process's grant and reports whatever the
terminal already had.

**That plist mixes two encodings.** In `com.apple.symbolichotkeys` the keycode
is a Carbon virtual code but the modifier mask is `NSEvent.ModifierFlags`
(shift `1<<17` … command `1<<20`). Third-party preferences use Carbon's own
constants instead — Shottr stores `⌘⇧` as `768`, where the plist stores
`1179648`. Reading one with the other's table yields a plausible wrong answer
rather than an error.

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
that cannot do it rather than the platform — that mistake has been made three
times here, with tray icons, with window listing, and with macOS capture, and all
three turned out to be possible.

The macOS one is worth reading as a method. Three rounds of reading binaries
(`nm -u` showing `NSWindow`, Swift class names like `RegionSelector`, a
`mach-register` entitlement) all pointed the wrong way; one `pgrep -x
screencapture` while the picker was open settled it in seconds. Prefer watching
the program run over inferring from what it links.

## Licence

GPL-3.0-only. `src/main.rs` carries the notice the licence asks each source
release to include; keep it there. Adding a dependency under a licence that
cannot be combined with the GPL — anything proprietary, or Apache-2.0 in the
GPLv2 direction — is a licensing decision, not a routine `cargo add`.

## Platform support

| | Linux | Windows | macOS |
|---|---|---|---|
| Region picker | shotr's own, over a pre-shot | shotr's own | `screencapture -i` |
| Screen capture | xcap | xcap | `screencapture -D N` |
| Window capture | Wayland protocols | xcap | `screencapture -i -W` |
| Window list in tray | yes | yes | no — the overlay lists them |
| Tray daemon | ksni (SNI over D-Bus) | tray-icon on winit | tray-icon on winit |
| Single instance | unix socket | named pipe | unix socket |
| Editor window controls | drawn: — ▢ ✕, right of the strip | same | Apple's lights, left of the strip |
| Editor window transparency | yes, given a compositor | yes | yes |
| Capture hotkey | the desktop binds `shotr --capture` | the desktop binds it | shotr binds it, in Preferences |
| Screen recording grant | — | — | TCC, per bundle |
| OCR (Vietnamese) | tesseract | tesseract if installed | tesseract if installed |

`cargo check --target x86_64-pc-windows-gnu` covers Windows; it needs
`mingw-w64`. Run it — `capture/xcap.rs` is `cfg`-ed out on macOS, so a syntax
error in it passes a host build and fails only there. That has already happened
once.

macOS cannot be cross-checked without Apple's SDK; CI builds it on a real runner.

**Permissions on macOS need a stable signature.** The linker's ad-hoc signature is
computed from the binary's bytes, so every rebuild is a different app and every
grant has to be given again. Sign with any identity — `SHOTR_SIGN_IDENTITY` — and
the grant survives rebuilds. See `packaging/README.md`.
