# Documentation System

This directory contains the complete documentation for the game's design, architecture, and implementation. The system is designed to maintain clear separation of concerns while providing full context traceability from initial concept to final implementation.

## Document Types

### 00-spec/ - Game Design Specifications
**Creator:** PLAYER role (game design perspective)
**Purpose:** Define what the game should be from the player's perspective
**Lifecycle:** Living documents that evolve with the game
**Audience:** Designers, developers, players, anyone interested in the game

Specs describe mechanics, interactions, player experience, and fun factor without prescribing implementation details.

### 01-rfc/ - Requests for Comments
**Creators:** PLAYER + ARCHITECT roles (collaborative)
**Purpose:** Define features and establish feasibility before implementation
**Lifecycle:** Draft → Under Review → Approved (then frozen)
**Audience:** Design and technical team

RFCs bridge player needs and technical reality. They contain:
- Feature request (player need, desired experience)
- Feasibility analysis (technical constraints, integration)
- Discussion (iteration between PLAYER and ARCHITECT)
- Approval decision

**Constraints:**
- Each RFC must be completable in a single SOW (≤20 hours)
- Target length: ~200 lines (if longer, consider splitting the feature)

### 02-adr/ - Architecture Decision Records
**Creator:** ARCHITECT role (extracted from approved RFCs)
**Purpose:** Document significant architectural decisions and their rationale
**Lifecycle:** Permanent (never deleted, only superseded)
**Audience:** Developers, technical team

ADRs capture architectural pillars - the foundational decisions that affect multiple systems and are hard to change. Not every RFC produces an ADR; only those containing significant architectural choices.

**Format:** One decision per document. Why we chose X over Y, consequences, alternatives considered. Target length: ~200 lines (if longer, consider whether multiple decisions should be separate ADRs).

### 03-sow/ - Statements of Work
**Creators:** ARCHITECT + DEVELOPER roles (collaborative)
**Purpose:** Define and execute implementation work packages
**Lifecycle:** Planned → In Progress → Review → Approved → Merged (then frozen)
**Audience:** Implementation team

SOWs are work orders for developers. They contain:
- Implementation plan (phases, deliverables, acceptance criteria)
- Discussion (implementation questions, decisions, deviations)
- Acceptance review (final assessment before merge)

**Mapping:** 1 RFC → 1 SOW (one-to-one relationship)

**Philosophy:** SOWs define **what** needs to be built, **why** it matters, and the **constraints** the implementation must satisfy. They are NOT prescriptive about **how** to implement - no code snippets or detailed step-by-step instructions. Developers have autonomy over implementation details, patterns, and approaches within the defined constraints.

**Length:** Target ~200 lines at draft (Planned status), can grow to ~300 lines as Discussion and Acceptance Review sections are added during implementation.

---

## The Process

### 1. Specification Creation

**PLAYER** creates or updates game design specifications in `00-spec/`.

Specs define player experience, mechanics, and what makes the game fun.

**Output:** `00-spec/[system].md`

### 2. Feature Matrix Maintenance

**ARCHITECT** maintains feature tracking matrices alongside specs.

Matrices show what's designed, what's implemented, and current status.

**Output:** `00-spec/[system]-feature-matrix.md`

**Update triggers:**
- Spec changes (new features added to design)
- RFC approved (features marked "Planned")
- SOW started (features marked "In Progress")
- SOW merged (features marked "Complete")

### 3. Feature Request

**PLAYER** identifies a needed feature and creates an RFC document.

The RFC starts with a feature request section describing:
- Player need (what problem this solves)
- Desired experience (how it should feel to play)
- Priority justification (why this feature now)

**Output:** `01-rfc/NNN-[feature].md` (Status: Draft)

### 4. Feasibility Analysis

**ARCHITECT** evaluates the feature request and adds to the RFC:
- Technical assessment (can we build this?)
- System integration (how does it fit existing architecture?)
- Risks and unknowns

**Output:** RFC updated (Status: Draft)

### 5. Discussion and Iteration

**PLAYER** and **ARCHITECT** iterate within the RFC document.

Iterations happen in the "Discussion" section:
- PLAYER raises concerns about player experience
- ARCHITECT proposes solutions or adjustments
- Both roles refine the feature until consensus

**Output:** RFC updated (Status: Under Review)

### 6. RFC Approval

**PLAYER** and **ARCHITECT** reach agreement on scope and plan.

**Approval criteria:**
- PLAYER validates: solves the player need
- ARCHITECT validates: feasible and maintainable
- Scope constraint: fits in one SOW (≤20 hours)
- No unresolved conflicts

**Output:** RFC updated (Status: Approved)

### 7. Architecture Decision Records

**ARCHITECT** extracts ADRs from approved RFCs when they contain significant architectural decisions.

Not every RFC produces an ADR - only those introducing architectural pillars that affect multiple systems.

**Output:** `02-adr/NNN-[decision].md` (if applicable)

### 8. Statement of Work Creation

**ARCHITECT** creates a SOW from the approved RFC.

The SOW includes:
- Implementation plan (phases, deliverables, estimates)
- Acceptance criteria (how we know it's done)
- Reference to the RFC

**Output:** `03-sow/NNN-[feature].md` (Status: Planned)

**Feature matrix updated:** Feature marked "Planned" with RFC and SOW links

### 9. Implementation

**DEVELOPER** begins implementation, working through phases.

**SOW status updated:** Planned → In Progress
**Feature matrix updated:** Feature marked "In Progress"

Work is tracked via:
- Git commits (implementation log)
- SOW Discussion section (questions, decisions, deviations)

Deviations from the plan are documented in the SOW's Discussion section as they occur, with rationale and impact noted.

**Output:** Code commits + SOW Discussion updates

### 10. Implementation Review

**ARCHITECT** reviews the implementation within the SOW document.

Review assesses:
- Acceptance criteria met
- Deviations (documented in Discussion with rationale)
- Test coverage and passing
- Code quality and maintainability
- Integration and regressions

**Possible outcomes:**
- **Approved:** Ready to merge
- **Needs Changes:** Specific requirements for revision
- **RFC Revision Required:** Implementation revealed RFC was infeasible

**Output:** SOW updated with Acceptance Review section (Status: Review → Approved)

### 11. Merge and Completion

Branch is merged to main when Acceptance Review reaches Approved status.

**ARCHITECT** updates the feature matrix to reflect completion:
- Status: "Complete"
- Link to RFC and SOW
- Deviations (if any divergence from spec)

**If implementation deviates significantly from spec, ARCHITECT decides:**
- **Update Spec:** Implementation proved a better design
- **Document Deviation:** Implementation is MVP, spec describes ideal
- **Reject:** Implementation must change to match spec (rare, caught in review)

**Output:** SOW updated (Status: Merged), Feature matrix updated

---

## When to Use the Process

**Multi-commit work → RFC + SOW:**
- Significant changes requiring multiple commits
- New features or systems
- Changes affecting multiple files or systems
- Anything requiring design iteration or architectural decisions

**Single-commit work → Direct to code:**
- Bug fixes
- Parameter tuning
- Minor polish
- Typo fixes
- Obvious improvements

**Simple rule:** If it takes more than one commit, it deserves an RFC and SOW. The process isn't onerous - it captures context and ensures proper review.

---

## Document Status Lifecycle

### RFC Status Flow
```
Draft → Under Review → Approved
```

### SOW Status Flow
```
Planned → In Progress → Review → Approved → Merged
```

---

## Key Principles

### Single Source of Truth
- **Specs:** Authoritative game design
- **Feature Matrix:** Current implementation status
- **RFCs:** Approved feature plans
- **ADRs:** Architectural decisions
- **SOWs:** Implementation records

### Context in One Document
- RFC contains entire feature request → feasibility → approval discussion
- SOW contains entire implementation plan → discussion → review
- No need to hunt across multiple documents to understand decisions

### Traceability
Every feature can be traced:
```
Spec (design) → RFC (request + feasibility) → ADR (architecture) → SOW (implementation) → Feature Matrix (status)
```

### Iteration Where It Happens
- Design iteration happens in RFCs
- Implementation iteration happens in SOWs
- All discussion captured in the relevant document

---

## Document Numbering

- **Specs:** Named by system (e.g., `combat-system.md`)
- **RFCs:** Numbered sequentially (e.g., `014-spatial-difficulty.md`)
- **ADRs:** Numbered independently (e.g., `002-ecs-combat-foundation.md`)
- **SOWs:** Match RFC numbers (e.g., RFC-014 → SOW-014)

**Rationale:** RFC and SOW share numbers because they're 1-to-1 related. ADRs have independent numbering because architectural decisions don't map 1-to-1 to features.

---

## Roles

The documentation system assumes three perspectives:

### PLAYER
- Creates specs (game design)
- Initiates feature requests
- Validates player experience
- Advocates for fun, clarity, accessibility

### ARCHITECT
- Maintains feature matrices
- Evaluates technical feasibility
- Creates RFCs (with PLAYER)
- Extracts ADRs from approved RFCs
- Creates SOWs from approved RFCs
- Reviews implementations
- Decides spec vs. implementation authority

### DEVELOPER
- Implements SOWs
- Documents implementation decisions in Discussion section
- Works through deviations via Discussion

---

## Best Practices

### Writing Good Specs
- Focus on player experience, not implementation
- Describe what's fun and why
- Include examples and edge cases
- Keep implementation details out

### Writing Good RFCs
- Start with clear player need
- Be honest about technical constraints
- Document iteration (why changes were made)
- Scope to fit one SOW

### Writing Good ADRs
- One decision per document
- Explain why, not just what
- List alternatives considered
- Document consequences

### Writing Good SOWs
- Define **what** to build and **why** it matters, not **how** to build it
- Specify **constraints** the implementation must satisfy (performance, integration, compatibility)
- Provide developer autonomy - no code snippets or prescriptive implementation steps
- Clear phases with deliverables
- Explicit acceptance criteria
- Document deviations in Discussion as they occur
- Honest assessment in review

---

## Reference

### Folder Structure
```
/docs/
├── README.md (this file)
├── 00-spec/          - Game design specs and feature matrices
├── 01-rfc/           - Requests for Comments
├── 02-adr/           - Architecture Decision Records
└── 03-sow/           - Statements of Work
```

### Templates and Guides

Each folder contains a README with templates, examples, and writing guidance:
- `00-spec/README.md` - Spec writing guide
- `01-rfc/README.md` - RFC process and template
- `02-adr/README.md` - ADR format and examples
- `03-sow/README.md` - SOW template and deviation rules

### Recovering Context

Starting a fresh session? Point to the relevant document:
- **"Continue RFC-014"** → All context in `01-rfc/014-spatial-difficulty.md`
- **"Continue SOW-014"** → All context in `03-sow/014-spatial-difficulty.md`
- **"What's left to do?"** → Check feature matrix in `00-spec/`

### Quick Navigation

For questions about:
- **What to build:** See specs in `00-spec/`
- **Why we built it this way:** See RFCs in `01-rfc/` and ADRs in `02-adr/`
- **How it was built:** See SOWs in `03-sow/`
- **What's done vs. planned:** See feature matrices in `00-spec/`