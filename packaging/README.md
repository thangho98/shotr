# Packaging

One script per platform. Each writes into `dist/` and each is safe to run
repeatedly.

| Script | Runs on | Produces |
|---|---|---|
| `build-linux.sh` | Linux | `shotr-VERSION-linux-ARCH.tar.gz` (binary + `install.sh`) |
| `build-windows.sh` | Windows, or Linux with mingw-w64 | `.zip`, plus `setup.exe` when `makensis` is present |
| `build-macos.sh` | macOS only | universal `.app` inside a `.dmg` |
| `install-macos.sh` | macOS only | copies that `.app` into `/Applications` |

## Why these formats

**Linux: a tarball, not an AppImage.** shotr links only libraries any desktop
already has, so bundling a runtime would add tens of megabytes and a FUSE
dependency to solve a problem it does not have. `install.sh` inside the tarball
places the binary, icons and desktop entry under `~/.local`, no root needed.

**Windows: zip first, installer second.** The zip is the artefact that always
builds; NSIS is optional so a missing `makensis` cannot fail a release. The
installer is per-user (`RequestExecutionLevel user`) — screen capture needs no
elevation, and asking for it would be a bad look for a screenshot tool.

**macOS: universal binary.** Two `cargo build`s joined with `lipo`, so one
download covers Intel and Apple Silicon. `Info.plist` carries
`NSScreenCaptureUsageDescription`; without it the permission prompt appears
with no explanation and people deny it.

## Cross-compiling

Windows can be built from Linux — that is how it is checked during development:

```bash
sudo pacman -S mingw-w64-gcc          # or your distro's equivalent
cargo check --target x86_64-pc-windows-gnu
```

macOS cannot. It needs Apple's SDK, which is not redistributable, so CI builds
it on a real macOS runner. Do not add an `osxcross` step.

## Signing

Neither the Windows nor the macOS artefact is signed. Both systems will warn on
first launch. Signing needs paid certificates:

- **macOS** — `codesign --deep --force --sign "Developer ID Application: NAME"`,
  then `xcrun notarytool submit` for notarisation.
- **Windows** — `signtool sign /fd sha256 /a shotr-setup.exe`.

Until then the honest thing is to say so in the release notes rather than let
users hit a scary dialog with no warning.

### Developing on macOS: sign anyway, or fight TCC forever

A paid certificate is needed to *ship*. Keeping macOS permissions while you work
needs only a free self-signed one, and without it the loop is maddening: macOS
records a Screen Recording or Files-and-Folders grant against the code
signature, the linker's ad-hoc signature is computed from the binary's own
bytes, so **every rebuild is a different app** and every permission has to be
granted again. Granting it harder does not help — it was never the same app
twice.

Make the certificate once, in Keychain Access → Certificate Assistant → Create a
Certificate: any name, *Self Signed Root*, certificate type **Code Signing**.
Then:

```bash
export SHOTR_SIGN_IDENTITY="shotr-dev"   # whatever you named it
bash packaging/build-macos.sh
```

`build-macos.sh` signs with it when that variable is set and stays ad-hoc when
it is not, so CI is unaffected. If permissions were already granted to the
ad-hoc builds, clear the stale entries once with
`tccutil reset ScreenCapture dev.shotr.app` before granting again.

**Then quit shotr from its own menu, and only then start it again.** macOS
decides Screen Recording once per process and caches the answer for that
process's lifetime, so a daemon that was already running when the switch was
flipped stays denied — and since every shot is a *child* of that daemon, every
shot re-asks. The permission panel lists shotr as allowed throughout, which
makes it look as though the grant is being ignored. It is not: the running
process predates it.

Quitting is the part that catches people, and it caught us:

- **Opening the app again does not restart it.** The second launch hands its
  request to the daemon over the socket and exits, so the old, denied process is
  still the one taking your screenshots. Nothing on screen says so.
- **The daemon is `Accessory`,** so it has no Dock icon to quit, does not appear
  in cmd+tab, and Force Quit does not list it.

Its tray menu's *Quit* is the only way out, or `pkill -f MacOS/shotr` from a
terminal. Grant the permission, quit it *properly*, start it again — in that
order, or the grant never reaches a process that can use it.
