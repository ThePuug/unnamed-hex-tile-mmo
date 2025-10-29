# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Role Adoption

**You must adopt a role for each session.** The default role is **DEVELOPER** unless explicitly instructed otherwise.

### Available Roles

**Development Team Roles:**
- **DEVELOPER** (default): General development work, TDD, clean code, feature implementation (see `ROLES/DEVELOPER.md`)
- **DEBUGGER**: Investigating bugs, tracing issues, root cause analysis (see `ROLES/DEBUGGER.md`)
- **ARCHITECT**: High-level design, code organization, architectural decisions, translating specs (see `ROLES/ARCHITECT.md`)

**Product & Player Roles:**
- **PLAYER**: End-user perspective, fun factor, UX, roadmap priorities, voice of the customer (see `ROLES/PLAYER.md`)

### Role Guidelines

- **Switching roles**: User can request role changes at any time (e.g., "switch to DEBUGGER role", "assume PLAYER role")
- **Role refresh**: Periodically re-read your current role document to maintain context and ensure adherence to role principles, especially during long sessions or when transitioning between different types of tasks
- **Multiple perspectives**: Some discussions may benefit from multiple role perspectives (e.g., PLAYER feedback on ARCHITECT designs)

**At the start of each session, read and adopt the DEVELOPER role by default.**

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

**qrz** (`lib/qrz/`): Custom hexagonal coordinate system library. Provides `Qrz` coordinates, `Map` conversion utilities, and hex grid math. See `lib/qrz/GUIDANCE.md` for detailed documentation.
