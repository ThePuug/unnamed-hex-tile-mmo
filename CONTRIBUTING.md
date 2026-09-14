# Contributing

How to build and run the project. Not architecture or design — see `AGENTS.md` for codebase constraints and conventions, and `design/` in the `unnamed-indie-studio-internal` repo (sibling checkout, `projects/unnamed-hex-tile-mmo/`) for system specs.

## Prerequisites

### Windows (Native)

The primary development environment. You need:

- [Rust toolchain](https://rustup.rs/) (install via `rustup`)
- Visual Studio Build Tools (installed automatically with rustup on Windows)

### WSL / Linux

Required for running headless tools like `world-viewer` on Linux, or for cross-platform validation.

Install the Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install system dependencies:

```bash
sudo apt update
sudo apt install -y clang mold pkg-config libasound2-dev libudev-dev
```

**Why these packages?**

| Package | Reason |
|---------|--------|
| `clang` | Linker and C compiler required by several Rust crates |
| `mold` | Fast linker configured in `.cargo/config.toml` for faster builds |
| `pkg-config` | Locates system libraries needed by native dependency crates |
| `libasound2-dev` | ALSA headers required by `alsa-sys` (Bevy audio dependency) |
| `libudev-dev` | udev headers required by Bevy's input/gamepad subsystem |

## Build and Run

```bash
cargo build                  # Build everything
cargo run --bin server       # Run server
cargo run --bin client       # Run client (requires display)
```

## Development Tools

### world-viewer

Headless world composite visualization tool. Renders PNGs for validating terrain generation and world event output without running the full client.

```bash
cargo run --bin world-viewer -- --layers plates,elevation --radius 15000 --scale 8 --output world.png
```

Views, what each reads, and when one is added or removed: `crates/world-viewer/README.md`.

See `cargo run --bin world-viewer -- --help` for all options.

### texgen

Tileable texture generator. Each texture is a module; the PNG under `assets/textures` is its output.

```bash
cargo run --bin texgen -- grass-plain
cargo run --bin texgen -- --list
```

Textures and how one is made: `crates/texgen/README.md` and `crates/texgen/AGENTS.md`.

### console

Server monitoring console tool.

```bash
cargo run --bin console
```

## Project Structure

See [AGENTS.md](AGENTS.md) for code organization, invariants, anti-patterns, and comment style.
