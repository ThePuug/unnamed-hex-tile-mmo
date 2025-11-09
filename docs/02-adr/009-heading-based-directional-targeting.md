# ADR-009: Heading-Based Directional Targeting and Ability Execution

## Status

Proposed

## Context

**Related RFC:** [RFC-004: Ability System and Directional Targeting](../01-rfc/004-ability-system-and-directional-targeting.md)

Combat system requires directional targeting: **"Face your enemies, position matters, no cursor required."**

### Requirements

- Keyboard-only targeting (no mouse/clicking)
- Automatic target selection (nearest in facing direction)
- Reuse existing Heading component (6 directions)
- Fast performance (every frame for local player)
- Server validation (prevent cheating)

### Options Considered

**Option 1: Click Targeting** - Mouse selects targets
- ❌ Requires cursor (violates spec)
- ❌ Not controller-friendly
- ❌ Slower combat flow

**Option 2: Tab Targeting** - TAB cycles through enemies
- ❌ No directional element
- ❌ Extra input burden
- ⚠️ Defer to Phase 2 as manual override

**Option 3: Heading-Based Auto-Selection** - Nearest in facing cone
- ✅ Keyboard-only (arrow keys turn, Q attacks)
- ✅ Reuses existing Heading component
- ✅ Fast (NNTree + angular filter)
- ✅ Deterministic (client/server identical)

## Decision

**Use heading-based directional targeting with automatic target selection (Option 3).**

### Core Mechanism

**Heading to Facing Cone:**
- Heading component: 6 directions (NE=30°, E=90°, SE=150°, SW=210°, W=270°, NW=330°)
- Facing cone: ±30° from heading angle (60° total)
- Angular check: `|target_angle - heading_angle| <= 30°`

**Target Selection Algorithm:**
1. Query entities within max range (NNTree spatial index)
2. Filter to actors in 60° facing cone
3. Select nearest by hex distance
4. Geometric tiebreaker: Closest to exact heading angle

**Example:**
```rust
pub fn select_target(
    caster_loc: Loc,
    caster_heading: Heading,
    nntree: &NNTree,
    query: &Query<(&EntityType, &Loc)>,
) -> Option<Entity> {
    // 1. Query nearby (20 hex radius)
    let nearby = nntree.locate_within_distance(caster_loc, 20);

    // 2. Filter to actors in facing cone
    let mut candidates = Vec::new();
    for ent in nearby {
        if let Ok((EntityType::Actor(_), loc)) = query.get(ent) {
            if is_in_facing_cone(caster_heading, caster_loc, *loc) {
                candidates.push((ent, *loc, caster_loc.distance(*loc)));
            }
        }
    }

    // 3. Select nearest, tiebreaker by angle
    candidates.sort_by_key(|(_, _, dist)| *dist);
    // ... geometric tiebreaker logic
}
```

### Ability Execution

**Ability Structure:**
```rust
pub enum AbilityType {
    BasicAttack,  // Single target, instant
    Dodge,        // Self target, clears queue
}

pub enum TargetingPattern {
    SingleTarget,  // Uses indicated target
    SelfTarget,    // No targeting required
}
```

**Client Prediction Flow:**
1. Player presses ability key (Q/E)
2. Client queries current target (select_target)
3. Client validates locally (range, resources)
4. Client predicts effects (spend stamina, apply damage)
5. Client sends `Try::UseAbility { ent, ability_type, target }`

**Server Validation:**
1. Recalculate target using select_target
2. Validate resources, GCD, target validity
3. Execute ability effects if valid
4. Broadcast `Do::AbilityUsed` or `Do::AbilityFailed`

**Key Principle:** Server recalculates target (doesn't trust client) but uses identical algorithm.

### Target Indicator (Client-Side)

**Red hostile indicator:**
- Runs every frame for local player
- Calls select_target to get current hostile
- Spawns/updates sprite at target position
- Despawns when no valid target

**Update frequency:** Every frame (~60fps) for smooth movement

---

## Rationale

### 1. Reuses Existing Heading Component

- Heading already exists (NE, E, SE, SW, W, NW)
- Movement systems update heading automatically
- No new infrastructure needed
- Seamless integration with controlled movement

### 2. Deterministic Shared Logic

- Client and server use identical `select_target` function
- Same inputs → same target (no desync)
- Pure function (easy to test, no hidden state)
- Located in `common/systems/targeting.rs`

### 3. Performance Efficient

- NNTree query: < 0.5ms for 100 entities
- Angular filtering: < 0.1ms (simple math)
- Only local player updates every frame (remote entities don't need indicators)
- Total: < 1ms per selection

### 4. Keyboard-Only Combat

- Arrow keys: Move and face
- Q/E keys: Cast abilities
- No mouse required (controller-ready)
- Faster than click-targeting (no extra input step)

### 5. Extensible Design

Foundation supports future features:
- Tier lock system (filter by range tiers)
- TAB cycling (manual target override)
- Complex patterns (Line, Radius, Adjacent)
- Flanking bonuses (target's heading matters)

---

## Consequences

### Positive

- **Keyboard-only:** Fully playable without mouse (accessibility)
- **Directional gameplay:** Positioning and facing matter
- **Automatic targeting:** Handles 90% of cases (nearest hostile)
- **Fast performance:** < 1ms target selection, 60fps indicator
- **Controller-ready:** Easy to port to gamepad
- **Deterministic:** Same on client and server

### Negative

- **No mouse flexibility:** Can't manually click specific targets
- **Heading discretization:** Only 6 directions (60° each)
- **Target mismatch potential:** Latency causes client/server to select different targets (rare)
- **Geometric tiebreaker ambiguity:** May not match player intent in edge cases

### Mitigations

- Phase 2 adds TAB cycling for manual control
- Geometric tiebreaker resolves equidistant targets consistently
- Server accepts client's target if "close enough" (latency tolerance)
- Visual feedback for "Invalid target" errors

---

## Alternatives Rejected

**Alternative: Continuous 360° Heading**
- Smoother rotation, more precision
- **Rejected:** Hex grid naturally discrete (6 edges), existing Heading component sufficient, diminishing returns (tiebreaker provides sub-60° precision)

**Alternative: Server Trusts Client's Target**
- Lower latency (no recalculation)
- **Rejected:** Cheating risk (client sends fake target), validation required for PvP

**Alternative: Fixed Targeting (No Auto-Select)**
- Require manual selection for every ability
- **Rejected:** High input burden, poor UX, violates "automatic targeting" spec

---

## Implementation Notes

**Shared code location:** `common/systems/targeting.rs`
- `select_target()` - Main algorithm
- `is_in_facing_cone()` - Angular check
- `get_range_tier()` - Distance classification (future: tier lock)

**Client-side:** `client/systems/targeting_ui.rs`
- Target indicator rendering
- Update every frame for local player

**Server-side:** `server/systems/abilities.rs`
- Ability execution with target validation
- Recalculates target using select_target

**Network messages:** `common/message.rs`
- `Event::UseAbility { ent, ability_type, target }`
- `Event::AbilityUsed { ent, ability_type, target }`
- `Event::AbilityFailed { ent, ability_type, reason }`

---

## Validation Criteria

**Functional:**
- Heading::E → 90°, facing cone ±30° (60° total)
- Target selection deterministic (same inputs → same target)
- Geometric tiebreaker resolves equidistant targets
- BasicAttack hits indicated target
- Dodge clears queue (no targeting)

**Performance:**
- Target selection: < 1ms for 100 entities
- Indicator update: 60fps maintained
- AI targeting: < 5% CPU for 100 Wild Dogs

**Network:**
- Client prediction within 100ms of server confirmation
- Target mismatch rare (< 1% of abilities)
- Rollback within 1 frame if ability fails

---

## References

- **RFC-004:** Ability System and Directional Targeting
- **Spec:** `docs/00-spec/combat-system.md` (directional combat design)
- **Codebase:** `src/common/components/heading.rs` (existing Heading component)
- **Related:** ADR-006/007/008 (reaction queue), ADR-002/004 (resources)

## Date

2025-10-30
