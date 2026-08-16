# CLAUDE.md

Instructions for Claude Code sessions in this repository.

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

This repo is code-only. Design specs, ADRs, and architectural guidance live in the
`unnamed-indie-studio-internal` repo (sibling checkout, `../unnamed-indie-studio-internal/projects/unnamed-hex-tile-mmo/`):

| Location (in that repo) | Purpose |
|----------|---------|
| `GUIDANCE.md` | Architectural patterns, invariants, pitfalls. Read before coding. |
| `qrz-guidance.md` | Hex coordinate system reference. |
| `networking.md` | Transport, movement sync, visual interpolation. |
| `design.md` | Game pitch / north-star. |
| `design/` | Technical specs, one per system. |

In this repo:

| Location | Purpose |
|----------|---------|
| `README.md` | User-facing overview, controls, features. |
| `CONTRIBUTING.md` | Build prerequisites, platform setup. |

## Workflow

1. Check the relevant spec in the internal-studio repo before making changes
2. After completing work: update the spec there if behavior changed
3. If a new pitfall is discovered: add it to `GUIDANCE.md` there

**Docs describe what the code does.** Don't add Implementation Deviations / Gaps tables,
status checklists, phase plans, or roadmap sections — that scaffolding was deliberately
removed. If something is unbuilt, state it in one line inline. Verify claims against the
code before writing them; these docs drifted badly once already.
