# edge-deck

A fullscreen touch app for a 2560x720 display. Built with Tauri v2, React, Vite and Tailwind, styled entirely with `@crawfordyoung/ui`.

Status: the window places itself on the 2560x720 display and the app follows OBS stream state. The setup rail and the Stream and Code setups are not built yet.

## Window placement

The window starts hidden and is placed before it is shown. If an attached display is exactly 2560x720 physical pixels (horizontal), the app opens borderless and fullscreen on it, wherever Windows positions that display; it is found by size only, never by index, name or position. If two displays match, the leftmost wins, then the topmost. With no such display it opens as a normal decorated window centred on the primary display. The choice is a pure function in `src-tauri/src/placement.rs`.

## OBS stream state

A background thread watches OBS over obs-websocket v5 on `127.0.0.1` so the shell can follow the stream. It takes the port and password from OBS's own settings file, `%APPDATA%\obs-studio\plugin_config\obs-websocket\config.json`, re-read before every connect attempt; edge-deck never stores, logs or emits the password. Turn the server on in OBS under Tools > WebSocket Server Settings. The webview reads `{ connection: "off" | "connecting" | "connected" | "auth-failed", streaming }` with the `obs_state` command and gets each change as an `obs-state` event (`obsState()` and `onObsState()` in `src/api.ts`; subscribe first, then read, so no change slips between). "off" means OBS is closed, its server is disabled or the file is missing; a dropped connection reads as not streaming. The watcher retries forever with a backoff capped at 10 seconds and stops when the app exits. The code is in `src-tauri/src/obs/`.

## Prerequisites

- Windows 11 with WebView2 (preinstalled on Windows 11)
- Rust stable, MSVC toolchain (`rust-toolchain.toml` pins the channel and components)
- Visual Studio Build Tools with the "Desktop development with C++" workload
- Bun 1.4.2+ (see `packageManager` in `package.json`); it is both package manager and runtime, no separate Node install needed

## Commands

```sh
bun install
bun run tauri dev       # run the app (Vite dev server on strict port 1421)
```

Gates (CI runs the same on `windows-latest`):

```sh
bun run typecheck
bun run lint
bun run test             # Vitest, 100% coverage thresholds
bun run build            # must run before cargo: Tauri embeds dist/
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run tauri build --debug --no-bundle   # proves the app compiles
```
