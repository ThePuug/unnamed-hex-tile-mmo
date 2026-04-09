# CLAUDE.md

Instructions for Claude Code sessions in this repository.

## Role Adoption

**Adopt a role each session.** Default: **DEVELOPER** (`ROLES/DEVELOPER.md`).

Available roles: **DEVELOPER**, **DEBUGGER**, **ARCHITECT**, **STAFF_ENGINEER**, **PLAYER** — see `ROLES/` for details.

- User can switch roles at any time
- Periodically re-read your role document during long sessions
- Read and adopt the DEVELOPER role at the start of each session by default
- Read your role's `*-MEMORY.md` at session start for cross-session continuity
- Update your role's `*-MEMORY.md` at session end with current train of thought

## Commands

```bash
cargo build
cargo run -p server                # separate processes
cargo run -p client
cargo test                         # all tests
cargo test -p common physics       # specific module
cargo test -p server actor
```

## Documentation

| Location | Purpose |
|----------|---------|
| `GUIDANCE.md` | **Read before coding.** Architectural patterns, invariants, pitfalls. |
| `docs/design/` | Design specs — what systems should be. Each has Implementation Deviations/Gaps sections. |
| `docs/adr/` | Architecture Decision Records — non-obvious "why" behind implementation choices. |
| `ROLES/` | Role definitions for Claude sessions. |
| `ROLES/*-MEMORY.md` | Per-role session memory — current concerns, pending items, train of thought. |
| `crates/qrz/GUIDANCE.md` | Hex coordinate system reference. |
| `README.md` | User-facing overview, controls, features. |
| `CONTRIBUTING.md` | Build prerequisites, platform setup. |

## Workflow

1. Read role document + `GUIDANCE.md` before making changes
2. Check relevant design spec (including deviations/gaps at bottom)
3. After completing work: update design spec deviations/gaps if implementation differs
3b. If significant implementation: request ARCHITECT session to reconcile docs
3c. If performance-sensitive: request STAFF_ENGINEER review
4. If architectural decision made: create ADR via ARCHITECT role (only if non-obvious "why")
5. If new pitfall discovered: add to `GUIDANCE.md`
