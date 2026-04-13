# 3DMMEx Modern — Rust Port

Modern reimplementation of 3D Movie Maker using Rust, wgpu, and Tauri.

> **File format compatibility is sacred.** This port reads and writes files that are 100% compatible with the original 1995 3D Movie Maker.

## Quick Start

```bash
# Build all crates
cargo build

# Run tests
cargo test --workspace

# Inspect a .3mm file
cargo run -p cli -- inspect ../samples/bongo.3mm

# Validate round-trip compatibility
cargo run -p cli -- validate ../samples/bongo.3mm
```

## Project Structure

```
modern/
├── crates/
│   ├── chunky-format/   # CHN2 file format reader/writer
│   ├── engine/          # Domain model (Movie, Scene, Actor)
│   ├── renderer/        # wgpu 3D rendering
│   ├── audio/           # WAV + MIDI playback
│   └── cli/             # Command-line tools
├── app/                 # Tauri desktop application (future)
└── web/                 # WASM web version (future)
```

## Requirements

- Rust 1.75+ (stable)
- For the desktop app (future): Node.js 18+
- For the web build (future): wasm-pack

## Choosing Your Build

From the repository root, you can build either version:

```bash
# Original C++ build
cmake --preset x86-msvc-debug
cmake --build build/x86-msvc-debug

# Modern Rust build
cd modern
cargo build --release
```

## License

MIT — same as the parent 3DMMEx project.
