# CLAUDE.md — edge-deck

Apps-domain rules: `~/code/apps/CLAUDE.md`. A fullscreen touch app for a 2560x720 display (Tauri v2 + React).

## Gates

`bun run typecheck`, `bun run lint`, `bun run test` (100% coverage), `bun run build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `bun run tauri build --debug --no-bundle`. Run `bun run build` before any cargo step: `generate_context!` embeds `dist/`.

## Rules

- Package manager and runtime is Bun (1.4.2, pinned in `packageManager`). Never npm, yarn or pnpm. `bunfig.toml` sets `[run] bun = true`, so no Node needed.
- TypeScript is pinned `~6.0.3` because typescript-eslint's peer range is `<6.1.0`. Don't bump it until typescript-eslint widens.
- Only the dark theme exists (`class="dark"` on `index.html`).
- Dev server port 1421 is strict (`vite.config.ts` and Tauri `devUrl`), so it runs beside strate's 1420; don't let it fall back.

## Traps

- This repo is PUBLIC. Fixtures are synthetic only (placeholder values, no real data or paths). Tokens go in the OS credential store, never in files, env examples or the repo.
- UI comes from `@crawfordyoung/ui`; new pieces land in the library first.
- The target display is 2560x720 touch. Touch targets are at least 96px, and tests find them by size, never by index or position.
