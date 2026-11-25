# Liqlip — guidelines (CLAUDE.md)

Windows clipboard manager extracted from Liquid Tools into its own app: keyboard-first shelf
(`Alt + V`), full-text search with filter tokens, Quick Look, actions menu, multi-select,
text transforms, folders, favorites, snippets, paste stack, OCR on images, privacy by default.
**Clipboard only** — no dictation, no screenshots, no studio. Same liquid monochrome look as the
Liquid Tools shelf.

Lives in the workspace `C:\Coding\My Projects\LiquidTools\` next to `liquid-tools-app` (the
source we port from — **read-only for this project**). The workspace root is not a git repo.
Repo: `github.com/prodbyEDDY/liqlip` (private), branch `main`.

## Sources of truth — read before coding
- [`docs/SPEC.md`](docs/SPEC.md) — what we build: every surface, key map, action, setting, state.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — how: windows, setup order, SQLite schema,
  capture/paste/picker/hotkey design, **the IPC contract (§9)**, file ownership (§13),
  port map from Liquid Tools (§14).
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — status and waves (§10) + decision journal (§11).
  **Current status lives ONLY there.**
- Your task prompt in `docs/prompts/`.

**doc-as-code:** when you change code, update the matching section of `ARCHITECTURE.md`
(or the area doc you were told to keep) in the same change. Status/decisions go to
`ROADMAP.md` through the coordinator (`/lead`, [`.claude/commands/lead.md`](.claude/commands/lead.md)).

## Working principles (non-negotiable)
1. **Code only from verified documentation.** Never assume an API, crate signature, Win32
   behaviour or config key — check official docs (docs.rs, learn.microsoft.com, v2.tauri.app,
   svelte.dev) first. Accuracy > speed.
2. **Every scenario, every error path.** A clipboard manager is judged on edge cases:
   locked clipboard, hung source app, elevated target, huge items, rapid copies, Russian layout.
3. **Full state coverage** for every interactive component: `default · hover · focus-visible ·
   active · disabled · selected · loading(skeleton) · empty · error`. Loading = skeleton, never
   a centred spinner. Empty states teach.
4. **No AI slop.** No purple/blue gradients, glassmorphism, neon, glowing orbs, generic card grids.
5. Bilingual **EN + RU** — never hardcode a display string; every label is an i18n key.
6. **Simplicity and surgical changes.** Minimum code that solves the task; no speculative
   abstractions; touch only files you own.

## Stack
Tauri 2 (Rust) · Svelte 5 (runes) · Vite MPA (one HTML entry per window: `picker`, `settings`,
`hud`) · Tailwind v4 · TypeScript · Heroicons via local `src/lib/heroicons/` + `<Ico>` · Inter +
JetBrains Mono (`@fontsource-variable/*`). Rust: `rusqlite` (bundled, FTS5), `windows` 0.62
(Win32/WinRT), `image` (png), `serde`, `parking_lot`, `base64`, `tokio` (sync). Input is
`SendInput` directly (no `enigo`). Plugins: `single-instance`, `autostart`, `dialog`, `opener`.
Identifier `com.liqlip.app`; data in `%APPDATA%\com.liqlip.app\`.

## Commands
- `pnpm dev` — Vite only, port **1430** strict; without Tauri the IPC layer falls back to the
  in-memory mock (`src/lib/ipc/mock/`), so the shelf and settings work in a normal browser.
- `pnpm check` — svelte-check (must be 0 errors, 0 warnings).
- `pnpm test` — node:test suite (`node --test "tests/*.test.mjs"`), incl. the contract guard
  (§9.6 ≡ `lib.rs` ≡ `commands.ts` ≡ mock; `model.rs` ≡ `types.ts` field by field).
- `pnpm build` — svelte-check + vite build (the only thing that resolves icon imports).
- `cargo check --manifest-path src-tauri/Cargo.toml` — Rust gate (the app must not be running).
- `cargo test --manifest-path src-tauri/Cargo.toml` — works in this crate (W0, 2026-11-26): put
  `#[cfg(test)]` unit tests next to the code (ARCHITECTURE §12).
- `pnpm tauri dev` — the native app. **Wave agents do not run it**; the coordinator runs it at
  integration (INT-*) for end-to-end checks (ROADMAP PQ-02, 2026-11-26).

## Design system
Source of truth: [`docs/design-system.md`](docs/design-system.md) (W2-E, 2026-11-26; evolved from
Liquid Tools after the W1 design review) and the audit [`docs/design-audit.md`](docs/design-audit.md).
- **Color:** monochrome, LOCKED. Only tokens from `src/app.css` exist — surfaces `ground tile
  raised card card-hover card-lift scrim thumb` (+ `paper page content panel-2`), ink
  `ink ink-2 ink-3 ink-hover ink-active danger checker`, ink washes `track hover wash-sel selected
  toggle-track`. Control fills are ink washes over their surface. Accent **is** `ink`. Color
  appears only in user content (the ColorCard swatch). Both themes strictly black/white.
- **Shape:** surfaces are squircles (CSS `corner-shape: squircle`, the `squircle` utility);
  controls (buttons, fields, dropdowns, kbd, chips, pills) are capsules — plain
  `border-radius: 999px`, never `squircle`. **Nested radius = outer radius − gap**, computed from
  the `--r-*` / `--pad-*` tokens with `calc()`, never eyeballed.
- **No borders, dividers or rings** — separation by tone, elevation and space. Selection and the
  current card are shown by tone + lift. The only outline is `:focus-visible` (`--focus-ring`,
  following the element's shape). Nothing clips shadows, focus rings, lifts or text; scrolling
  edges fade with a mask.
- **Type:** Inter for UI; JetBrains Mono only for functional mono (kbd, code, hex, numbers);
  labels ≥ 12 px, `--text-micro` 11 px only for service marks.
- **Motion:** 150–250 ms ease-out; liquid easing `cubic-bezier(.34,1.24,.44,1)`; honour
  `prefers-reduced-motion`. Motion conveys state, not decoration.
- Shelf layers are built from `src/windows/picker/overlays/kit/` (`LayerSurface`, `LayerHeader`,
  `LayerItem`, `LayerFooter`) — see `docs/picker.md` §5.1.

## Hard rules
1. **`CLAUDE.md` and `AGENTS.md` are edited in pairs** — `AGENTS.md` is a byte-for-byte copy
   differing only in the title line. Line endings are **LF** everywhere (`.gitattributes`).
2. **Windows are pre-created hidden in `setup()`** and only shown/hidden later — never build a
   webview window lazily from a command or hotkey thread (deadlocks the event loop on Windows).
   Managed state (`Store`, settings) must exist before the first window is built.
   `CloseRequested` is intercepted on every window (hide, never destroy).
3. **Low-level hook callbacks do no work** — no IPC, no windows, no I/O; update state, decide
   whether to swallow, schedule, return. Windows silently removes a slow hook.
4. **Clipboard access is short and retried.** One `OpenClipboard` per snapshot/write, closed
   as early as possible; retries with backoff; per-format read timeout (delayed rendering).
5. **Privacy markers are honoured** (`ExcludeClipboardContentFromMonitorProcessing`,
   `CanIncludeInClipboardHistory`=0, `CanUploadToCloudClipboard`=0, `Clipboard Viewer Ignore`).
   Logs never contain clipboard content.
6. **Never type into the wrong window.** Before `SendInput`, the target must be confirmed
   foreground; held modifiers are released first; elevated targets get the copy-only path.
   Never change system-wide settings (e.g. `SPI_SETFOREGROUNDLOCKTIMEOUT`).
7. **Hub files belong to the coordinator:** `src-tauri/src/lib.rs`, `tauri.conf.json`,
   `capabilities/*.json`, the contract (`src-tauri/src/model.rs` + `src/lib/ipc/types.ts`),
   `src/lib/i18n/{en,ru}.ts` (append only your own `<prefix>.*` block, in both files, same
   order), `src/app.css`. Contract changes: additive only, in both files, reported.
8. **i18n parity is compiler-enforced:** `ru.ts` is typed `Record<TranslationKey, string>`.
   Native strings (tray) come from `src-tauri/src/i18n_native.rs`, EN + RU.
9. **Keyboard shortcuts match physical keys** (`KeyboardEvent.code`, VK codes in Rust) so they
   work in the Russian layout.
10. **Liquid Tools is read-only.** Port by reading `..\liquid-tools-app\`; never edit, build or
    commit there (its working tree has uncommitted work).
11. **Commits only on the coordinator's request.** Wave agents work in their own git
    worktree/branch and do not merge into `main`.
12. **Language:** `docs/` and coordination are Russian; code, comments, `CLAUDE.md` and
    `README.md` are English; product strings are EN + RU keys.

## Key files

| Zone | Files | Doc |
|---|---|---|
| Contract | `src-tauri/src/model.rs` · `src/lib/ipc/{types,commands,events}.ts` · `src/lib/ipc/mock/` | ARCHITECTURE §9–10 |
| Wiring hub | `src-tauri/src/lib.rs` · `tauri.conf.json` · `capabilities/default.json` | ARCHITECTURE §2–3 |
| Store (SQLite) | `src-tauri/src/store/` | ARCHITECTURE §4 |
| Capture | `src-tauri/src/capture/` | ARCHITECTURE §5 |
| Paste / stack / drag-out | `src-tauri/src/paste/` | ARCHITECTURE §6 |
| Picker window / HUD | `src-tauri/src/picker/` | ARCHITECTURE §7 |
| Hotkeys | `src-tauri/src/hotkeys/` | ARCHITECTURE §8 |
| Settings | `src-tauri/src/settings.rs` · `src/windows/settings/` | SPEC §4 |
| Shelf UI | `src/windows/picker/` | SPEC §3 |
| Design tokens | `src/app.css` | Liquid Tools `docs/design-system.md` |
| Tests | `tests/*.test.mjs` | ARCHITECTURE §12 |
