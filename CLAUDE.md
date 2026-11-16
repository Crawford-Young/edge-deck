# CLAUDE.md — edge-deck

Apps-domain rules: `~/code/apps/CLAUDE.md`. A fullscreen touch app for a 2560x720 display (Tauri v2 + React).

## Gates

`bun run typecheck`, `bun run lint`, `bun run test` (100% coverage), `bun run build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `bun run tauri build --debug --no-bundle`. Run `bun run build` before any cargo step: `generate_context!` embeds `dist/`.

## Rules

- Package manager and runtime is Bun (1.4.2, pinned in `packageManager`). Never npm, yarn or pnpm. `bunfig.toml` sets `[run] bun = true`, so no Node needed.
- TypeScript is pinned `~6.0.3` because typescript-eslint's peer range is `<6.1.0`. Don't bump it until typescript-eslint widens.
- Only the dark theme exists (`class="dark"` on `index.html`).
- Dev server port 1421 is strict (`vite.config.ts` and Tauri `devUrl`), so it runs beside strate's 1420; don't let it fall back.

## Architecture

- The Code setup's data is strate-core's ingest engine run in-process (`src-tauri/src/agents/`, its own store, never strate's). It emits `code-changed`; the webview re-reads the `code_snapshot` command (`src/code/useCodeSnapshot.ts`: subscribe first, then read, no timers). strate-core is a git dep pinned by `rev`.
- The agent web is `src/code/graph/` (React Flow `@xyflow/react` + `d3-force` layout, one island per session). Look and tap rules are pure functions in `graph/rules.ts` (dense past `DENSE_NODE_COUNT`, hubs, `MIN_TAP_PX` 96); nodes are the library's `AgentNodeRing` (`@crawfordyoung/ui` 0.34). A tap on a node smaller than 96px on screen zooms to it; a tappable node calls `focusTerminal`. Fits frame cluster targets (nodes plus label footprint, `layout.ts`), padded at the top to clear the Re-fit button, and re-run after a layout change until the user moves the view (Re-fit re-arms it). A background wait with a `bg_id` and no reason reads "blocked in the background" (`snapshot.rs`).
- Shared types (`FocusTarget`, `SessionKind`, snapshot shapes) live in `src/code/types.ts`; `src/api.ts` re-exports them. Keep `types.ts` free of imports from `api.ts` (no cycle).

## Traps

- This repo is PUBLIC. Fixtures are synthetic only (placeholder values, no real data or paths). Tokens go in the OS credential store, never in files, env examples or the repo.
- UI comes from `@crawfordyoung/ui`; new pieces land in the library first.
- The target display is 2560x720 touch. Touch targets are at least 96px, and tests find them by size, never by index or position.
