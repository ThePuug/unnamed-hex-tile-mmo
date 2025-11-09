# RFC-010: Combat Variety Phase 1

## Status

**Implemented** - 2025-11-03

## Feature Request

### Player Need

From player perspective: **Combat variety and tactical depth** - Encounters should feel different, positioning should matter beyond "stay in melee or kite."

**Current Problem:**
Without combat variety:
- Only one enemy type (Wild Dog - melee aggressor)
- No ranged combat patterns (all enemies melee)
- Can't easily target distant enemies when multiple enemies nearby (tier lock framework exists but not bound to keys)
- All players move at identical speed (Grace attribute feels useless)
- Combat encounters feel repetitive (same enemy, same tactics)

**We need a system that:**
- Adds ranged enemy threat variety (Forest Sprite with kiting AI)
- Completes tier lock targeting (1/2/3 keys to select range tiers)
- Makes Grace attribute valuable (movement speed scaling)
- Creates emergent tactical scenarios (ranged + melee mixed encounters)
- Tests core systems before adding more content

### Desired Experience

Players should experience:
- **Variety:** Different enemies require different tactics (can't use same rotation every fight)
- **Targeting Control:** Easy to select distant ranged threats (tier lock 3, target far enemy)
- **Attribute Value:** Grace immediately felt (faster movement, better kiting)
- **Skill Expression:** Dodging projectiles, positioning against mixed threats
- **Tactical Decisions:** "Kill ranged first (tier lock 3) or melee threat (auto-target)?"

### Specification Requirements

**MVP Combat Variety (3 features):**

**1. Tier Lock Targeting (1/2/3 Keys):**
- 1 key: Lock to Close tier (1-2 hexes)
- 2 key: Lock to Mid tier (3-6 hexes)
- 3 key: Lock to Far tier (7+ hexes)
- Lock drops after 1 ability use (returns to automatic)
- Empty tier: Shows range visualization, switches when enemy enters
- Visual: Tier badge on indicator ("1", "2", "3"), empty tier cone highlight

**2. Forest Sprite (Ranged Enemy):**
- HP: 80 (glass cannon vs. Wild Dog's 100)
- Damage: 20 physical (higher than Wild Dog's 15)
- Attack speed: 3s (slower than Wild Dog's 1s)
- Aggro range: 15 hexes (longer than Wild Dog's 10)
- Optimal distance: 5-8 hexes (kiting zone)
- AI behavior: Kites away if player closer than 3 hexes, attacks with projectiles
- Projectile travel time: ~1-2s (dodgeable)
- Spawn: 40% of enemy spawns (mixed with Wild Dogs 60%)

**3. Movement Speed (Grace Scaling):**
- Formula: `movement_speed = max(75, 100 + (grace / 2))`
- Grace 0: speed 100 (baseline)
- Grace 100: speed 150 (+50% bonus)
- Grace -100: speed 75 (clamped at -25% penalty)
- Applied to movement timing in FixedUpdate
- Creates mobility trade-offs (Might specialists slower, Grace specialists faster)

### MVP Scope

**Phase 1 includes:**
- Tier lock completion (1/2/3 keybinds, state management, UI)
- Projectile system (entity-based, travel time, collision detection)
- Forest Sprite enemy (kiting AI, projectile attacks, spawn integration)
- Movement speed scaling (Grace formula, applied to all movement)
- Mixed encounters (Sprite + Dogs test tactical variety)

**Phase 1 excludes:**
- TAB cycling (manual target selection - Phase 2)
- Player ranged abilities (Volley, Ward - Phase 2)
- Additional enemy types (bosses, elites - Phase 2+)
- Movement speed visual feedback (dust particles, speed lines - polish)

### Priority Justification

**HIGH PRIORITY** - Improves combat feel, validates systems, enables future content.

**Why high priority:**
- Tier lock: Solves targeting frustration (can't select distant enemies)
- Ranged enemies: Creates combat variety (all melee currently)
- Movement speed: Makes Grace attribute valuable (currently useless)
- Validates systems: Tests projectiles, kiting AI, targeting before adding more

**Benefits:**
- Tactical depth (mixed encounters require adaptation)
- Attribute value (Grace immediately felt)
- System validation (projectiles, tier lock, kiting AI tested)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Three Interconnected Features**

#### Core Mechanism

**Feature 1: Tier Lock Targeting**

```rust
pub struct TargetingState {
    pub mode: TargetingMode,
}

pub enum TargetingMode {
    Automatic,                    // Default: nearest in facing direction
    TierLocked(RangeTier),       // Locked to specific tier until ability use
}

pub enum RangeTier {
    Close,   // 1-2 hexes
    Mid,     // 3-6 hexes
    Far,     // 7+ hexes
}

// State transition:
// Default → Press 1/2/3 → Tier Locked
// Tier Locked → Use Ability → Default
```

**Integration:** Extends ADR-004 targeting framework with number key bindings.

**Feature 2: Forest Sprite (Ranged Enemy)**

**AI Behavior:**
- If distance < 3 hexes: FLEE (move away, maintain facing)
- If distance 3-5 hexes: REPOSITION (move to 6-7 hex range)
- If distance 5-8 hexes: ATTACK (projectile every 3s)
- If distance > 8 hexes: ADVANCE (move closer, maintain facing)

**Projectile Mechanics:**
- Entity-based (not event-based)
- Travel speed: 4 hexes/second
- Dodgeable: Player can move off hex during travel
- Server-authoritative position

**Feature 3: Movement Speed**

**Formula (from attribute-system.md):**
```rust
let movement_speed = max(75, 100 + (grace / 2));
```

**Implementation:**
- Scale movement delta in FixedUpdate
- `scaled_velocity = base_velocity * movement_speed_multiplier`
- Maintains fixed update timing (deterministic physics)

#### Performance Projections

**Tier Lock:**
- Component state per player (minimal overhead)
- State transitions on keypress + ability use (rare events)
- UI rendering: 1 badge + optional cone highlight (negligible)

**Projectiles:**
- Entity count: ~5-10 active projectiles in typical combat
- Update system runs in FixedUpdate (physics tick)
- Collision detection: Hex-based (simple, fast)
- Overhead: < 1ms per projectile

**Movement Speed:**
- Multiplier applied in existing movement system
- Per-entity calculation from Grace attribute
- Overhead: negligible (single multiply operation)

**Development Time:**
- Phase 1 (MVP): 7-11 days (1.5-2 weeks)

#### Technical Risks

**1. Projectile System Complexity**
- *Risk:* Entity lifecycle management, collision detection, network sync
- *Mitigation:* Start with simple hex-based collision, use existing entity patterns
- *Frequency:* One-time implementation, reusable for future projectile abilities

**2. Kiting AI Complexity**
- *Risk:* Inverse pathfinding (move away), maintaining LoS, opportunistic attacks
- *Mitigation:* Simple distance threshold (if too close, move 1-2 hexes away), reuse existing pathfinding
- *Frequency:* One-time implementation per kiting behavior variant

**3. Movement Speed Balancing**
- *Risk:* Grace 100 might be "unkitable," Grace -100 might feel unplayable
- *Mitigation:* Playtest extensively, adjust formula if needed, cap Grace at ±50 for MVP
- *Impact:* Balancing issue, not technical blocker

**4. Tier Lock Learning Curve**
- *Risk:* New players might not understand (no tutorial)
- *Mitigation:* Add tutorial pop-up, make keybindings configurable, document clearly
- *Impact:* UX issue, not technical blocker

### System Integration

**Affected Systems:**
- Targeting system (tier lock state, range filtering)
- AI system (kiting behavior, projectile attacks)
- Movement system (Grace scaling)
- Spawning system (Forest Sprite entity type)
- Rendering (projectile sprites, tier lock UI)

**Compatibility:**
- ✅ Extends ADR-004 targeting framework (tier lock completes it)
- ✅ Uses existing damage pipeline (projectiles enter reaction queue)
- ✅ Uses existing entity spawning (projectiles are entities)
- ✅ Uses existing attribute system (Grace formula from spec)

### Alternatives Considered

#### Alternative 1: Events Instead of Entities (Projectiles)

Projectiles as instant events, no travel time.

**Rejected because:**
- No visual representation (projectiles "teleport" on hit)
- Harder to dodge (no reaction time)
- Violates "conscious but decisive" design pillar (no time to react)

#### Alternative 2: TAB Cycling Instead of Tier Lock

Manual target selection instead of range tier locking.

**Rejected for MVP because:**
- More complex (cycle through all entities)
- Slower (multiple keypresses to reach desired target)
- Tier lock solves 80% of use cases (target distant enemies)
- TAB cycling deferred to Phase 2

#### Alternative 3: Variable Fixed Update Rate (Movement Speed)

Faster entities run FixedUpdate more frequently.

**Rejected because:**
- Massive complexity (per-entity schedules)
- Breaks physics determinism (different tick rates = different collisions)
- Over-engineered for simple movement speed scaling

#### Alternative 4: Separate Biomes (Forest Sprites)

Forest Sprites only in forest chunks, Wild Dogs in plains.

**Rejected because:**
- Delays testing tier lock (players might avoid forests)
- Less variety in encounters (can't have mixed fights)
- Mixed spawns create emergent scenarios (Sprite + Dogs)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Projectiles as entities (not events) enables dodging, visual feedback, and travel time - critical for "conscious but decisive" combat. Sets pattern for all future projectile abilities.

**Tier lock completes targeting:** ADR-004 framework is now fully usable. Number keys solve 80% of targeting issues (distant enemies). TAB cycling deferred to Phase 2.

**Movement speed foundational:** Grace scaling affects all movement. Creates attribute value (Grace immediately felt). Enables future features (dodge recovery, hit chance).

**Extensibility:**
- Future: Player ranged abilities (Volley) reuse projectile system
- Future: More enemy types (ranged mage, archer) reuse kiting AI
- Future: Movement abilities (Charge, Leap) scale with movement speed

### PLAYER Validation

**From combat-system.md spec:**

**Success Criteria:**
- ✅ Tier lock targeting (1/2/3 keys) - Lines 85-106
- ✅ Ranged enemy (Forest Sprite) - Lines 482-490
- ✅ Projectile attacks with travel time - Lines 151-161
- ✅ Movement speed scales with Grace - attribute-system.md Lines 338-347

**Variety Validation:**
- Mixed encounters (Sprite + Dogs) require different tactics
- Tier lock enables target prioritization (kill ranged first)
- Movement speed creates playstyle differences (fast vs. slow)

**Skill Expression:**
- Dodging projectiles (move during travel time)
- Tier lock prioritization (which enemy first?)
- Positioning against kiting enemies (cut off escape routes)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Projectile system solid, tier lock completes framework, movement speed foundational
- PLAYER: ✅ Creates variety, tactical depth, makes Grace valuable

**Scope Constraint:** Fits in one SOW (7-11 days for 3 features)

**Dependencies:**
- ADR-002: Movement system (Grace scaling)
- ADR-003: Reaction queue (projectile damage enters queue)
- ADR-004: Targeting framework (tier lock extends it)
- ADR-005: Damage pipeline (projectile damage)
- ADR-006: AI framework (kiting behavior variant)

**Next Steps:**
1. ARCHITECT creates ADR-015 documenting projectile system architecture
2. ARCHITECT creates SOW-010 with 5-phase implementation plan
3. DEVELOPER begins Phase 1 (tier lock targeting)

**Date:** 2025-11-03
