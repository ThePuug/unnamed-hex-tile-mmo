# RFC-009: MVP Ability Set

## Status

**Implemented** - 2025-11-03

## Feature Request

### Player Need

From player perspective: **Engaging tactical combat with minimal content** - Feel skilled through positioning and resource management, not button complexity.

**Current Problem:**
Without defined MVP ability set:
- Spec shows many potential abilities (Charge, Dodge, Counter, Parry, Fortify, Ward, etc.)
- Unclear which subset creates minimal viable tactical experience
- No progression systems yet (attributes at 0, no Triumvirate selection)
- Limited enemy variety (Wild Dog only)
- Risk of scope creep (implementing too many abilities before validating core systems)

**We need a system that:**
- Creates engaging combat with 4-5 abilities (not 20+)
- Teaches positioning importance (not just button mashing)
- Forces resource management decisions (meaningful stamina costs)
- Tests reaction queue system under pressure
- Validates combat foundation before adding progression

### Desired Experience

Players should experience:
- **Positioning Matters:** Staying in melee = value (auto-attack DPS), kiting = safety (no free damage)
- **Resource Decisions:** "Burst now (Lunge + Overpower) OR save stamina for defense (Deflect)?"
- **Skill Expression:** Good positioning → rarely need expensive defense → efficient stamina usage
- **Tactical Depth:** Simple enemy AI, but combat feels thoughtful (not spam fest)
- **System Validation:** Combat foundation (resources, queue, damage) tested under realistic use

### Specification Requirements

**MVP Ability Set (5 abilities):**

**1. Auto-Attack (Passive):**
- Triggers every 1.5s when adjacent to hostile target
- 20 physical damage (scales with Might)
- Free (no cost)
- Incentivizes melee engagement (staying close = DPS)

**2. Lunge (Q - Gap Closer):**
- Range 4 hexes
- Teleport adjacent to target + 40 damage
- Costs 20 stamina (cheap, frequent use)
- Scales with Vitality (Direct approach)

**3. Overpower (W - Heavy Strike):**
- Range 1 hex (adjacent only)
- 80 physical damage (highest burst)
- Costs 40 stamina (expensive)
- 2s cooldown
- Scales with Presence (Overwhelming approach)

**4. Knockback (E - Positioning Tool):**
- Range 2 hexes
- Push target 1 hex away (no damage)
- Costs 30 stamina
- 1.5s cooldown
- Creates space without clearing queue

**5. Deflect (R - Emergency Defense):**
- Self-target
- Clears ALL queued threats
- Costs 50 stamina (very expensive, 50% of base pool)
- 0.5s GCD
- Forces positioning as primary defense

**Resource Economy:**
- Base stamina pool: 100 (at 0 attributes)
- Stamina regen: 10/sec
- Full rotation (Lunge + Overpower + Knockback) = 90 stamina (leaves 10, can't afford Deflect)
- Forces choice: "All-in burst OR save for defense"

**All abilities cost stamina only** (no mana in MVP)

### MVP Scope

**Phase 1 includes:**
- Auto-attack system (1.5s timer, adjacent check, pause when not adjacent)
- 4 active abilities (Lunge, Overpower, Knockback, Deflect)
- Stamina-only resource economy
- Integration with existing systems (damage pipeline, reaction queue, targeting)

**Phase 1 excludes:**
- Mana-based abilities (Ward, Volley - Phase 2)
- Magic damage type (physical only)
- Attribute progression (all players at 0 attributes)
- Triumvirate selection UI (build identity - Phase 3)
- Additional damage abilities (Trap, Mark, Counter - Phase 2)

### Priority Justification

**CRITICAL** - Blocks MVP combat playability. Without ability set, combat system cannot be tested.

**Why critical:**
- Validates combat foundation (resources, queue, damage) under realistic use
- Tests reaction queue under sustained pressure (auto-attack + enemy attacks)
- Demonstrates positioning importance (auto-attack incentive, Knockback utility, expensive Deflect)
- Creates skill ceiling (efficient stamina usage vs. panic Deflect spam)

**Benefits:**
- Faster iteration (5 abilities vs. 20+ from spec)
- Clearer testing (validates core systems before adding complexity)
- Skill expression (good positioning → efficient resources → win faster)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Stamina-Only, Auto-Attack Focused Combat**

#### Core Mechanism

**Design Philosophy:**

**1. Auto-Attack as Passive DPS:**
- Incentivizes melee engagement (free damage when adjacent)
- Reduces button mashing (no manual basic attack spam)
- Creates positioning incentive (staying close = value, kiting = safety)
- Frees up Q key for gap closer (Lunge)

**2. Expensive Defense (Deflect at 50 stamina):**
- Forces positioning as primary defense (can't spam Deflect)
- Creates resource pressure (50 stamina = 5s regeneration time)
- Removes cheap panic button (no Dodge at 30 stamina)
- Rewards skill (good positioning → rarely need Deflect)

**3. Stamina-Only Economy:**
- Simplifies MVP (no mana pool UI, no magic damage)
- All abilities use same resource (easier to balance)
- Can add mana/magic in Phase 2 (Ward, Volley)

**Integration with Existing Systems:**

**ADR-002 (Combat Foundation):**
- All abilities consume Stamina component
- Server validates stamina cost before execution
- Client predicts stamina reduction (instant feedback)
- CombatState triggers auto-attack when in_combat == true

**ADR-003 (Reaction Queue):**
- Deflect uses existing `clear_all_threats()` function
- Triggers 0.5s GCD on reaction abilities
- Knockback creates space (doesn't clear queue)

**ADR-004 (Ability System):**
- Lunge/Overpower/Knockback use directional targeting (60° cone)
- Range validation (1/2/4 hexes)
- Instant execution pattern (no projectiles)

**ADR-005 (Damage Pipeline):**
- Auto-attack: 20 base × (1.0 + might/100) × (1.0 - armor)
- Lunge: 40 base × (1.0 + vitality/100) × (1.0 - armor)
- Overpower: 80 base × (1.0 + presence/100) × (1.0 - armor)

**New Systems Required:**

**1. Auto-Attack System:**
- Query entities with (Stamina, CombatState, Heading, Loc)
- Filter to in_combat == true
- Every 1.5s, if adjacent hostile exists: deal 20 damage, reset timer
- If not adjacent: pause timer

**2. Ability Execution Handlers:**
- Lunge: Validate range → teleport → deal damage
- Overpower: Validate adjacent → deal damage → trigger 2s cooldown
- Knockback: Validate range → calculate push direction → move target
- Deflect: Clear queue → consume stamina → trigger GCD

#### Performance Projections

**Auto-Attack Overhead:**
- Query all in_combat entities every 1.5s (not every frame)
- Timer check + adjacent check + damage calculation
- Scales with entity count (100 entities = 100 checks per 1.5s)
- Negligible CPU cost (< 1ms per tick)

**Development Time:**
- Phase 1 (MVP): 6-8 days (5 abilities + auto-attack system + integration)

#### Technical Risks

**1. Auto-Attack Complexity**
- *Risk:* Timer tracking per entity, "pause when not adjacent" logic
- *Mitigation:* Timer tracking already exists (GCD, regen), reuse patterns
- *Frequency:* One-time implementation, well-isolated system

**2. Deflect May Feel Too Expensive**
- *Risk:* 50 stamina (50% pool) → players "never use it" (hoarding)
- *Mitigation:* Playtesting, willing to reduce to 40 if too punishing
- *Impact:* Balancing issue, not technical blocker

**3. Knockback Push Mechanics**
- *Risk:* Requires collision detection (terrain blocks push)
- *Mitigation:* Existing pathfinding has terrain data, check before push
- *Frequency:* One-time implementation, edge case handling

**4. Attribute Scaling Won't Be Felt**
- *Risk:* All players at 0 attributes → no damage scaling differences
- *Mitigation:* Intentional (MVP tests base values, progression adds scaling)
- *Impact:* Attribute system value not demonstrated until Phase 3

### System Integration

**Affected Systems:**
- Combat foundation (Stamina consumption, CombatState triggers)
- Reaction queue (Deflect clears queue)
- Targeting (Lunge/Overpower/Knockback range validation)
- Damage pipeline (auto-attack, ability damage)
- AI behavior (enemy re-pathfinding after Knockback)

**Compatibility:**
- ✅ Uses existing components (Stamina, ReactionQueue, Heading, Loc)
- ✅ No new components needed
- ✅ Extends existing systems (ability execution, GCD)
- ✅ Client prediction for stamina (instant feedback)

### Alternatives Considered

#### Alternative 1: Manual Basic Attack (No Auto-Attack)

Q key = basic attack (20 damage, free), no passive DPS.

**Rejected because:**
- Incentivizes button mashing (spam Q for DPS)
- No positioning incentive (hit once, kite, repeat)
- Wastes Q key on boring ability (gap closer more interesting)

#### Alternative 2: Cheap Dodge (30 stamina) + Expensive Deflect (50 stamina)

Both Dodge and Deflect available, different costs.

**Rejected because:**
- Dodge at 30 stamina becomes "always correct answer" (panic button)
- Positioning importance reduced (Dodge solves everything)
- Resource pressure low (can Dodge 5 times)
- Skill expression minimal (spam Dodge, no planning needed)

#### Alternative 3: Multiple Damage Types (Physical + Magic)

Include Ward (magic defense), Volley (magic damage) in MVP.

**Rejected for MVP because:**
- Requires mana pool implementation
- Requires magic damage pipeline
- Adds complexity before validating core systems
- Defer to Phase 2 (magic damage + mana)

#### Alternative 4: 10+ Abilities (Larger Set)

Implement many abilities from Triumvirate spec (Charge, Parry, Counter, Trap, etc.).

**Rejected because:**
- Scope creep (too much to implement/balance)
- No progression system yet (can't unlock abilities)
- MVP should validate core, not explore variety
- Phase 2+ adds more abilities after validation

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Auto-attack passive DPS + expensive defense (Deflect 50) forces positioning as primary tactic. Cheap panic buttons (Dodge 30) reduce skill expression.

**Resource economy critical:** Full rotation (90 stamina) leaves 10 (can't afford Deflect). Forces "burst OR defense" choice. Regeneration time (5-10s) creates pacing.

**Stamina-only simplification:** All abilities same resource → easier balancing, clearer resource pressure. Mana adds complexity (Phase 2) after core validated.

**Extensibility:**
- Phase 2: Add Ward (magic defense, mana cost), Volley (ranged, mana cost)
- Phase 3: Attribute progression (damage scaling felt)
- Phase 4: Triumvirate selection (build identity)

### PLAYER Validation

**From combat-system.md spec:**

**Success Criteria:**
- ✅ Playable combat loop with Wild Dog enemy
- ✅ Positioning matters (auto-attack DPS, Knockback spacing, expensive Deflect)
- ✅ Resource management creates decisions (Deflect 50 = half pool)
- ✅ Reaction queue functional (Deflect clears, auto-attack fills)
- ✅ Combat feels responsive and tactical (instant stamina feedback, meaningful choices)

**Positioning Validation:**
- Auto-attack: Staying in melee = free 20 dmg/1.5s (13 DPS)
- Knockback: Creates space when pressured (30 stamina vs. 50 Deflect)
- Lunge: Re-engages to melee (20 stamina gap close)
- Deflect expensive: Must position to avoid needing it (50 stamina = 5s regen)

**Skill Expression:**
- Good players: Use Knockback proactively → rarely Deflect → stamina efficient
- Bad players: Spam Deflect → run out → take more damage
- Great players: Stay in melee (auto-attack DPS) → win faster

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Clean design, validates core systems, extensible to Phase 2+
- PLAYER: ✅ Solves MVP needs, positioning matters, skill ceiling exists

**Scope Constraint:** Fits in one SOW (6-8 days for 5 abilities + auto-attack)

**Dependencies:**
- ADR-002: Stamina component, CombatState component
- ADR-003: ReactionQueue component, clear_all_threats()
- ADR-004: Directional targeting, ability execution
- ADR-005: Damage pipeline, attribute scaling
- ADR-011: GCD component (cooldown tracking)

**Next Steps:**
1. ARCHITECT creates ADR-015 documenting auto-attack + stamina-only architecture
2. ARCHITECT creates SOW-009 with 6-phase implementation plan
3. DEVELOPER begins Phase 1 (auto-attack system)

**Date:** 2025-11-03
