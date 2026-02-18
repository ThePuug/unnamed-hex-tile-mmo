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
cargo run -p server
cargo run -p client

# Tests
cargo test                    # All tests
cargo test -p common physics  # Specific module tests
cargo test -p server actor
```

## Code Organization

All workspace members live under `crates/`:

- `crates/common/`: Shared library crate (components, physics, messages)
- `crates/client/`: Client binary crate (rendering, input, camera)
- `crates/server/`: Server binary crate (AI, terrain generation, connections)
- `crates/qrz/`: Custom hexagonal grid library
- `crates/console/`: Server monitoring console tool

## Documentation Map

The repository contains several interconnected documentation systems. Understanding where to find information and when to update documentation is critical.

### Root-Level Documents

**[README.md](README.md)**
- User-facing overview of the game
- Current playable features
- Controls and what to expect
- Architectural foundations list
- **Update when:** Adding major features, changing controls, or updating build instructions

**[GUIDANCE.md](GUIDANCE.md)** ⚠️ CRITICAL
- **ALWAYS read before making code changes**
- Core architecture patterns (client-server, ECS, chunk system)
- TDD workflow rules
- Position/movement system details
- Client-side prediction mechanics
- Common pitfalls and anti-patterns
- System execution order
- **Update when:** User confirms a solution works AND pattern should be documented for future reference
- **Never commit** - only update the file locally

**[CLAUDE.md](CLAUDE.md)** (this file)
- Instructions for Claude Code sessions
- Role adoption system
- Documentation map
- Commands and code organization
- **Update when:** Adding new documentation types, changing project structure, or updating Claude workflow

### Role Documents (`ROLES/`)

Define different perspectives for development work:
- **[DEVELOPER.md](ROLES/DEVELOPER.md)** - Default role: TDD, clean code, feature implementation
- **[ARCHITECT.md](ROLES/ARCHITECT.md)** - High-level design, code organization, architectural decisions
- **[DEBUGGER.md](ROLES/DEBUGGER.md)** - Bug investigation, tracing issues, root cause analysis
- **[PLAYER.md](ROLES/PLAYER.md)** - End-user perspective, UX, fun factor, roadmap priorities

**Update when:** Refining role principles or adding new specialized roles

### Design Specifications (`docs/design/`)

Game design documents describing **what systems should be** and tracking implementation progress:

**Specs:**
- **[combat.md](docs/design/combat.md)** - Combat philosophy, mechanics, abilities, MVP scope, combat UI
- **[attributes.md](docs/design/attributes.md)** - Bipolar pairs, Axis/Spectrum/Shift, three scaling modes
- **[combat-balance.md](docs/design/combat-balance.md)** - Super-linear scaling, queue capacity, dismiss, NPC coordination
- **[triumvirate.md](docs/design/triumvirate.md)** - Origin/Approach/Resilience classification system
- **[hubs.md](docs/design/hubs.md)** - Dynamic settlements, influence, merging
- **[siege.md](docs/design/siege.md)** - Encroachment, anger, siege mechanics
- **[haven.md](docs/design/haven.md)** - Starter havens, bootstrap problem solution

Each spec includes **Implementation Deviations** and **Implementation Gaps** sections at the bottom tracking where the codebase differs from the spec and what remains to be built.

### Architecture Decision Records (`docs/adr/`)

Documents recording **non-obvious implementation decisions** and their rationale. Only decisions where the "why" isn't self-evident are preserved here.

- **001** - Chunk-based world partitioning (8x8 chunks, 3 invariants, bandwidth analysis)
- **007** - Timer synchronization via insertion time (latency compensation pattern)
- **010** - Two-phase damage pipeline (attacker snapshot at insertion, defender at resolution)
- **012** - AI TargetLock and behavior tree integration (sticky targeting, dual FaceTarget)
- **016** - Movement intent architecture (intent-then-confirmation, 70% lag reduction)
- **018** - Ability execution pipeline (3-stage architecture, pure function extraction)
- **019** - Unified interpolation model (Position + VisualPosition, jitter fix)
- **020** - Super-linear level multiplier (polynomial stat scaling)
- **027** - Commitment tiers (20/40/60% thresholds, tier-based identity)
- **030** - Reaction queue window mechanic (unbounded queue, visibility window)
- **031** - Relative meta-attributes rework (rotated oppositions, contest function)

**Purpose:** Preserve non-obvious architectural decisions someone would re-ask about

**Update when:** Making significant architectural decisions (create new ADR via ARCHITECT role)

### Internal Library Documentation

**[crates/qrz/GUIDANCE.md](crates/qrz/GUIDANCE.md)**
- Hexagonal coordinate system documentation
- Qrz coordinate conversions
- Map utilities
- Hex grid math

**Update when:** Adding features to qrz library or discovering important usage patterns

---

## Documentation Workflow

### When Starting Work
1. **Read role document** (default: DEVELOPER)
2. **Read [GUIDANCE.md](GUIDANCE.md)** (architectural patterns)
3. **Check relevant design spec** (including deviations/gaps sections at bottom)
4. **Review related ADRs** (implementation decisions)

### During Development
1. **Follow TDD** (GUIDANCE.md Rule 1)
2. **Write tests first**
3. **Consult specs** for design intent

### After Completing Feature
1. **Update design spec** deviations/gaps sections if implementation differs from spec
2. **Create/update ADR** if architectural decision made
3. **Update GUIDANCE.md** only after user confirms solution works

### When Creating New Systems
1. **ARCHITECT role** creates ADR documenting decision (only if non-obvious "why")
2. **DEVELOPER role** implements per design spec
3. **Update design spec** deviations/gaps as needed
