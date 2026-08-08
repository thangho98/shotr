# Packaging

One script per platform. Each writes into `dist/` and each is safe to run
repeatedly.

| Script | Runs on | Produces |
|---|---|---|
| `build-linux.sh` | Linux | `shotr-VERSION-linux-ARCH.tar.gz` (binary + `install.sh`) |
| `build-windows.sh` | Windows, or Linux with mingw-w64 | `.zip`, plus `setup.exe` when `makensis` is present |
| `build-macos.sh` | macOS only | universal `.app` inside a `.dmg` |

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
