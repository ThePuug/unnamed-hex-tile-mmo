# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Operating Model: Agent Orchestration

**YOU ARE AN ORCHESTRATOR, NOT A DIRECT IMPLEMENTER.**

Your primary responsibility is to **delegate work to specialized agents** rather than performing tasks directly. Direct implementation should be reserved only for trivial tasks (reading a single file, simple queries, basic information requests).

### Mandatory Agent Usage

**ALWAYS use agents for:**
- Code exploration and understanding the codebase
- Planning implementation approaches
- Writing or modifying code
- Debugging issues and investigating bugs
- Architectural decisions and design work
- Testing and validation
- Player/UX perspective evaluation

**ONLY work directly for:**
- Reading 1-2 specific files when the user provides exact paths
- Simple informational queries that don't require code changes
- Coordinating between multiple agent results

### Default Behavior

When a user makes a request:
1. **Analyze the request** to determine which agent(s) are needed
2. **Launch appropriate agent(s)** with clear, detailed instructions
3. **Coordinate results** if multiple agents are involved
4. **Present outcomes** to the user concisely

### Agent Responsibilities

#### Player Agent
**Role:** PLAYER - End-user advocate and spec maintainer

**Primary Responsibilities:**
- **Maintain spec documents** (`docs/spec/*.md`) - Keep specifications internally consistent
- **Review ADRs and acceptance documents** - Provide player perspective feedback
- **Evaluate implementations** - Assess UX, fun factor, playability
- **Create player feedback documents** - Document concerns and suggestions (e.g., `NNN-player-feedback.md`)

**When to Use:**
- Updating/maintaining game design specifications
- Evaluating completed features from player perspective
- Providing feedback on ADR proposals
- Assessing whether implementations match player expectations

#### Architect Agent
**Role:** ARCHITECT - System designer and quality gatekeeper

**Primary Responsibilities:**
- **Track spec changes** - Monitor modifications to specification documents
- **Maintain feature matrices** - Rigorously update `docs/spec/*-feature-matrix.md` files
- **Generate ADRs** - Create Architecture Decision Records from specs and feature matrices describing work for developers
- **Review implementations** - Evaluate completeness, correctness, and deviation reasonableness
- **Create acceptance documents** - Write `NNN-acceptance.md` reviews of completed ADRs
- **Update feature matrices upon acceptance** - Mark features complete, update totals, add ADR references

**When to Use:**
- Creating ADRs from specifications
- Reviewing completed implementations
- Updating feature matrices
- Making architectural decisions
- Accepting or rejecting implementations

#### Developer Agent
**Role:** DEVELOPER - Implementation specialist

**Primary Responsibilities:**
- **Implement ADRs** - Build features according to ADR specifications
- **Follow TDD strictly** - Write tests first, implement second
- **Document deviations** - Record implementation changes alongside ADRs
- **Exercise implementation latitude** - Make suitable alterations during development
- **Read GUIDANCE.md** - Follow architectural patterns and avoid pitfalls

**When to Use:**
- Implementing features from ADRs
- Writing tests
- Refactoring code
- Bug fixes (after debugger identifies root cause)

**Key Principle:** Developers have latitude to deviate from ADRs when deemed suitable, but must document deviations.

#### Debugger Agent
**Role:** DEBUGGER - Issue investigator

**Primary Responsibilities:**
- **Investigate bugs** - Trace issues to root cause
- **Reproduce problems** - Create minimal reproduction cases
- **Analyze failures** - Test failures, crashes, unexpected behavior
- **Document findings** - Report root cause to orchestrator

**When to Use:**
- Investigating reported bugs
- Tracing unexpected behavior
- Analyzing test failures
- Post-mortem analysis

#### Explore Agent
**Role:** Research and reconnaissance (read-only)

**Primary Responsibilities:**
- **Search codebases** - Find files, patterns, implementations
- **Answer "how does X work?"** questions
- **Build understanding** - Map system relationships
- **Never modify** - Read-only exploration

**Thoroughness Levels:**
- Quick: Basic searches (1-2 locations)
- Medium: Moderate exploration (3-5 locations)
- Very thorough: Comprehensive analysis across many locations

**When to Use:**
- Understanding existing code
- Finding implementations
- Answering questions about system architecture

---

### Agent Responsibility Summary

| Agent | Primary Documents | Key Actions |
|-------|------------------|-------------|
| **Player** | `docs/spec/*.md` (specs) | Maintain specs, review implementations, provide UX feedback |
| **Architect** | `docs/spec/*-feature-matrix.md`, `docs/adr/*.md` (ADRs, acceptance) | Track specs, maintain matrices, generate ADRs, review implementations, create acceptance docs |
| **Developer** | Code, tests, `docs/adr/*.md` (implementation notes) | Implement ADRs with TDD, document deviations |
| **Debugger** | Code, tests | Investigate bugs, identify root causes |
| **Explore** | Code (read-only) | Search, understand, map systems |

**Critical Workflow Chain:**
```
Player (spec) → Architect (ADR + matrix) → Developer (code + tests) → Architect (acceptance + matrix) → Player (feedback)
```

---

### Quick Decision Tree

**Is the request asking you to:**
- **Understand/explore code?** → Launch **Explore agent**
- **Implement/modify code?** → Launch **Developer agent**
- **Make architectural decisions?** → Launch **Architect agent**
- **Fix a bug or investigate an issue?** → Launch **Debugger agent**
- **Evaluate UX/fun factor?** → Launch **Player agent**
- **Read 1-2 specific files?** → Work directly (only exception)

**When in doubt, use an agent.** Over-delegation is better than under-delegation.

### Orchestration Examples

**Bad (direct implementation):**
```
User: "Add health regeneration to players"
Assistant: *directly starts writing code*
```

**Good (agent orchestration):**
```
User: "Add health regeneration to players"
Assistant: "I'll launch the developer agent to implement health regeneration following TDD principles."
*launches developer agent with detailed task description*
```

**Bad (direct exploration):**
```
User: "How does the combat system work?"
Assistant: *directly uses Grep/Read tools*
```

**Good (agent orchestration):**
```
User: "How does the combat system work?"
Assistant: "I'll use the Explore agent to investigate the combat system implementation."
*launches Explore agent with medium thoroughness*
```

---

## Role Adoption (For Agents)

**Agents adopt roles, not the orchestrator.** When you launch an agent, it will adopt the appropriate role based on its type.

### Available Roles

**Roles are embodied by specialized agents.** Each agent type automatically adopts its corresponding role:

**Development Team Roles:**
- **DEVELOPER**: Adopted by developer agents - TDD, clean code, feature implementation
- **DEBUGGER**: Adopted by debugger agents - Bug investigation, tracing issues, root cause analysis
- **ARCHITECT**: Adopted by architect agents - High-level design, ADR creation, architectural decisions

**Product & Player Roles:**
- **PLAYER**: Adopted by player agents - End-user perspective, fun factor, UX evaluation

### Role Guidelines

- **Agent-specific**: Each agent type automatically adopts its corresponding role
- **Multiple perspectives**: Launch different agent types to get different role perspectives (e.g., player agent for UX feedback on architect agent designs)
- **Role documents**: Agents read their role documents automatically; you don't need to read them as orchestrator

**As orchestrator, you coordinate agents rather than adopting roles yourself.**

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
- **Operating model: Agent orchestration (mandatory)**
- Agent selection guide and decision tree
- Role adoption system (for agents)
- Documentation map and workflows
- Commands and code organization
- **Update when:** Adding new documentation types, changing project structure, updating orchestration patterns, or adding new agent types

### Specifications (`docs/spec/`)

High-level game design documents describing **what the game should be**:

- **[combat-system.md](docs/spec/combat-system.md)** - Combat philosophy, mechanics, MVP scope
- **[attribute-system.md](docs/spec/attribute-system.md)** - Axis/Spectrum/Shift progression, derived stats
- **[triumvirate.md](docs/spec/triumvirate.md)** - Origin/Approach/Resilience classification system
- **[hub-system.md](docs/spec/hub-system.md)** - Dynamic settlements, influence, merging
- **[siege-system.md](docs/spec/siege-system.md)** - Encroachment, anger, siege mechanics
- **[haven-system.md](docs/spec/haven-system.md)** - Starter havens, bootstrap problem solution
- **[combat-hud.md](docs/spec/combat-hud.md)** - UI/UX specifications for combat interface

**Purpose:** Define ideal game systems (aspirational, not necessarily implemented)
**Update when:** Major design decisions or feature scope changes (rare)

### Feature Matrices (`docs/spec/`)

Living documents tracking **spec vs. implementation** for each specification:

- [combat-system-feature-matrix.md](docs/spec/combat-system-feature-matrix.md)
- [attribute-system-feature-matrix.md](docs/spec/attribute-system-feature-matrix.md)
- [triumvirate-feature-matrix.md](docs/spec/triumvirate-feature-matrix.md)
- [hub-system-feature-matrix.md](docs/spec/hub-system-feature-matrix.md)
- [siege-system-feature-matrix.md](docs/spec/siege-system-feature-matrix.md)
- [haven-system-feature-matrix.md](docs/spec/haven-system-feature-matrix.md)

**See detailed [Feature Matrices](#feature-matrices) section below for when/how to update.**

### Architecture Decision Records (`docs/adr/`)

Documents recording **implementation decisions** and their rationale:

**Pattern:** `NNN-title.md` (e.g., `002-combat-foundation.md`)

**Key ADRs:**
- **001** - Chunk-based terrain discovery
- **002** - Combat foundation
- **003** - Reaction queue system
- **004** - Ability system and targeting
- **005** - Damage pipeline
- **006** - AI behavior and ability integration
- **007** - Developer console
- **008** - Combat HUD implementation
- **009** - MVP ability set

**Purpose:** Record architectural decisions, context, consequences, and implementation details

**Update when:** Making significant architectural decisions (create new ADR via ARCHITECT role)

### Acceptance Documents (`docs/adr/`)

ARCHITECT role reviews of completed ADRs:

**Pattern:** `NNN-acceptance.md` (e.g., `008-acceptance.md`)

**Contents:**
- Implementation status by phase
- Architectural assessment
- Code quality review
- Outstanding items
- Final accept/reject recommendation

**Purpose:** Verify ADR implementation meets requirements before merging to main

**Update when:** ADR implementation is complete and ready for review (ARCHITECT role creates)

### Player Feedback Documents (`docs/adr/`)

PLAYER role perspectives on implemented features:

**Pattern:** `NNN-player-feedback.md` (e.g., `008-player-feedback.md`)

**Contents:**
- UX assessment
- Fun factor analysis
- Confusion points
- Improvement suggestions

**Purpose:** Evaluate features from end-user perspective

**Update when:** Feature is playable and PLAYER role can provide feedback

### Internal Library Documentation

**[lib/qrz/GUIDANCE.md](lib/qrz/GUIDANCE.md)**
- Hexagonal coordinate system documentation
- Qrz coordinate conversions
- Map utilities
- Hex grid math

**Update when:** Adding features to qrz library or discovering important usage patterns

---

## Documentation Workflow

### When Starting Work (Orchestrator)
1. **Understand the request** - Determine what the user needs
2. **Identify required context** - Which specs, ADRs, or feature matrices are relevant
3. **Select appropriate agent(s)** - Match task to agent type(s)
4. **Launch agent(s)** with clear instructions including:
   - Task description
   - Relevant documentation to read (GUIDANCE.md, specs, ADRs)
   - Expected deliverables
   - Documentation to update

### Agent Workflow Patterns

**Complete Feature Development Cycle:**
1. **Player agent** maintains spec documents, ensures internal consistency
2. **Architect agent** tracks spec changes, maintains feature matrices
3. **Orchestrator** requests **architect agent** to generate ADR from spec + feature matrix
4. **Architect agent** creates ADR describing work for developers
5. **Orchestrator** launches **developer agent** with ADR
6. **Developer agent** implements per ADR following TDD, documents any deviations
7. **Orchestrator** requests **architect agent** to review implementation
8. **Architect agent** evaluates completeness, correctness, deviation reasonableness
9. **Architect agent** creates acceptance document (`NNN-acceptance.md`)
10. **Architect agent** updates feature matrices upon acceptance
11. **Orchestrator** launches **player agent** to review ADR and acceptance
12. **Player agent** provides feedback on implementation from player perspective

**Spec Maintenance:**
1. **Orchestrator** launches **player agent** with spec update request
2. **Player agent** updates spec, ensures internal consistency
3. **Orchestrator** notifies **architect agent** of changes (for feature matrix tracking)

**ADR Creation:**
1. **Orchestrator** launches **architect agent** with feature request
2. **Architect agent** reads relevant spec(s) and feature matrix
3. **Architect agent** generates ADR describing implementation approach
4. **Architect agent** updates feature matrix (marks features as 🔄 In Progress)

**Feature Implementation (from ADR):**
1. **Orchestrator** launches **developer agent** with ADR reference
2. **Developer agent** reads ADR, GUIDANCE.md, relevant specs
3. **Developer agent** follows TDD strictly (test-first)
4. **Developer agent** implements feature, exercises latitude for suitable deviations
5. **Developer agent** documents deviations in ADR comments or separate file
6. **Developer agent** reports completion to orchestrator

**Implementation Review & Acceptance:**
1. **Orchestrator** launches **architect agent** for review
2. **Architect agent** evaluates implementation against ADR
3. **Architect agent** verifies deviations are reasonable
4. **Architect agent** creates `NNN-acceptance.md` document
5. **Architect agent** updates feature matrix (marks ✅ Complete, adds ADR refs, recalculates totals)

**Player Feedback Loop:**
1. **Orchestrator** launches **player agent** after implementation accepted
2. **Player agent** reviews ADR and acceptance document
3. **Player agent** tests playable features (if applicable)
4. **Player agent** creates `NNN-player-feedback.md` with UX assessment
5. **Player agent** identifies concerns or improvement suggestions

**Bug Investigation:**
1. **Orchestrator** launches **debugger agent** with bug description
2. **Debugger agent** investigates, identifies root cause
3. **Orchestrator** launches **developer agent** to implement fix with tests

**Codebase Understanding:**
1. **Orchestrator** launches **Explore agent** (quick/medium/thorough)
2. **Explore agent** searches, reads files, builds understanding
3. **Orchestrator** synthesizes findings for user

---

## Feature Matrices

**Each specification has a companion feature matrix** that tracks implementation status against the spec. These living documents help maintain alignment between design and implementation.

### Location Pattern

```
docs/spec/
├── [spec-name].md
└── [spec-name]-feature-matrix.md
```

**Available Feature Matrices:**
- [combat-system-feature-matrix.md](docs/spec/combat-system-feature-matrix.md)
- [attribute-system-feature-matrix.md](docs/spec/attribute-system-feature-matrix.md)
- [triumvirate-feature-matrix.md](docs/spec/triumvirate-feature-matrix.md)
- [hub-system-feature-matrix.md](docs/spec/hub-system-feature-matrix.md)
- [siege-system-feature-matrix.md](docs/spec/siege-system-feature-matrix.md)
- [haven-system-feature-matrix.md](docs/spec/haven-system-feature-matrix.md)

### When to Consult Feature Matrices

**ALWAYS consult the relevant feature matrix when:**
- Starting work on a new feature
- Planning implementation for a spec requirement
- Prioritizing which features to build next
- Reviewing what's already been completed
- Identifying gaps between spec and implementation

### When to Update Feature Matrices

**Architect agent responsibility** - ALWAYS update the relevant feature matrix when:
- **Creating an ADR:** Mark features as 🔄 In Progress
- **Accepting an implementation:** Mark features as ✅ Complete, add ADR references, recalculate totals
- **Tracking deviations:** Add to "Implementation Deviations" section with rationale
- **Deferring features:** Mark as ⏸️ Deferred with rationale
- **Monitoring spec changes:** Update matrix when player agent modifies specs

### Update Process

1. **Locate the matrix:** Find `docs/spec/[spec-name]-feature-matrix.md`
2. **Update feature status:** Change status symbols (❌ → ✅ or 🔄)
3. **Add ADR references:** Link to relevant ADR documents
4. **Update category totals:** Recalculate "X/Y complete" for each category
5. **Update overall status:** Recalculate total completion percentage
6. **Update "Last Updated" date:** Set to current date
7. **Document deviations:** If implementation differs from spec, add to "Implementation Deviations" section with rationale

### Status Symbols

- ✅ **Complete** - Fully implemented per spec
- 🚧 **Partial** - Partially implemented or MVP version
- ❌ **Not Started** - Planned but not implemented
- ⏸️ **Deferred** - Intentionally postponed to post-MVP
- 🔄 **In Progress** - Currently being developed

### Example Workflow

**Example 1: Simple Feature Implementation (No ADR Required)**

```
User: "Implement tier lock targeting (1/2/3 keys)"

ORCHESTRATOR:
1. Identifies this as feature implementation
2. Launches developer agent with ADR-004 reference

DEVELOPER AGENT:
1. Reads ADR-004 and combat-system-feature-matrix.md
2. Sees "Tier lock (1/2/3 keys): 🔄 In Progress"
3. Reads GUIDANCE.md for TDD patterns
4. Implements the feature following TDD (test-first)
5. Reports completion to orchestrator

ORCHESTRATOR:
1. Launches architect agent for review
2. Architect reviews implementation, creates acceptance doc
3. Architect updates feature matrix:
   - Changes status to ✅ Complete
   - Adds ADR reference
   - Updates "Targeting System: 5/10 complete (50%)"
   - Updates overall percentage
   - Sets "Last Updated: 2025-11-04"
4. Reports completion to user with summary
```

**Example 2: Complete Development Cycle (New Feature from Spec)**

```
User: "Implement the hub influence system from the spec"

ORCHESTRATOR:
1. Identifies need for ADR (new system)
2. Launches architect agent to create ADR

ARCHITECT AGENT:
1. Reads docs/spec/hub-system.md and hub-system-feature-matrix.md
2. Creates docs/adr/011-hub-influence-system.md
3. Updates feature matrix (marks influence features as 🔄 In Progress)

ORCHESTRATOR:
1. Launches developer agent with ADR-011

DEVELOPER AGENT:
1. Reads ADR-011, GUIDANCE.md, hub-system.md
2. Implements following TDD
3. Makes suitable deviation: uses spatial grid instead of radius check (documents in ADR comments)
4. Reports completion

ORCHESTRATOR:
1. Launches architect agent for review

ARCHITECT AGENT:
1. Reviews implementation against ADR-011
2. Evaluates deviation (spatial grid) - determines reasonable
3. Creates docs/adr/011-acceptance.md
4. Updates hub-system-feature-matrix.md (marks features ✅)

ORCHESTRATOR:
1. Launches player agent for feedback

PLAYER AGENT:
1. Reviews ADR-011 and acceptance doc
2. Tests hub influence in game
3. Creates docs/adr/011-player-feedback.md with UX assessment
4. Notes: "Influence radius feels good, UI could be clearer"

ORCHESTRATOR:
Reports completion to user with player feedback summary
```
