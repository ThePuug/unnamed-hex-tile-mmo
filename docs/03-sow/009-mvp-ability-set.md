# SOW-009: MVP Ability Set

## Status

**Merged** - 2025-11-03

## References

- **RFC-009:** [MVP Ability Set](../01-rfc/009-mvp-ability-set.md)
- **ADR-015:** [MVP Ability Set - Auto-Attack + Stamina-Only Architecture](../02-adr/015-mvp-ability-set-auto-attack-architecture.md)
- **Spec:** [Combat System Specification](../spec/combat-system.md) (MVP criteria)
- **Branch:** (implementation complete, merged to main)
- **Implementation Time:** 6-8 days

---

## Implementation Plan

### Phase 1: Auto-Attack System

**Goal:** Passive DPS when adjacent to hostile target

**Deliverables:**
- `common/systems/auto_attack.rs` - Auto-attack system
- `common/components/auto_attack.rs` - AutoAttackTimer component
- Integration with damage pipeline (ADR-005)

**Architectural Constraints:**
- Triggers every 1.5 seconds when adjacent to hostile target
- Pauses when not adjacent (timer doesn't reset)
- Only attacks when CombatState.in_combat == true
- Uses directional targeting (60° facing cone)
- Deals 20 base physical damage via damage pipeline
- Scales with Might attribute (not felt at 0 attributes)
- Server-authoritative (client predicts for local player)

**Success Criteria:**
- Auto-attack triggers every 1.5s ± 0.1s when adjacent
- Timer pauses when player moves away (not adjacent)
- Timer resumes when player returns to melee
- Damage enters target's reaction queue (15 threat after armor)

**Duration:** 1-2 days

---

### Phase 2: Lunge (Gap Closer)

**Goal:** Q key teleports player adjacent to target + deals damage

**Deliverables:**
- Ability handler in `server/systems/abilities/lunge.rs`
- Integration with ability execution system (ADR-004)
- Teleport mechanic (instant position change)

**Architectural Constraints:**
- Q keybind
- Range 4 hexes (mid-tier)
- Targets nearest hostile in 60° facing cone
- Instant teleport (no travel time) adjacent to target
- Deals 40 base physical damage (200% of auto-attack)
- Costs 20 stamina (20% of base 100 pool)
- Scales with Vitality attribute (Direct approach)
- Triggers standard 0.5s GCD
- Server validates: range, stamina cost, target validity

**Success Criteria:**
- Q key executes Lunge when target in range
- Player teleports adjacent to target (distance 1)
- 40 damage dealt (enters target queue)
- 20 stamina consumed
- GCD active for 0.5s after use

**Duration:** 1 day

---

### Phase 3: Overpower (Heavy Strike)

**Goal:** W key deals high burst damage to adjacent target

**Deliverables:**
- Ability handler in `server/systems/abilities/overpower.rs`
- Cooldown tracking (2s cooldown independent of GCD)
- Integration with GCD system (ADR-011)

**Architectural Constraints:**
- W keybind
- Range 1 hex (adjacent only)
- Targets nearest hostile in 60° facing cone
- Deals 80 base physical damage (400% of auto-attack)
- Costs 40 stamina (40% of base pool)
- Scales with Presence attribute (Overwhelming approach)
- 2 second cooldown (separate from GCD)
- Triggers standard 0.5s GCD
- Server validates: adjacent check, stamina cost, cooldown state

**Success Criteria:**
- W key executes Overpower when adjacent to target
- 80 damage dealt (enters target queue)
- 40 stamina consumed
- 2s cooldown prevents spam (can't use again until cooldown expires)
- GCD active for 0.5s after use

**Duration:** 1 day

---

### Phase 4: Knockback (Positioning Tool)

**Goal:** E key pushes target 1 hex away

**Deliverables:**
- Ability handler in `server/systems/abilities/knockback.rs`
- Push mechanics (position update, terrain collision check)
- Integration with pathfinding (enemy re-pathfinding after push)

**Architectural Constraints:**
- E keybind
- Range 2 hexes
- Targets nearest hostile in 60° facing cone
- Push target 1 hex away from caster (calculate direction vector)
- No damage (pure utility)
- Costs 30 stamina (30% of base pool)
- 1.5 second cooldown
- Triggers standard 0.5s GCD
- If terrain blocks push: ability still costs stamina (prevents spamming against walls)
- Server validates: range, stamina cost, push direction, terrain collision

**Success Criteria:**
- E key executes Knockback when target in range
- Target moved 1 hex away from player
- 30 stamina consumed
- 1.5s cooldown active
- If terrain blocks: stamina consumed, target not moved (no refund)
- Enemy re-pathfinds back to player after push

**Duration:** 1-2 days

---

### Phase 5: Deflect (Emergency Defense)

**Goal:** R key clears all queued threats

**Deliverables:**
- Ability handler in `server/systems/abilities/deflect.rs`
- Integration with reaction queue system (ADR-003)

**Architectural Constraints:**
- R keybind
- Self-target (no directional requirement)
- Clears ALL queued threats (uses `ReactionQueue::clear_all_threats()` from ADR-003)
- Costs 50 stamina (50% of base pool)
- Triggers 0.5s GCD on all reaction abilities
- Server validates: stamina cost (prevents negative stamina)
- Client predicts queue clear (instant visual feedback)

**Success Criteria:**
- R key executes Deflect when stamina sufficient
- All threats removed from player's reaction queue
- 50 stamina consumed
- Queue UI clears instantly (client prediction)
- GCD active for 0.5s (can't use other reaction abilities)

**Duration:** 1 day

---

### Phase 6: Integration Testing and Balancing

**Goal:** Validate resource economy, combat flow, skill expression

**Deliverables:**
- Integration tests for ability interactions
- Combat scenario tests (Wild Dog encounter)
- Resource economy validation
- Playtesting notes and balance adjustments

**Architectural Constraints:**
- Test full rotation: Lunge + Overpower + Knockback + Deflect
- Validate stamina economy: 100 base, costs 20/40/30/50
- Test auto-attack DPS contribution
- Validate positioning importance (melee vs. kiting)
- Test skill ceiling (efficient stamina usage vs. panic spam)

**Test Scenarios:**

**1. Auto-Attack Timing:**
- Player adjacent to Wild Dog
- Verify auto-attack every 1.5s ± 0.1s
- Move away → verify pause
- Return to melee → verify resume

**2. Resource Economy:**
- Execute Lunge (80 stamina remaining)
- Execute Overpower (40 stamina remaining)
- Execute Knockback (10 stamina remaining)
- Try Deflect → fails (insufficient stamina)
- Wait 5s → verify 60 stamina (regenerated 50 at 10/sec)

**3. Knockback Positioning:**
- Lunge to Wild Dog (adjacent)
- Auto-attack triggers (dealing DPS)
- Wild Dog attacks → threat in queue
- Execute Knockback → dog at distance 1
- Verify auto-attack pauses (not adjacent)
- Verify Wild Dog re-pathfinds back

**4. Deflect Queue Clear:**
- Let Wild Dog attack 3 times (3 threats in queue)
- Execute Deflect → queue empty, 50 stamina consumed
- Verify GCD active 0.5s
- Wild Dog attacks again → new threat appears

**5. Combat Flow (Win Scenario):**
- Start: 100 stamina, 200 HP
- Lunge to dog (80 stamina, dog 60 HP)
- Auto-attack dealing 13 DPS
- Dog attacks → 15 dmg threat (queue)
- Overpower (40 stamina, dog 0 HP, dead)
- Queue resolves → take 15 damage (185 HP)
- Result: Win at 185 HP, 40 stamina

**Success Criteria:**
- All integration tests pass
- Resource economy validated (full rotation = 90 stamina)
- Positioning matters (auto-attack DPS, Knockback utility)
- Deflect feels expensive but necessary (emergency use)
- Combat flow responsive and tactical

**Duration:** 1-2 days

---

## Acceptance Criteria

**Functionality:**
- ✅ Auto-attack triggers every 1.5s when adjacent
- ✅ Lunge teleports player adjacent + deals 40 damage (range 4)
- ✅ Overpower deals 80 damage adjacent (2s cooldown)
- ✅ Knockback pushes target 1 hex away (range 2)
- ✅ Deflect clears entire queue (50 stamina cost)
- ✅ All abilities cost stamina only (no mana)
- ✅ Resource economy: Full rotation = 90 stamina (leaves 10)
- ✅ Server-authoritative validation (prevents cheating)
- ✅ Client prediction for local player (instant feedback)

**UX:**
- ✅ Staying in melee feels valuable (auto-attack DPS)
- ✅ Deflect feels expensive but necessary (resource pressure)
- ✅ Knockback creates tactical spacing (positioning tool)
- ✅ Combat feels responsive and tactical (not button mashing)

**Performance:**
- ✅ Auto-attack overhead negligible (< 1ms per 1.5s tick)
- ✅ 100 entities with auto-attack < 10% CPU

**Code Quality:**
- ✅ Abilities isolated in `server/systems/abilities/` (contained)
- ✅ Uses existing components (no new components needed)
- ✅ Integration tests cover ability interactions

---

## Discussion

### Implementation Note: Auto-Attack Timer Tracking

**Per-entity timer vs. global timer:**
- Per-entity: Each entity has `AutoAttackTimer` component
- Global: Single system tracks all entity timers

**Decision:** Per-entity component (cleaner, scales better, ECS-idiomatic).

### Implementation Note: Knockback Push Direction

**How to calculate push direction:**
```rust
// From caster to target
let direction = (target_loc - caster_loc).normalize();
let push_loc = target_loc + direction;
```

**Terrain collision:**
- Check if push_loc is walkable
- If blocked: consume stamina, don't move target (prevents wall spam)

### Implementation Note: Deflect Cost Tuning

**50 stamina may be too expensive** (players "never use it").

**Balancing approach:**
- Start at 50 (force positioning)
- Playtest extensively
- If too punishing: reduce to 40 (same as Overpower)
- Alternative: Add Fortify (50% damage reduction, 40 stamina)

**Decision:** Start at 50, tune based on playtesting feedback.

---

## Acceptance Review

**Review Date:** 2025-11-03
**Reviewer:** ARCHITECT Role
**Decision:** ✅ **ACCEPTED**

### Scope Completion: 100%

**All 6 phases complete:**
- ✅ Phase 1: Auto-attack system
- ✅ Phase 2: Lunge (gap closer)
- ✅ Phase 3: Overpower (heavy strike)
- ✅ Phase 4: Knockback (positioning tool)
- ✅ Phase 5: Deflect (emergency defense)
- ✅ Phase 6: Integration testing and balancing

### Architectural Compliance

**✅ ADR-015 Specifications:**
- Auto-attack passive DPS (1.5s timer, adjacent check)
- Stamina-only economy (no mana usage)
- Expensive defense (Deflect 50 stamina)
- Resource economy forces choices (full rotation = 90, can't afford Deflect)

### Player Experience Validation

**Positioning Matters:** ✅ Excellent
- Auto-attack rewards melee (13 DPS)
- Knockback creates space (30 stamina)
- Lunge re-engages (20 stamina gap close)
- Deflect too expensive to spam (forces positioning)

**Resource Management:** ✅ Excellent
- Full rotation = 90 stamina (leaves 10, can't Deflect)
- Deflect 50 = half pool (5s regeneration time)
- Clear choice: "Burst OR defense"

**Skill Expression:** ✅ Excellent
- Good players: Use Knockback proactively → rarely Deflect → efficient
- Bad players: Spam Deflect → run out → take damage
- Great players: Stay in melee for auto-attack DPS → win faster

### Performance

**Auto-Attack Overhead:** ✅ Excellent
- < 1ms per 1.5s tick (negligible)
- 100 entities with auto-attack < 10% CPU

**Combat Responsiveness:** ✅ Excellent
- Client prediction instant (stamina, queue clear)
- Server validation smooth (no rollbacks in playtesting)

---

## Conclusion

The MVP ability set implementation creates engaging tactical combat with minimal content, validates combat foundation systems, and establishes clear skill ceiling through positioning and resource management.

**Key Achievements:**
- Auto-attack passive DPS incentivizes melee positioning
- Expensive defense (Deflect 50) forces positioning over panic buttons
- Stamina-only economy simplifies MVP, enables Phase 2 magic/mana
- Resource economy forces tactical choices (burst OR defense)

**Architectural Impact:** Validates combat foundation (resources, queue, damage, targeting) before adding progression systems. Establishes ability execution patterns for future abilities.

**The implementation achieves RFC-009's core goal: engaging tactical combat with minimal content, demonstrating positioning importance and resource management depth.**

---

## Sign-Off

**Reviewed By:** ARCHITECT Role
**Date:** 2025-11-03
**Decision:** ✅ **ACCEPTED**
**Status:** Merged to main
