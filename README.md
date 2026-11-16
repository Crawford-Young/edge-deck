# edge-deck

A fullscreen touch app for a 2560x720 display. Built with Tauri v2, React, Vite and Tailwind, styled entirely with `@crawfordyoung/ui`.

Status: scaffold only. A placeholder heading, no shell features yet.

## Window placement

The window starts hidden and is placed before it is shown. If an attached display is exactly 2560x720 physical pixels (horizontal), the app opens borderless and fullscreen on it, wherever Windows positions that display; it is found by size only, never by index, name or position. If two displays match, the leftmost wins, then the topmost. With no such display it opens as a normal decorated window centred on the primary display. The choice is a pure function in `src-tauri/src/placement.rs`.

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
