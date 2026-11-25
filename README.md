# Liqlip

A keyboard-first clipboard manager for Windows. Everything you copy — text, links, code,
colors, contacts, images, files — is kept locally and is one `Alt + V` away: a liquid,
monochrome shelf of cards with full-text search and filter tokens, Quick Look, an actions
menu, multi-select, text transforms, folders, favorites, snippets, a paste stack and OCR on
images. Password-manager copies are never stored.

Built with Tauri 2 (Rust) and Svelte 5. Extracted from [Liquid Tools](https://github.com/prodbyEDDY/liquid-tools)
as a standalone, clipboard-only app.

**Status:** in development. The foundation is in place — project skeleton, design system,
the full IPC contract with a stub for every command, settings, tray, the three windows and an
in-browser mock backend. Capture, storage, paste, the shelf and the settings UI are the next
wave. See [`docs/ROADMAP.md`](docs/ROADMAP.md) (Russian).

## Docs

- [`docs/SPEC.md`](docs/SPEC.md) — product spec
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — technical design, IPC contract (§9) and
  inter-module APIs (§15)
- [`CLAUDE.md`](CLAUDE.md) — rules for contributors and coding agents

## Run

Requirements: Node 24+, pnpm 12+, Rust stable (1.90+), Windows 10/11 with WebView2.

```bash
pnpm install
pnpm dev          # UI in a normal browser with a mock backend:
                  #   http://localhost:1430/picker.html · /settings.html · /hud.html?kind=copied
pnpm check        # svelte-check (0 errors, 0 warnings)
pnpm test         # node:test suite (contract, mock, hotkey recorder)
pnpm build        # svelte-check + production frontend build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml
pnpm tauri dev    # the native app
```

Mock flags for the browser preview: `?mock-empty=1`, `?mock-error=1`, `?mock-live=0`,
`?mock-size=5000`, `?mock-target=chrome|none|elevated`, `?mock-unimpl=ocr,import`.
