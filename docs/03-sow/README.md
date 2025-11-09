# Statements of Work (SOWs)

SOWs are work orders for developers. Each SOW defines what needs to be built, why it matters, and the constraints the implementation must satisfy - but NOT how to implement it.

## SOW Index

| # | Title | Category | Status | Created | Estimated | Actual |
|---|-------|----------|--------|---------|-----------|--------|
| 001 | Chunk-Based Terrain Discovery | 🌍 World/Terrain | ✅ Accepted | 2025-11-03 | 4-6 hours | - |
| 002 | Combat Foundation | ⚔️ Combat | ✅ Accepted | 2025-11-03 | 8-12 hours | - |
| 003 | Reaction Queue System | ⚔️ Combat | ✅ Accepted | 2025-11-03 | 6-9 hours | - |
| 004 | Ability System and Directional Targeting | ⚔️ Combat | 📝 Proposed | 2025-11-03 | 7-10 hours | - |
| 005 | Damage Pipeline | ⚔️ Combat | ✅ Accepted | 2025-11-03 | 8-12 hours | - |
| 006 | AI Behavior and Ability Integration | 🤖 AI | 📝 Proposed | 2025-11-03 | 8-11 hours | - |
| 007 | Developer Console | 🛠️ Dev Tools | ✅ Accepted | 2025-11-03 | 6-9 hours | - |
| 008 | Combat HUD | 🎨 UI/HUD | 📝 Proposed | 2025-11-03 | 17-23 days | - |
| 009 | MVP Ability Set | ⚔️ Combat | ✅ Accepted | 2025-11-03 | 6-8 days | - |
| 010 | Combat Variety Phase 1 | ⚔️ Combat | 📝 Proposed | 2025-11-03 | 7-11 days | - |
| 011 | Movement Intent System | 🌐 Network | 📝 Proposed | 2025-11-05 | 6-9 days | - |
| 012 | Ability Recovery and Synergies | ⚔️ Combat | ✅ Accepted | 2025-11-07 | 5-8 days | - |
| 013 | Ability Execution Pipeline | 🔧 Code Quality | 📝 Proposed | 2025-11-07 | 3-4 days | - |
| 014 | Spatial Difficulty System | 📊 Progression | ✅ Merged | 2025-11-07 | 6.5-9.5 hours | ~8 hours |
| 015 | Architectural Invariant Testing | 🧪 Testing | ✅ Merged | 2025-11-08 | 2.5 days | ~2 days |

**Legend:**
- **Status:** ✅ Accepted/Merged | 🔄 In Progress/Review | 📝 Planned/Proposed
- **Category:** ⚔️ Combat | 🌍 World | 🤖 AI | 🎨 UI | 🛠️ Dev Tools | 🌐 Network | 📊 Progression | 🧪 Testing | 🔧 Code Quality

---

## SOW Template

```markdown
# SOW-NNN: [Feature Name]

## Status

**[Planned / In Progress / Review / Approved / Merged]** - YYYY-MM-DD

## References

- **RFC-NNN:** [Feature Name](../01-rfc/NNN-feature.md)
- **ADR-NNN:** [Decision Name](../02-adr/NNN-decision.md) (if applicable)
- **Spec:** [Spec Reference](../00-spec/system.md) (if applicable)
- **Branch:** [branch-name / (proposed) / (merged)]
- **Implementation Time:** [X-Y hours/days]

---

## Implementation Plan

### Phase 1: [Phase Name]

**Goal:** [One sentence describing what this phase achieves]

**Deliverables:**
- [Specific file/component 1]
- [Specific file/component 2]
- [Specific file/component 3]

**Architectural Constraints:**
- [Constraint 1 - WHAT must be true, not HOW to do it]
- [Constraint 2 - Performance/integration requirements]
- [Constraint 3 - System boundaries/interfaces]
- [Constraint 4 - Data structures/formats]

**Success Criteria:**
- [Testable outcome 1]
- [Testable outcome 2]
- [Testable outcome 3]

**Duration:** [X hours/days]

---

### Phase 2: [Phase Name]

[Repeat structure for each phase]

---

## Acceptance Criteria

**Functional:**
- [All features work as specified]
- [Edge cases handled correctly]

**UX:**
- [Player-facing quality metrics]
- [No regressions in existing features]

**Performance:**
- [Overhead measurements]
- [Scalability requirements]

**Code Quality:**
- [Test coverage requirements]
- [Documentation completeness]
- [Code organization standards]

---

## Discussion

*This section is populated during implementation with questions, decisions, and deviations.*

### Implementation Note: [Topic]

[Document decisions made during implementation, deviations from plan, and rationale]

---

## Acceptance Review

*This section is populated after implementation is complete.*

### Scope Completion: [X%]

**Phases Complete:**
- ✅ Phase 1: [Name]
- ✅ Phase 2: [Name]
- ⏸️ Phase 3: [Name] (deferred to post-MVP)

### Architectural Compliance

[Assessment of whether implementation follows ADR specifications]

### Player Experience Validation

[Assessment from PLAYER role perspective]

### Performance

[Actual measurements vs. targets]

---

## Conclusion

[Summary of what was achieved, impact, and next steps]

---

## Sign-Off

**Reviewed By:** [ARCHITECT Role]
**Date:** YYYY-MM-DD
**Decision:** ✅ **[ACCEPTED / NEEDS CHANGES / REJECTED]**
**Status:** [Merged to main / Needs revision]
```

---

## SOW Lifecycle

### 1. Creation (Planned Status)

**Who:** ARCHITECT role creates from approved RFC

**Output:** `03-sow/NNN-[feature].md` matching RFC number

**Contains:**
- Implementation Plan (phases with deliverables)
- Architectural Constraints (what/why/constraints, NOT how)
- Acceptance Criteria (how we know it's done)

**Key Principle:** SOWs are **descriptive** (what to build), not **prescriptive** (how to build it)

### 2. Implementation Begins (Planned → In Progress)

**Who:** DEVELOPER role starts work

**Process:**
- Developer reads SOW phases sequentially
- Works through deliverables autonomously
- Has freedom to choose implementation approaches within constraints

**Status Update:** Change to "In Progress" when first commit made

**Feature Matrix Update:** Mark feature as "In Progress"

### 3. Discussion Updates (During In Progress)

**Who:** DEVELOPER role documents as work proceeds

**Adds to SOW:**
- Implementation questions and answers
- Decisions made during development
- Deviations from plan with rationale
- Discoveries that affect approach

**Location:** Discussion section of SOW

**Example:**
```markdown
### Implementation Note: Counter Ability Timing

Initially planned Counter to trigger on any queue entry, but discovered
this allows spamming. Changed to require front-of-queue only, ensuring
defensive timing matters. This aligns with ADR-003 FIFO processing.
```

### 4. Implementation Complete (In Progress → Review)

**Who:** DEVELOPER role finishes all phases

**Triggers:**
- All deliverables implemented
- Tests passing
- Branch ready for review

**Status Update:** Change to "Review"

**Next Step:** ARCHITECT reviews implementation

### 5. Acceptance Review (Review → Approved/Needs Changes)

**Who:** ARCHITECT role reviews against acceptance criteria

**Adds to SOW:**
- Scope Completion assessment
- Architectural Compliance check
- Player Experience Validation (PLAYER role input)
- Performance measurements
- Final sign-off decision

**Location:** Acceptance Review section of SOW

**Outcomes:**
- ✅ **Approved:** Ready to merge
- 🔄 **Needs Changes:** Specific revisions required
- ❌ **RFC Revision Required:** Implementation revealed RFC was infeasible

### 6. Merge and Lockdown (Approved → Merged)

**Who:** ARCHITECT or DEVELOPER merges branch

**Process:**
1. Merge branch to main
2. Update SOW status to "Merged"
3. Update feature matrix (mark complete, link to SOW)
4. SOW is now frozen (historical record)

**Status Update:** Change to "Merged", add merge date

**Feature Matrix Update:** Mark feature as "Complete"

### 7. Post-Merge (Merged Status)

**SOW is locked:** No further changes (historical record)

**If issues found:**
- Bug fixes: Direct commits to main (no SOW update)
- Design changes: Create new RFC + SOW
- Spec deviations: Document in feature matrix

---

## Writing Tips

### Implementation Plan

**Good Phase Structure:**
- Clear goal (one sentence)
- Specific deliverables (files, components)
- **Constraints, not instructions** (what must be true, not how)
- Testable success criteria

**Example - Descriptive (Good):**
```markdown
**Architectural Constraints:**
- Lockout durations: Lunge 1.0s, Overpower 2.0s, Knockback 0.5s
- Single GlobalRecovery component per player (not per-ability)
- Synergy detection runs AFTER lockout insertion (needs GlobalRecovery)
```

**Example - Prescriptive (Bad):**
```markdown
**Implementation Steps:**
1. Create GlobalRecovery struct with these fields...
2. In lunge.rs, add this code: `commands.insert(GlobalRecovery {...})`
3. Loop through all abilities and check...
```

**Why Descriptive Wins:**
- Developer has autonomy (can choose best approach)
- Constraints define correctness (implementation is flexible)
- Easier to maintain (doesn't prescribe every detail)

### Architectural Constraints

**Focus on:**
- Performance requirements ("< 0.1ms per ability use")
- Integration points ("Uses existing ReactionQueue from ADR-003")
- Data formats ("MovementIntent { dest: Qrz, duration_ms: u16, seq: u8 }")
- System boundaries ("Server authoritative, client predicts for local player")
- Timing/ordering ("Detection runs AFTER lockout, BEFORE UI update")

**Avoid:**
- Code snippets (let developer write it)
- Step-by-step instructions (trust developer autonomy)
- Implementation details ("use HashMap" vs "fast lookup required")

### Success Criteria

**Make it testable:**
- ✅ "Level 5 enemies spawn at 500-599 tiles from origin"
- ❌ "Enemies have correct level"

**Be specific:**
- ✅ "Engagement despawns after 60s with no players within 100 tiles"
- ❌ "Cleanup works correctly"

**Cover edge cases:**
- ✅ "Counter fails if ReactionQueue empty (validation working)"
- ❌ "Counter ability works"

### Discussion Section

**Document during implementation:**
- Design decisions made on-the-fly
- Trade-offs discovered during coding
- Deviations from original plan
- Bugs found and how they were fixed

**Format:**
```markdown
### Implementation Note: [Topic]

[Context: What we discovered]
[Decision: What we chose to do]
[Rationale: Why we made that choice]
```

---

## Common Patterns

### When to Update SOW During Implementation

**Update Discussion section when:**
- ✅ You discover a better approach than planned
- ✅ You find a constraint that wasn't documented
- ✅ You deviate from the plan (with good reason)
- ✅ You make a design decision the next developer should know

**Don't update for:**
- ❌ Implementation details (code-level choices)
- ❌ Bug fixes during development (expected)
- ❌ Refactoring within same phase (internal)

### When to Split Phases

**Split phases when:**
- Dependencies exist (Phase 2 needs Phase 1 complete)
- Natural checkpoints (infrastructure → feature → polish)
- Duration > 2 days (break into smaller units)

**Keep together when:**
- Tightly coupled (can't test one without other)
- Short duration (< 4 hours total)

### When to Defer to Post-MVP

Use "excludes" in Implementation Plan:
```markdown
**Phase 1 excludes:**
- Data-driven synergies (hardcoded MVP rules only - Phase 2)
- Multiple synergy sources (one per ability - Phase 2)
```

Then document in Acceptance Review if actually deferred.

---

## Questions?

- **Can I deviate from the SOW?** Yes, document in Discussion with rationale
- **What if I find a better approach?** Use it, explain why in Discussion
- **How detailed should phases be?** Enough for another dev to understand constraints
- **When do I update feature matrix?** At status transitions (Planned → In Progress → Complete)
- **Can SOWs change after merge?** No - they're historical records
