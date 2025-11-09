# SOW-005: Damage Pipeline and Combat Resolution

## Status

**Merged** - 2025-10-31

## References

- **RFC-005:** [Damage Pipeline and Combat Resolution](../01-rfc/005-damage-pipeline.md)
- **ADR-010:** [Damage Pipeline with Two-Phase Calculation](../02-adr/010-damage-pipeline-two-phase-calculation.md)
- **Branch:** (implementation details from acceptance)
- **Implementation Time:** 9-11 days

---

## Implementation Plan

### Phase 1: Damage Calculation Functions (1 day)

**Goal:** Shared damage calculation logic

**Deliverables:**
- `common/systems/damage.rs` module
- `calculate_outgoing_damage(base, attrs, type) -> f32`
- `roll_critical(attrs) -> (bool, f32)`
- `apply_passive_modifiers(damage, attrs, type) -> f32`
- Unit tests for all formulas

**Architectural Constraints:**
- Pure functions (no state, testable)
- Shared in `common/` (client and server identical)
- Formulas match spec exactly:
  - Physical scaling: 1 + (might/100)
  - Crit chance: 5% + (instinct/200), max 55%
  - Crit mult: 1.5 + (instinct/200), max 2.0x
  - Armor: vitality/200, max 75%

**Success Criteria:** All formula tests pass, crit probability correct (sample 1000 rolls)

---

### Phase 2: Server Damage Pipeline (2-3 days)

**Goal:** Server processes damage from abilities to Health

**Deliverables:**
- `server/systems/damage::process_deal_damage`
  - Process `Try::DealDamage` events
  - Roll crit, calculate outgoing damage
  - Insert into ReactionQueue
  - Emit `Do::InsertThreat`
- `server/systems/damage::process_resolve_threat`
  - Process `Try::ResolveThreat` events
  - Apply passive modifiers
  - Apply to Health component
  - Emit `Do::ApplyDamage`
  - Check death (emit `Try::Death` if HP <= 0)
- Update `abilities::execute_ability` to emit `Try::DealDamage`
- System schedule: expired_threats → resolve_threat → check_death (chained)

**Architectural Constraints:**
- Server authoritative (all calculations)
- Two-phase: Outgoing at insertion, mitigation at resolution
- Critical rolled at insertion time (stored in threat.damage)
- Death check after all damage (order-independent, mutual destruction)

**Success Criteria:** Wild Dog attack → InsertThreat → (timer) → ApplyDamage → Health decreased

---

### Phase 3: Client Damage Response (2 days)

**Goal:** Client responds to damage events

**Deliverables:**
- Handle `Do::ApplyDamage` events in client renet system
- Update Health component (remote: set `state`, local: accept correction)
- Spawn floating damage numbers
- `client/systems/floating_text.rs` module
  - FloatingText component (lifetime, velocity)
  - Update system (move upward, fade out)
  - Despawn after lifetime

**Architectural Constraints:**
- Damage numbers spawn above entity (Y + 2.0)
- Float upward (velocity 0.5), fade out (1 second)
- White text, 24pt font (MVP: no color coding)
- Limit max simultaneous (10 damage numbers)

**Success Criteria:** Player takes damage → health bar decreases, damage number appears and floats upward

---

### Phase 4: Client Damage Prediction (1-2 days)

**Goal:** Client predicts local player Health changes

**Deliverables:**
- `client/systems/damage::predict_threat_resolution`
  - Run in Update schedule
  - Query local player's ReactionQueue
  - Call `check_expired_threats` (client-side)
  - Predict damage: `health.step -= apply_passive_modifiers(...)`
- Rollback handling: Receive `ApplyDamage` → compare to predicted
- Dodge handling: Threat cleared → no ApplyDamage → health restored

**Architectural Constraints:**
- Predict only for local player (remote wait for server)
- Predict at resolution time (not insertion - may be Dodged)
- Update `step` field (not `state` - server owns state)
- Use same `apply_passive_modifiers` as server

**Success Criteria:** Local player threat expires → health drops instantly (predicted), server confirms within 100ms

---

### Phase 5: Death and Mutual Destruction (1 day)

**Goal:** Death flow completes damage pipeline

**Deliverables:**
- Verify death check in `resources::check_death` (from ADR-002)
  - Runs after damage application (FixedUpdate, chained)
  - Query entities with `Health.state <= 0.0`
  - Emit `Try::Death { ent }`
- Mutual destruction test:
  - Both entities have lethal damage queued
  - Both threats expire same frame
  - Both apply damage
  - Both die (Death events for both)
- Client death handling:
  - Receive `Do::Despawn` → remove entity

**Architectural Constraints:**
- Death check after all damage in frame (order-independent)
- System schedule: resolve_threat → check_death (chained)
- No tiebreaker (both die if mutual destruction)

**Success Criteria:** Player Health reaches 0 → Death event → Despawn; Mutual destruction: both die

---

### Phase 6: Damage UI Polish (2 days)

**Goal:** Visual clarity for Health changes

**Deliverables:**
- Health bar rendering above entities
  - Green bar (current HP), grey background (max HP)
  - Width proportional to current/max
  - Positioned at Y + 1.5
- Health bar updates:
  - Interpolate smoothly (no snapping)
  - Use `health.step` for local, `health.state` for remote
  - Show in combat only (CombatState check)
- Health bar lifecycle:
  - Spawn when entity enters combat
  - Update every frame
  - Despawn when out of combat and full HP

**Architectural Constraints:**
- World-space UI (follows entity)
- Scale with camera zoom
- Only visible in combat
- Smooth interpolation (no frame drops)

**Success Criteria:** Health bars visible on all entities in combat, update smoothly on damage, hidden out of combat

---

## Acceptance Criteria

**Functionality:**
- ✅ Damage formulas correct (scaling, mitigation, crit)
- ✅ Ability → InsertThreat → ResolveThreat → ApplyDamage → Death
- ✅ Attributes affect damage (Might increases, Vitality reduces)
- ✅ Critical hits work (~55% at Instinct=100)
- ✅ Mutual destruction: both die if lethal
- ✅ Client prediction: local player health drops instantly

**Performance:**
- ✅ Damage processing: < 1ms per event
- ✅ 100 damage events/sec: < 10ms total
- ✅ Damage numbers: 60fps with 100 simultaneous

**Network:**
- ✅ InsertThreat + ApplyDamage per attack: ~70 bytes
- ✅ 10 players fighting: ~500 bytes/sec
- ✅ Prediction within 100ms of server confirmation

**Code Quality:**
- ✅ Shared logic in `common/systems/damage.rs`
- ✅ Pure functions (deterministic, testable)
- ✅ Comprehensive tests (formulas, pipeline flow)
- ✅ Clear system ordering (chained FixedUpdate)

---

## Discussion

### Design Decision: Two-Phase Timing

**Context:** When to calculate damage (insertion vs resolution)?

**Decision:** Two-phase - outgoing at insertion, mitigation at resolution.

**Rationale:**
- Attacker scaling reflects state at attack time (fair)
- Defender mitigation reflects state at resolution time (fair)
- Handles mid-queue attribute changes correctly
- Critical roll at attack time (attacker's Instinct)

**Example:**
```
T=0s: Wild Dog attacks (Might=20)
  → outgoing = 20 * 1.2 = 24 damage
  → Store in threat

T=0.5s: Player gains buff (+50 Vitality)
  → armor now 50%

T=1s: Threat resolves
  → final = 24 * 0.5 = 12 damage (uses current armor)
  → Fair: player's buff helps
```

---

### Design Decision: Crit Storage

**Context:** Store crit flag or crit-modified damage?

**Decision:** Store crit-modified damage in `threat.damage`.

**Rationale:**
- Simpler: Client doesn't need to recalculate crit damage
- Fewer fields in QueuedThreat struct
- Crit roll reflects attacker's Instinct at attack time

**Trade-off:** Client doesn't know if damage was crit (visual distinction deferred to Phase 2)

---

### Design Decision: Damage Prediction Timing

**Context:** Predict at insertion or resolution?

**Decision:** Predict at resolution time (when threat expires).

**Rationale:**
- Threat may be cleared by Dodge before expiry
- Predicting at insertion causes premature health drop (feels wrong)
- Resolution prediction only wrong if server denies (rare)

**Example:**
```
Insertion: Don't predict (may be Dodged)
Resolution: Predict damage (likely to apply)
```

---

### Implementation Note: Mutual Destruction

**System schedule ensures both die:**
```rust
FixedUpdate: (
    queue::process_expired_threats,  // Both threats expire
    damage::process_resolve_threat,  // Both apply damage
    resources::check_death,           // Both checked for death
).chain()  // Sequential, all damage before death checks
```

**Key:** Death checks after ALL damage in frame (order-independent)

---

### Implementation Note: Damage Number Performance

**Concern:** 100 damage numbers = 100 entities

**Mitigation:**
- Limit max simultaneous (10 damage numbers)
- Despawn oldest when limit reached
- Object pooling if profiling shows issue (future)

**MVP approach:** Accept overhead, optimize if < 60fps

---

## Acceptance Review

**Review Date:** 2025-10-31
**Reviewer:** ARCHITECT Role
**Decision:** ✅ **ACCEPTED**

### Scope Completion: 100%

**All 6 phases complete:**
- ✅ Phase 1: Damage calculation functions
- ✅ Phase 2: Server damage pipeline
- ✅ Phase 3: Client damage response
- ✅ Phase 4: Client damage prediction
- ✅ Phase 5: Death and mutual destruction
- ✅ Phase 6: Damage UI polish

### Architectural Compliance

**✅ ADR-010 Specifications:**
- Two-phase calculation (outgoing at insertion, mitigation at resolution)
- Server authoritative (all calculations)
- Critical rolled at attack time (attacker's Instinct)
- Mutual destruction supported (order-independent death checks)

**✅ Integration:**
- ADR-002: Health resources (apply damage to Health.state)
- ADR-006/007/008: Reaction queue (threat insertion, expiry)
- ADR-009: Ability system (DealDamage events from abilities)

### Performance Verification

**Damage Processing:** < 1ms per event ✅
**Pipeline Throughput:** 100 events/sec < 10ms ✅
**Damage Numbers:** 60fps with 100 simultaneous ✅

### Code Quality

**Strengths:**
- Clean pipeline (each system single responsibility)
- Pure functions (deterministic, testable)
- Clear system ordering (chained FixedUpdate)
- Comprehensive tests (formulas, pipeline flow)

---

## Conclusion

The damage pipeline implementation completes the combat system loop: **abilities → queue → health → death**.

**Key Achievements:**
- Fair damage calculation (two-phase timing)
- Server authoritative (prevents cheating)
- Minimal client prediction (local player only)
- Mutual destruction supported
- Extensible design (hooks for future features)

**Architectural Impact:** Enables combat playtesting, AI behavior tuning, and attribute balance validation.

**The implementation achieves RFC-005's core goal: integrating abilities with the reaction queue and health system for complete combat resolution.**

---

## Sign-Off

**Reviewed By:** ARCHITECT Role
**Date:** 2025-10-31
**Decision:** ✅ **ACCEPTED**
**Status:** Merged to main
