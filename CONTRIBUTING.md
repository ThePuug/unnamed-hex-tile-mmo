# Contributing

## Prerequisites

### Windows (Native)

The primary development environment. You need:

- [Rust toolchain](https://rustup.rs/) (install via `rustup`)
- Visual Studio Build Tools (installed automatically with rustup on Windows)

### WSL / Linux

Required for running headless tools like `terrain-viewer` on Linux, or for cross-platform validation.

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
cargo run -p server          # Run server
cargo run -p client          # Run client (requires display)
```

## Development Tools

### terrain-viewer

Headless terrain visualization tool. Renders heightmap PNGs for validating terrain generation without running the full client.

```bash
cargo run -p terrain-viewer -- --mode elevation --radius 20000 --scale 10 --output elevation.png
```

Available modes: `elevation`, `plates`, `boundary-type`, `plate-character`, `slope`

See `cargo run -p terrain-viewer -- --help` for all options.

### console

Server monitoring console tool.

```bash
cargo run -p console
```

## Project Structure

See [CLAUDE.md](CLAUDE.md) for code organization, documentation map, and development workflow.

## Roles and Workflow

This project uses a role-based development process documented in the `ROLES/` directory. See [CLAUDE.md](CLAUDE.md) for details.
