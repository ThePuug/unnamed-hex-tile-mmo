# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build

# Run (separate processes)
cargo run --bin server
cargo run --bin client

# Tests
cargo test                    # All tests
cargo test physics            # Specific module tests
cargo test actor
```

## Code Organization

- `src/common/`: Shared code between client and server (components, physics, messages)
- `src/client/`: Client-only code (rendering, input, camera)
- `src/server/`: Server-only code (AI, terrain generation, connections)
- `src/run-client.rs`: Client binary entry point
- `src/run-server.rs`: Server binary entry point
- `lib/qrz/`: Custom hexagonal grid library

## Critical Reading

**ALWAYS read GUIDANCE.md before making changes.** It contains essential architecture details, TDD workflow, and common pitfalls.

## Internal Libraries

**qrz** (`lib/qrz/`): Custom hexagonal coordinate system library. Provides `Qrz` coordinates, `Map` conversion utilities, and hex grid math.
