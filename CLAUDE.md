# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run
cargo run -p bmz-app
cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result

# Lint & format
cargo clippy
cargo fmt
cargo fmt --check

# Test
cargo test
cargo test -p <crate-name>
```

**Useful run flags:**
- `--boot-play-sample` — load the bundled sample chart on boot
- `--autoplay-on-start` — enable autoplay (useful for smoke tests)
- `--smoke-exit-on-result` — exit automatically when the result screen is reached
- `--smoke-exit-after-frames <N>` — exit after N rendered frames

## Architecture

Cargo workspace with 6 domain crates under `crates/`:

| Crate | Responsibility |
|---|---|
| `bmz-app` | Entry point, screens (select/play/result), config, SQLite storage |
| `bmz-core` | Shared primitive types: `Lane`, `Judge`, `TimeUs`, `ChartTick`, `NoteId`, replay, input events |
| `bmz-chart` | BMS format parsing pipeline → `PlayableChart` |
| `bmz-audio` | cpal-backed audio engine, clock sync, mixer |
| `bmz-gameplay` | Judge engine, scoring, gauges, autoplay, key bindings |
| `bmz-render` | wgpu renderer, beatoraja JSON skin system, text/image rendering |

### App flow

`main.rs` → `bootstrap.rs` (config, DB, library scan) → `app.rs` (winit event loop) → screen state machine:

```
SelectState → PlayStartState → PlayLoopState → ResultState
```

Each screen builds a render snapshot (`scene.rs`) which `bmz-render` consumes to produce GPU draw calls.

### Skin system

Skins are beatoraja-compatible JSON definitions loaded from `assets/`. `bmz-render` parses skin JSON into a node tree and evaluates conditions/value-references at render time against the current game snapshot. Node types include images, text, gauges, judge numbers, animations, and progress sliders.

### Storage layout

Runtime data lives in `data/` (not committed):
- `config.toml` — app/audio/video/input settings
- `profiles/` — per-profile configs
- `library.db` — song/chart metadata (SQLite)
- `score_db.db` — play scores and replays (SQLite)
- `logs/` — tracing output

### Key conventions

- Error handling: `anyhow::Result` at app boundaries, `thiserror` for domain errors
- Logging: `tracing` crate; log level configured in `config.toml`
- Clippy config in `Cargo.toml`: line width 100, `too_many_arguments` threshold 10
- In-module `#[cfg(test)]` blocks for unit tests; no separate integration test directory yet
