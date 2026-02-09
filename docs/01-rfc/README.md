# Requests for Comments (RFCs)

RFCs bridge player needs and technical reality. Each RFC starts with a player-facing problem, explores feasibility, and results in an approved plan for implementation.

## RFC Index

| # | Title | Category | Status | Created | Has ADR |
|---|-------|----------|--------|---------|---------|
| 001 | Chunk-Based Terrain Discovery | 🌍 World/Terrain | ✅ Implemented | 2025-11-03 | Yes (ADR-001) |
| 002 | Combat Foundation | ⚔️ Combat | ✅ Implemented | 2025-11-03 | Yes (ADR-002/003/004/005) |
| 003 | Reaction Queue System | ⚔️ Combat | ✅ Implemented | 2025-11-03 | Yes (ADR-006/007/008) |
| 004 | Ability System and Directional Targeting | ⚔️ Combat | ✅ Implemented | 2025-11-03 | Yes (ADR-009) |
| 005 | Damage Pipeline | ⚔️ Combat | ✅ Implemented | 2025-11-03 | Yes (ADR-010) |
| 006 | AI Behavior and Ability Integration | 🤖 AI | ✅ Implemented | 2025-11-03 | Yes (ADR-011/012) |
| 007 | Developer Console | 🛠️ Dev Tools | ✅ Implemented | 2025-11-03 | Yes (ADR-013) |
| 008 | Combat HUD | 🎨 UI/HUD | ✅ Implemented | 2025-11-03 | Yes (ADR-014) |
| 009 | MVP Ability Set | ⚔️ Combat | ✅ Implemented | 2025-11-03 | No (design scope) |
| 010 | Combat Variety Phase 1 | ⚔️ Combat | ✅ Implemented | 2025-11-03 | Yes (ADR-015) |
| 011 | Movement Intent System | 🌐 Network | ✅ Implemented | 2025-11-05 | Yes (ADR-016) |
| 012 | Ability Recovery and Synergies | ⚔️ Combat | ✅ Implemented | 2025-11-07 | Yes (ADR-017) |
| 013 | Ability Execution Pipeline | 🔧 Code Quality | 📝 Proposed | 2025-11-07 | Yes (ADR-018) |
| 014 | Spatial Difficulty System | 📊 Progression | ✅ Implemented | 2025-11-07 | No (design scope) |
| 015 | Architectural Invariant Testing | 🧪 Testing | ✅ Implemented | 2025-11-08 | No (testing strategy) |
| 016 | Movement System Rewrite | 🌐 Network | ✅ Approved | 2025-02-08 | Yes (ADR-019) |
| 017 | Combat Balance Overhaul | ⚔️ Combat | ✅ Implemented | 2026-02-09 | Yes (ADR-020/021/022) |
| 018 | NPC Engagement Coordination | 🤖 AI | 📝 Draft | 2026-02-09 | Yes (ADR-023/024) |

**Legend:**
- **Status:** ✅ Implemented (merged to main) | ✅ Approved (ready for implementation) | 🔄 Under Review | 📝 Draft/Proposed
- **Category:** ⚔️ Combat | 🌍 World | 🤖 AI | 🎨 UI | 🛠️ Dev Tools | 🌐 Network | 📊 Progression | 🧪 Testing | 🔧 Code Quality

---

## RFC Template

```markdown
# RFC-NNN: [Feature Name]

## Status

**[Draft / Under Review / Approved]** - YYYY-MM-DD

## Feature Request

### Player Need

From player perspective: **[One sentence summary]** - [Describe the problem from player's viewpoint]

**Current Problem:**
Without [feature]:
- [Specific pain point 1]
- [Specific pain point 2]
- [Specific pain point 3]

**We need a system that:**
- [Requirement 1]
- [Requirement 2]
- [Requirement 3]

### Desired Experience

Players should experience:
- **[Aspect 1]:** [Description]
- **[Aspect 2]:** [Description]
- **[Aspect 3]:** [Description]

### Specification Requirements

**[Feature Component 1]:**
- [Specific requirement]
- [Specific requirement]

**[Feature Component 2]:**
- [Specific requirement]
- [Specific requirement]

### MVP Scope

**Phase 1 includes:**
- [What's in scope]

**Phase 1 excludes:**
- [What's deferred]

### Priority Justification

**[HIGH / MEDIUM / LOW] PRIORITY** - [One line reason]

**Why [priority]:**
- [Justification 1]
- [Justification 2]

**Benefits:**
- [Benefit 1]
- [Benefit 2]

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: [Solution Name]**

#### Core Mechanism

[Describe how it works - formulas, data flow, key components]

#### Performance Projections

[Overhead estimates, development time]

#### Technical Risks

**1. [Risk Name]**
- *Risk:* [Description]
- *Mitigation:* [How to handle]
- *Impact:* [Severity assessment]

### System Integration

**Affected Systems:**
- [System 1]
- [System 2]

**Compatibility:**
- ✅ [Integration point 1]
- ✅ [Integration point 2]

### Alternatives Considered

#### Alternative 1: [Name]

[Description]

**Rejected because:**
- [Reason 1]
- [Reason 2]

---

## Discussion

### ARCHITECT Notes

[Key architectural insights, extensibility, technical observations]

### PLAYER Validation

[How this meets player needs, success criteria from spec]

---

## Approval

**Status:** [Approved / Needs Changes / Rejected]

**Approvers:**
- ARCHITECT: [✅/❌] [Comments]
- PLAYER: [✅/❌] [Comments]

**Scope Constraint:** Fits in one SOW ([X] hours/days)

**Dependencies:**
- [Dependency 1]
- [Dependency 2]

**Next Steps:**
1. [Action 1]
2. [Action 2]

**Date:** YYYY-MM-DD
```

---

## RFC Lifecycle

### 1. Creation (Draft Status)

**Who:** PLAYER role identifies a feature need

**Output:** `01-rfc/NNN-[feature].md` with Status: Draft

**Contains:**
- Feature Request section (player perspective)
- Specification Requirements (what we want)
- Priority Justification (why now)

### 2. Feasibility Analysis (Draft → Under Review)

**Who:** ARCHITECT role evaluates technical feasibility

**Adds to RFC:**
- Technical Assessment (can we build this?)
- System Integration (how does it fit?)
- Alternatives Considered (what else did we explore?)
- Risks and unknowns

**Output:** RFC updated with Status: Under Review

### 3. Discussion and Iteration (Under Review)

**Who:** PLAYER and ARCHITECT collaborate

**Happens in:** RFC's Discussion section

**Process:**
- PLAYER raises concerns about player experience
- ARCHITECT proposes solutions or adjustments
- Both refine until consensus reached

**Duration:** As long as needed (multiple rounds possible)

### 4. Approval (Under Review → Approved)

**Who:** Both PLAYER and ARCHITECT must approve

**Approval Criteria:**
- ✅ PLAYER: Solves the player need
- ✅ ARCHITECT: Feasible and maintainable
- ✅ Scope: Fits in one SOW (≤20 hours)
- ✅ No unresolved conflicts

**Output:** RFC updated with Status: Approved, frozen from further changes

### 5. ADR Extraction (If Applicable)

**Who:** ARCHITECT role

**Decision:** Does this RFC contain significant architectural decisions?

**Examples:**
- ✅ **Yes:** Projectile System (entities vs events) → ADR-015
- ✅ **Yes:** Movement Intent ("Intent then Confirmation" pattern) → ADR-016
- ❌ **No:** MVP Ability Set (just design choices, no ADR)
- ❌ **No:** Spatial Difficulty (just scope, no ADR)

**If Yes:** Create `02-adr/NNN-[decision].md` documenting the architectural choice

**If No:** Proceed directly to SOW creation

### 6. SOW Creation

**Who:** ARCHITECT role

**Output:** `03-sow/NNN-[feature].md` matching RFC number

**Contains:**
- Implementation plan (phases, deliverables)
- Acceptance criteria (how we know it's done)
- References to RFC (and ADR if applicable)

### 7. Lockdown (Approved Status)

**When:** Once approved, RFCs are frozen

**Why:** Preserve historical record of what was decided and why

**Changes After Approval:**
- RFC remains unchanged (locked)
- Implementation deviations documented in SOW Discussion section
- If major changes needed, create new RFC

---

## Writing Tips

### Feature Request Section

- **Start with player pain:** What's frustrating right now?
- **Use player language:** Avoid technical jargon in "Player Need"
- **Be specific:** "Combat feels same every time" not "needs variety"
- **Quantify when possible:** "All enemies level 10" not "uniform difficulty"

### Feasibility Analysis Section

- **Be honest:** If it's hard, say so and explain risks
- **Show your work:** Include formulas, data flow, key decisions
- **Estimate realistically:** Development time, performance overhead
- **Consider alternatives:** Why did we reject other approaches?

### Discussion Section

- **Document iteration:** Show how the design evolved
- **Capture insights:** Key realizations during analysis
- **Note trade-offs:** What did we sacrifice and why?

### Approval Section

- **Scope constraint is critical:** Must fit in one SOW (≤20 hours)
- **Dependencies matter:** What must exist first?
- **Clear next steps:** Who does what next?

---

## Common Patterns

### When to Split an RFC

If your RFC describes multiple independent features, split it:
- ❌ **Too broad:** "Combat System" (damage, abilities, AI, HUD)
- ✅ **Well-scoped:** "Damage Pipeline" (just damage calculation)

### When to Defer Features

Use "MVP Scope" section to defer complexity:
- **Phase 1 includes:** Core mechanic (must have)
- **Phase 1 excludes:** Polish, advanced features (nice to have)

### When ADR is Needed

Create ADR when RFC makes architectural choices:
- ✅ **Architecture:** Projectiles as entities (not events)
- ✅ **Architecture:** "Intent then Confirmation" pattern
- ❌ **Not architecture:** Which abilities to implement
- ❌ **Not architecture:** 100 tiles per level scaling

---

## Questions?

- **Where do RFCs come from?** Player pain points, spec gaps, technical debt
- **Who can create RFCs?** Anyone, but PLAYER role formalizes them
- **How long does review take?** As long as needed for consensus
- **Can approved RFCs change?** No - create new RFC instead
- **What if implementation deviates?** Document in SOW Discussion section
