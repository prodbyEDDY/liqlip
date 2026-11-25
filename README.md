# Liqlip

A keyboard-first clipboard manager for Windows. Everything you copy — text, links, code,
colors, contacts, images, files — is kept locally and is one `Alt + V` away: a liquid,
monochrome shelf of cards with full-text search and filter tokens, Quick Look, an actions
menu, multi-select, text transforms, folders, favorites, snippets, a paste stack and OCR on
images. Password-manager copies are never stored.

Built with Tauri 2 (Rust) and Svelte 5. Extracted from [Liquid Tools](https://github.com/prodbyEDDY/liquid-tools)
as a standalone, clipboard-only app.

**Status:** in development — planning done, foundation wave next. See
[`docs/ROADMAP.md`](docs/ROADMAP.md) (Russian).

## Docs

- [`docs/SPEC.md`](docs/SPEC.md) — product spec
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — technical design and IPC contract
- [`CLAUDE.md`](CLAUDE.md) — rules for contributors and coding agents

## Run (once the foundation lands)

```bash
pnpm install
pnpm dev          # UI in a normal browser on http://localhost:1430 with a mock backend
pnpm check        # svelte-check
pnpm test         # node tests
pnpm build        # production frontend build
cargo check --manifest-path src-tauri/Cargo.toml
pnpm tauri dev    # the native app
```
