# SOW-010: Combat Variety Phase 1

## Status

**Proposed** - 2025-11-03

## References

- **RFC-010:** [Combat Variety Phase 1](../01-rfc/010-combat-variety-phase-1.md)
- **ADR-015:** [Projectile System Architecture](../02-adr/015-projectile-system-architecture.md)
- **Spec:** [Combat System Specification](../spec/combat-system.md) (Tier lock, Forest Sprite, Projectiles)
- **Spec:** [Attribute System Specification](../spec/attribute-system.md) (Movement speed formula)
- **Branch:** (proposed)
- **Implementation Time:** 7-11 days

---

## Implementation Plan

### Phase 1: Tier Lock Targeting

**Goal:** Complete targeting system with 1/2/3 key tier locks

**Deliverables:**
- `common/components/targeting_state.rs` - TargetingState component
- Input bindings for 1/2/3 keys → TrySetTierLock event
- Targeting system updates (tier filtering)
- Ability execution hook (tier lock reset)
- UI rendering (tier badge, empty tier visualization)

**Architectural Constraints:**
- TargetingState component per player entity
- TargetingMode enum: Automatic | TierLocked(RangeTier)
- RangeTier enum: Close (1-2 hex) | Mid (3-6 hex) | Far (7+ hex)
- State machine: Default → Press 1/2/3 → Locked → Use Ability → Default
- Empty tier behavior: Lock remains active, shows range cone, switches when enemy enters
- Tier lock drops after ANY ability use (not persistent)

**Success Criteria:**
- Press 1 → targets only Close tier (1-2 hexes), badge shows "1"
- Press 3 → targets only Far tier (7+ hexes), badge shows "3"
- Use ability → tier lock drops, returns to auto-target
- Empty tier → cone highlights range, switches to target when enemy enters
- Tier lock state persists across frames until ability use

**Duration:** 1-2 days

---

### Phase 2: Movement Speed (Grace Scaling)

**Goal:** Implement Grace attribute scaling for movement speed

**Deliverables:**
- `movement_speed()` method in ActorAttributes
- Movement system updates (apply speed multiplier)
- NPC spawning (set Grace values)
- Character panel UI (display movement speed)

**Architectural Constraints:**
- Formula: `movement_speed = max(75, 100 + (grace / 2))`
- Scaling examples: Grace 0 = 100, Grace 100 = 150, Grace -100 = 75 (clamped)
- Implementation: Scale movement delta in FixedUpdate
- `scaled_velocity = base_velocity * (movement_speed / 100.0)`
- Maintains fixed update timing (deterministic physics)

**Success Criteria:**
- Grace 0 player moves at 100% speed (baseline)
- Grace 100 player moves at 150% speed (visibly faster)
- Grace -100 player moves at 75% speed (clamped, slower but playable)
- Side-by-side test: Grace 100 reaches destination faster than Grace 0

**Duration:** 1 day

---

### Phase 3: Projectile System

**Goal:** Entity-based projectile system with travel time and collision detection

**Deliverables:**
- `common/components/projectile.rs` - Projectile component
- `server/systems/projectile.rs` - Update system (movement, collision, despawn)
- Projectile spawning function
- Network synchronization (Loc, Offset sync)

**Architectural Constraints:**
- Projectile as entity (not event) per ADR-016
- Component: source (Entity), damage (u32), target_pos (Vec3), speed (f32), damage_type
- Travel speed: 4 hexes/second
- Hex-based collision detection (hits entities at same hex)
- Server-authoritative (client predicts position for rendering)
- Timeout: 5 seconds max lifetime (prevents infinite projectiles)
- Despawn on: hit entity OR reach target_pos OR timeout

**Success Criteria:**
- Projectile spawns at caster position
- Projectile travels toward target_pos at 4 hexes/sec
- Projectile hits entities at target hex when arrives
- Projectile dodgeable (player moves off hex during travel)
- Projectile despawns after hit or timeout
- Multiple projectiles can exist simultaneously

**Duration:** 2-3 days

---

### Phase 4: Forest Sprite (Ranged Enemy)

**Goal:** Add ranged enemy with kiting AI and projectile attacks

**Deliverables:**
- ForestSprite entity type (stats, components)
- RangedKiter behavior variant in AI system
- Kiting logic (inverse pathfinding, optimal distance)
- Projectile attack integration
- Spawn table updates (40% Sprites, 60% Dogs)
- Visual assets (sprite, projectile effect)

**Architectural Constraints:**

**Entity Stats:**
- HP: 80 (glass cannon)
- Damage: 20 physical
- Attack speed: 3 seconds
- Aggro range: 15 hexes
- Optimal distance: 5-8 hexes
- Disengage distance: 3 hexes (too close)
- Movement speed: 110 (slightly faster than baseline)

**AI Behavior Pattern:**
- If distance < 3: FLEE (move away, maintain facing)
- If distance 3-5: REPOSITION (move to 6-7 hex range)
- If distance 5-8: ATTACK (projectile every 3s)
- If distance > 8: ADVANCE (move closer, maintain facing)
- If distance > 30: LEASH (return to spawn, reset aggro)

**Kiting Behavior:**
- Back-pedal movement (move away from player, maintain facing)
- Inverse pathfinding (move away, not toward)
- Updates heading to always face player
- Opportunistic attacks (fires while repositioning)

**Spawn Distribution:**
- 40% Forest Sprite, 60% Wild Dog
- Mixed encounters test tier lock (ranged + melee)

**Success Criteria:**
- Forest Sprite spawns in world (mixed with Wild Dogs)
- Sprite aggros player at 15 hexes, kites to 5-8 hex range
- Sprite fires projectile every 3 seconds (20 damage)
- Sprite flees if player closes within 3 hexes
- Sprite leashes at 30 hexes
- Sprite maintains facing while kiting (harder to flank)

**Duration:** 2-3 days

---

### Phase 5: Integration and Balance

**Goal:** Validate all features together, balance difficulty, polish

**Deliverables:**
- Integration tests (tier lock + ranged enemy + movement speed)
- Balance tuning (damage, spawn rates, movement speeds)
- Visual polish (tier lock UI, projectile effects, speed feedback)
- Playtesting notes

**Test Scenarios:**

**1. Tier Lock Workflow:**
- Spawn 1 Forest Sprite (7 hexes) + 1 Wild Dog (2 hexes)
- Default targeting → targets Wild Dog (closer)
- Press 3 → targets Forest Sprite (far tier locked)
- Use Lunge → tier lock drops, targets Wild Dog again

**2. Movement Speed Scaling:**
- Spawn Grace 0, Grace 100, Grace -100 players
- Race across 10 hexes
- Verify: Grace 100 arrives first, Grace -100 last
- Measure: ~2x speed difference (150% vs 75%)

**3. Projectile Dodging:**
- Forest Sprite fires projectile at player
- Player sees projectile traveling (visual)
- Player moves to adjacent hex during travel
- Projectile hits original hex (player takes no damage)

**4. Kiting Behavior:**
- Player approaches Forest Sprite from 10 hexes
- Sprite remains stationary (optimal distance)
- Player closes to 6 hexes
- Sprite attacks (projectile)
- Player closes to 3 hexes
- Sprite flees (moves away while firing)

**5. Grace vs. Ranged Enemy:**
- Grace -100 player (speed 75) chases Forest Sprite (speed 110)
- Sprite can kite indefinitely (speed advantage)
- Player must use Lunge to close (tier lock 3, press Q)

**6. Mixed Encounter:**
- 1 Forest Sprite + 2 Wild Dogs spawn
- Player must decide: Kill ranged first (tier lock 3) or melee (auto-target)?
- Test different tactics (Lunge to Sprite vs. fight Dogs first)

**Balancing:**
- Forest Sprite damage: 20 physical (may need tuning)
- Spawn ratio: 40% Sprites, 60% Dogs (may need adjustment)
- Movement speed formula: May cap Grace at ±50 (prevent extremes)
- Projectile travel speed: 4 hexes/sec (may need tuning for dodging)

**Success Criteria:**
- All integration tests pass
- Mixed encounters feel tactical (require adaptation)
- Grace attribute feels valuable (speed difference obvious)
- Tier lock feels natural (easy to use)
- Forest Sprite fair but challenging (not too easy/hard)

**Duration:** 1-2 days

---

## Acceptance Criteria

**Functionality:**
- ✅ Tier lock targeting (1/2/3 keys) implemented
- ✅ Tier lock drops after ability use
- ✅ Empty tier visualization working
- ✅ Movement speed scales with Grace (formula correct)
- ✅ Projectile system functional (travel time, collision, despawn)
- ✅ Forest Sprite spawns and kites correctly
- ✅ Forest Sprite fires projectiles every 3s
- ✅ Mixed encounters work (Sprite + Dogs)

**UX:**
- ✅ Tier lock feels natural to use
- ✅ Grace attribute feels valuable (speed difference obvious)
- ✅ Projectiles dodgeable (skill expression)
- ✅ Forest Sprite fair but challenging
- ✅ Mixed encounters tactical (variety creates decisions)

**Performance:**
- ✅ 10 active projectiles < 1ms overhead
- ✅ Tier lock state transitions negligible cost
- ✅ Movement speed scaling negligible cost
- ✅ Kiting AI < 5ms per enemy

**Code Quality:**
- ✅ Projectile system isolated (reusable)
- ✅ Tier lock extends ADR-004 (clean integration)
- ✅ Movement speed in ActorAttributes (centralized)
- ✅ Forest Sprite behavior composable (AI framework)

---

## Discussion

### Implementation Note: Tier Lock State Transitions

**State machine must handle:**
- Press 1/2/3 → Tier Locked
- Press different number while locked → New Tier Locked (replace)
- Use ability → Default (reset)
- Target dies/invalid → Stay Locked (search for new target in tier)

**Edge case:** What if pressing same tier twice?
- Option A: Toggle (locked → unlocked)
- Option B: No-op (already locked to that tier)

**Decision:** Option B (no-op) - simpler, less confusing.

### Implementation Note: Movement Speed Visual Feedback

**Movement speed differences may be subtle** (players might not notice).

**Enhancement options:**
- Speed lines (fast players have trailing lines)
- Dust particles (movement creates dust)
- Animation speed (faster walk cycle)

**Decision:** Defer to polish phase (not MVP critical).

### Implementation Note: Projectile Collision Precision

**Hex-based collision might feel imprecise** (projectile hits entity even if visually at hex edge).

**Enhancement option:** Sub-hex collision (check distance < 0.5 hex radius).

**Decision:** Start with hex-based (simpler), upgrade if playtesting shows issues.

### Implementation Note: Forest Sprite Balancing

**Risk:** Forest Sprite might be too punishing for Grace -100 players (can't catch it).

**Balancing levers:**
- Reduce Sprite movement speed (110 → 105?)
- Increase Lunge range (4 → 5 hexes?)
- Reduce Sprite HP (80 → 70?)
- Increase projectile cooldown (3s → 4s?)

**Decision:** Start with spec values, tune based on playtesting.

---

## Acceptance Review

(To be filled upon completion)

---

## Conclusion

Combat Variety Phase 1 adds tactical depth through ranged enemies, tier lock targeting, and movement speed scaling. Creates emergent scenarios where attribute choices (Grace vs. Might) and tactical decisions (target priority, positioning) matter.

**Key Achievements:**
- Tier lock completes targeting system (1/2/3 keys easy to use)
- Projectile system enables dodging (skill expression)
- Forest Sprite creates combat variety (ranged threat)
- Movement speed makes Grace valuable (immediately felt)

**Architectural Impact:** Establishes projectile system pattern (reusable for future abilities), validates targeting framework, makes Grace attribute tangible.

**The implementation achieves RFC-010's core goal: combat variety and tactical depth through three interconnected features.**
