# AGENTS.md

## Purpose
This repository embeds Ghostty into an Iced app on macOS. Use this file as the default research workflow so changes are grounded in source-of-truth APIs instead of guesswork.

## Research Order (Always Follow)
1. Local repo code first (`src/`, `build.rs`, existing docs).
2. Vendored Ghostty source next (`vendor/ghostty/`) for exact ABI and behavior.
3. Installed crate source for Iced (`~/.cargo/registry/src/.../iced_*`).
4. Apple/AppKit official docs for Cocoa behavior.
5. Web search only for missing context, and prefer official sources.

## Fast Commands
- List project files: `rg --files`
- Find symbols quickly: `rg -n "symbol_name" src vendor`
- Inspect Ghostty C API: `rg -n "ghostty_surface_" vendor/ghostty/include/ghostty.h`
- Inspect Ghostty embedded implementation: `rg -n "export fn ghostty_surface_" vendor/ghostty/src/apprt/embedded.zig`
- Inspect Ghostty runtime behavior: `rg -n "cursorPosCallback|scrollCallback|mouseButtonCallback" vendor/ghostty/src/Surface.zig`
- Inspect Iced event types: `rg -n "pub enum Event" ~/.cargo/registry/src/index.crates.io-*/iced_core-*/src`

## Ghostty FFI Research Playbook
- C header is ABI source of truth:
  - `vendor/ghostty/include/ghostty.h`
- For each C symbol, verify implementation in Zig:
  - `vendor/ghostty/src/apprt/embedded.zig`
- For runtime behavior details (scrolling, focus, cursor semantics), verify surface callbacks:
  - `vendor/ghostty/src/Surface.zig`
- For macOS-specific behavior, check Ghostty macOS implementation under `vendor/ghostty/src/apprt/` and related macOS files.
- Do not invent enum values or bitmasks. Copy from header and verify in implementation.
- Confirm sign conventions explicitly (example: scroll up/down directions).
- Confirm special sentinels explicitly (example: negative cursor coordinates mean outside viewport).

## Iced Research Playbook
- Check exact event/enum definitions in the installed crate source before wiring input.
- Relevant files usually include:
  - `iced_core-*/src/event.rs`
  - `iced_core-*/src/mouse/event.rs`
  - `iced_core-*/src/window/event.rs`
  - `iced_core-*/src/keyboard/event.rs`
- Verify data space for all coordinates:
  - Iced pointer positions are logical units.
  - Ghostty surface input expects pixel space for embedded view.
- Always handle:
  - `CursorMoved`
  - `CursorLeft`
  - `ButtonPressed` and `ButtonReleased`
  - `WheelScrolled` with line vs pixel deltas

## Cocoa/AppKit Research Playbook
- Local bridge/shim source is first stop:
  - `src/ghostty_runtime_shim.m`
- Validate parent and child `NSView` frame management:
  - `host_view_set_frame`
  - `host_view_set_hidden`
  - resize propagation after window resize/rescale events
- Verify focus routing and first-responder implications when keyboard input appears broken.
- When behavior is unclear, confirm against Apple docs for `NSView`, event handling, and coordinate systems.

## Elm Interop Research Playbook
- Elm does not support arbitrary native FFI in Elm code.
- Correct pattern is:
  - Elm <-> JavaScript via ports.
  - JavaScript <-> Rust (native process API, HTTP/WebSocket, or WASM boundary).
- If using Elm + Rust/WASM:
  - Validate message contracts at the port boundary.
  - Keep terminal rendering/input logic in JS/native layer, not Elm runtime internals.
- Source of truth for Elm interop semantics: official Elm guide/docs.

## Debugging Checklist for Input Bugs
1. Confirm window focus and active session index.
2. Confirm cursor-in-terminal hit testing logic.
3. Confirm logical-to-pixel conversion uses current scale factor.
4. Confirm modifier state is updated and passed on mouse and keyboard paths.
5. Confirm Ghostty receives negative cursor position when cursor leaves viewport.
6. Confirm wheel event path distinguishes line vs pixel precision.
7. Verify with real apps (`vim`, `nvim`, shell editing, mouse selection, scrollback).

## Validation Before Commit
- `cargo fmt --all`
- `cargo check`
- `cargo build`
- Manual sanity run:
  - Type fast and edit command lines.
  - Backspace/delete/home/end.
  - Shift-modified symbols (like `:` in `nvim`).
  - Scroll behavior and resize behavior.

## Rules for Future Changes
- Prefer minimal, traceable fixes.
- Put every external API assumption next to a source reference (file path and symbol).
- Do not proceed on uncertain behavior; trace it in source first.
