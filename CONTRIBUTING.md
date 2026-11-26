# Contributing to Liqlip

Thanks for helping. Liqlip is a clipboard manager and screenshot tool for Windows: capturing,
annotating, finding and pasting what you copied or captured. Changes that make that faster, more
reliable or more private are welcome. Dictation, mockup/studio editors, sync and AI are out of scope.

## Before you start

- **Bugs:** open an [issue](https://github.com/prodbyEDDY/liqlip/issues/new/choose) with your
  Windows version, the steps and what you expected.
- **Larger changes:** open an issue first so we can agree on the approach before you write the
  code.
- **Security problems:** don't open a public issue; follow [SECURITY.md](SECURITY.md).

## Development setup

You need Windows 10 or 11, Node.js 24+, pnpm 12+, Rust 1.90+ (MSVC) and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm dev          # UI only, in a browser, with a mock backend: http://localhost:1430/picker.html
pnpm tauri dev    # the real app
```

The browser preview is the quickest way to work on the UI. Every IPC command has an in-memory
mock in `src/lib/ipc/mock/`, and URL flags such as `?mock-empty=1`, `?mock-error=1` and
`?mock-position=top` switch states (full list in `src/lib/ipc/mock/bus.ts`).

## Checks

Run these before opening a pull request. CI runs the same ones on `windows-latest`.

```bash
pnpm check                                        # must be 0 errors, 0 warnings
pnpm test                                         # node:test, incl. the IPC contract guard
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml   # close the app first: it locks the binary
```

Add a test for every logic change. Rust unit tests live next to the code (`#[cfg(test)]`), and
TypeScript tests live in `tests/*.test.mjs`.

## House rules

[`CLAUDE.md`](CLAUDE.md) is the full rulebook; these are the rules that matter most in review:

**Behaviour**
- **Never type into the wrong window.** Before `SendInput`, confirm the target is in the
  foreground and release held modifier keys. If anything fails, copy the item and show the HUD;
  never type.
- **Honour privacy.** Respect the Windows privacy markers. Logs must never contain clipboard
  content.
- **Keep clipboard access short.** One `OpenClipboard` per snapshot or write, closed as early as
  possible, with retries and read timeouts.
- **Low-level hook callbacks do no work**: no I/O, no IPC, no windows.
- **Create windows up front.** Windows are created hidden in `setup()` and only shown or hidden
  afterwards.

**Interface**
- **Translate every string.** No hard-coded UI text: add each key to both `src/lib/i18n/en.ts`
  and `ru.ts` (the compiler enforces parity). Native strings go to
  `src-tauri/src/i18n_native.rs`.
- **Match shortcuts by physical key** (`KeyboardEvent.code`, VK codes) so they work in any
  keyboard layout.
- **Follow the design system** ([`docs/design-system.md`](docs/design-system.md)). The palette is
  monochrome, with color only in user content. No borders or dividers: separate by tone and
  space. Nested corner radius = outer radius − gap, computed from tokens. Nothing may clip
  shadows, focus rings or text. Every interactive state needs a design.

**Code**
- **Change the IPC contract in both places at once:** `src-tauri/src/model.rs` and
  `src/lib/ipc/types.ts`, plus the mock. Add to it; don't break it.
- **Keep docs in step with code.** When behaviour changes, update the matching section of
  `docs/ARCHITECTURE.md`. The docs are in Russian; English notes in the PR are fine, and the
  maintainer will carry them over.
- **Keep changes small and surgical.** No drive-by refactors, no speculative abstractions.

## Commits and pull requests

- Use short imperative messages with a prefix: `fix: …`, `feat: …`, `docs: …`, `test: …`.
- One topic per pull request. In the description, say what changed, why, and how you checked it.
  For UI changes, add screenshots in both themes.
- Code, comments and commit messages are in English.

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
