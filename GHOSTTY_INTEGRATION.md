# Ghostty Integration Notes

## Submodule Pin

This project vendors Ghostty as a git submodule at:

- Path: `vendor/ghostty`
- Remote: `https://github.com/ghostty-org/ghostty.git`
- Branch tracked: `main` (Ghostty does not currently use `master`)
- Pinned commit in this repo: `68646d6d22f29dafe31ecdc2ef3349ad92bd4e5b`

## Runtime Behavior

- The app includes a **macOS Ghostty embedding spike path**:
  - It extracts the native AppKit `NSView` from the Iced window.
  - It initializes Ghostty runtime/app/surface through the C API.
  - It binds the Ghostty surface directly to the host `NSView`.
- Ghostty is now **statically linked** into this app build (no runtime `dlopen`).
- If Ghostty init fails at runtime, the app falls back to the embedded
  `iced_term` terminal widget so the app remains usable.

## Static Link Source

`build.rs` links this static archive by default:

- `vendor/ghostty/macos/GhosttyKit.xcframework/macos-arm64/libghostty-fat.a`

Override path if needed:

- `GHOSTTY_STATIC_LIB=/absolute/path/to/libghostty-fat.a`

If the archive is missing, `build.rs` automatically runs:

```bash
cd vendor/ghostty
zig build -Dapp-runtime=none -Demit-xcframework=true -Dxcframework-target=native -Doptimize=ReleaseFast
```

## Why this shape

Ghostty provides an embedding C API, but it is not currently documented as a
general-purpose stable API. Wiring a true Ghostty-rendered surface inside Iced
still requires platform-specific native view integration and a dedicated FFI
layer on top of `ghostty.h`.

The current setup therefore:

- keeps Ghostty pinned in-tree as a submodule,
- provides an actual Ghostty API integration spike for macOS,
- statically links Ghostty at build time for macOS-only usage,
- keeps `iced_term` fallback while Ghostty embed is still experimental.
