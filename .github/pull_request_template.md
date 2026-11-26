## What and why

<!-- What changes, and the problem it solves. Link the issue: "Fixes #123". -->

## How I checked it

- [ ] `pnpm check` — 0 errors, 0 warnings
- [ ] `pnpm test`
- [ ] `pnpm build`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml`
- [ ] Tried it in the app (`pnpm tauri dev`) or in the browser preview (`pnpm dev`)

## Checklist

- [ ] New UI strings are in both `en.ts` and `ru.ts`
- [ ] UI changes follow `docs/design-system.md`; screenshots in light and dark themes are attached
- [ ] IPC contract changes are in both `model.rs` and `types.ts`, plus the mock
- [ ] `docs/ARCHITECTURE.md` is updated if behaviour changed
