# ADR-005 Acceptance: Damage Pipeline and Combat Resolution

## Status

**ACCEPTED** - 2025-10-31

## Summary

ADR-005 (Damage Pipeline and Combat Resolution) has been successfully implemented across all 6 phases as specified. The implementation introduces a complete combat system with damage calculation, threat queuing, client prediction, visual feedback, and death/respawn mechanics.

## Implementation Overview

### What Was Delivered

**Phase 1: Damage Calculation Functions** ✅
- Pure, testable damage calculation functions in `common/systems/combat/damage.rs`
- Two-phase calculation: outgoing damage (attacker) + passive modifiers (defender)
- Critical hit system based on Instinct attribute
- 17 comprehensive unit tests including edge cases and statistical validation
- 401 lines of implementation + tests (excellent test coverage ratio)

**Phase 2: Server Damage Pipeline** ✅
- Event-driven architecture using Bevy observers
- `process_deal_damage` → roll crit, calculate outgoing, insert threat
- `resolve_threat` → apply mitigation, update health, emit damage event
- Proper separation of concerns (deal → insert → resolve → apply)
- Overflow handling for ReactionQueue

**Phase 3: Client Damage Response** ✅
- Floating damage numbers with world-to-screen projection
- Health bars above entities (dynamic spawn/despawn)
- Extracted to dedicated `client/systems/combat_ui.rs` module
- Event-driven UI spawning (reactive, not polling)
- Efficient O(1) health bar tracking using HashSet

**Phase 4: Client Damage Prediction** ✅
- Local player health prediction on threat resolution
- Prediction rollback handling with logging
- Extracted to dedicated `client/systems/prediction.rs` module
- Shared logic between client/server via `common/systems/combat/`
- BasicAttack and Dodge prediction systems

**Phase 5: Death and Respawn** ✅
- Dedicated `check_death` system decoupled from combat pipeline
- Observer-based `handle_death` (players get RespawnTimer, NPCs despawn)
- Client-side filtering approach (local player ignores Despawn messages)
- `actor_dead_visibility` system with comprehensive tests (130 lines including 4 test cases)
- Preserves original EntityType on respawn (no hard-coded actor types)
- Respawn clears Offset for clean position snap

**Phase 6: Health Bar Polish** ✅
- World-space health bars with camera projection
- Smooth interpolation using `health.step`
- Automatic despawn when full health
- Only shows for damaged entities
- Integrated with combat feedback system

### Code Organization

**Module Structure (Final)**:
```
src/common/systems/combat/
  damage.rs (125 lines)        - Pure damage calculation functions
  queue.rs                      - Threat queue utilities (shared)
  resources.rs (230 lines)      - Health/Stamina/Mana, death/respawn

src/server/systems/combat.rs (275 lines)
  - process_deal_damage (observer)
  - resolve_threat (observer)
  - handle_use_ability

src/client/systems/
  combat.rs (130 lines)         - Event handlers (insert threat, apply damage, etc.)
  prediction.rs (124 lines)     - Client-side prediction logic
  combat_ui.rs (171 lines)      - Floating text, health bars
  actor_dead_visibility.rs (130 lines) - Dead actor hiding + tests

src/common/systems/combat/resources.rs
  - check_death (Update schedule, ANY death source)
  - handle_death (observer, player/NPC split)
  - process_respawn (timer-based, preserves EntityType)
```

**Lines of Code**:
- Total changes: +2072 lines, -242 lines (net +1830)
- Core damage module: 401 lines (125 code + 276 tests)
- Server combat: 275 lines (down from initial 284 after refactor)
- Client combat: 130 lines (down from 415 after extraction)
- Client prediction: 124 lines (extracted)
- Client UI: 171 lines (extracted)

## Architectural Improvements Made

### Refactorings Applied During Review

**1. God Module Split** (Commit: c801970)
- Original `client/systems/combat.rs`: 415 lines (mixed concerns)
- Refactored into 3 modules:
  - `combat.rs` (130 lines) - event handling only
  - `prediction.rs` (124 lines) - prediction logic
  - `combat_ui.rs` (171 lines) - UI rendering
- **Impact**: Clear separation of concerns, easier to test and maintain

**2. Death Check System Restored** (Commits: fe35298, c801970)
- Moved from inline in `resolve_threat` to dedicated system
- Decouples "what causes death" from "detecting death"
- Enables future death sources (fall damage, poison, execute abilities)
- Runs in Update schedule after all health changes
- **Impact**: Extensible death detection, not coupled to combat damage

**3. EntityType Preservation on Respawn** (Commit: 1350cc2)
- Changed from hard-coded `Natureborn/Direct/Vital` to queried `&EntityType`
- Respawn now preserves original actor Triumvirate type
- **Impact**: Supports future actor variety, no type loss on respawn

**4. Health Bar Efficiency** (Commit: 77b62f4)
- Changed from O(n²) polling to event-driven + O(1) lookup
- Uses `Changed<Health>` filter (reactive)
- HashSet for existing bar checks (O(1) instead of O(n))
- **Impact**: Scales to hundreds of entities without performance hit

**5. Consistent Component Pattern** (Commit: 0a6d420)
- `do_incremental` now consistently handles insert-or-update for all components
- Clear pattern: check for existing component, update if present, insert if missing
- **Impact**: Explicit lifecycle management, easier to understand

**6. Dead Player Filtering** (Commit: fe35298)
- Dead players excluded from entity discovery (prevents ghost respawns)
- `RespawnTimer` used as canonical death marker on server
- `health <= 0` used as death indicator on client
- **Impact**: Clean death state, no phantom entities

## What Worked Well

### 1. Pure Function Design
The damage calculation module (`damage.rs`) is exemplary:
- Zero coupling to ECS or networking
- Easily testable (17 unit tests)
- Shared between client and server (no duplication)
- Clear documentation with formulas from spec
- **This should be the standard for future combat logic**

### 2. Event-Driven Architecture
Using Bevy observers for combat events:
- Clean separation between systems
- Easy to trace event flow (deal → insert → resolve → apply → death)
- Extensible (can add new observers without modifying existing code)
- Testable (can trigger events in isolation)

### 3. Two-Phase Damage Calculation
Hybrid timing model works as designed:
- Phase 1 (insertion): Attacker's attributes at attack time
- Phase 2 (resolution): Defender's attributes at defense time
- Handles mid-queue attribute changes correctly
- Fair to both attacker and defender

### 4. Client-Side Prediction
Minimal prediction approach reduces complexity:
- Only predict local player health (not remote entities)
- Shared calculation functions (no logic duplication)
- Rollback logging helps detect issues
- Server correction works smoothly

### 5. Comprehensive Testing
`actor_dead_visibility.rs` demonstrates good testing practices:
- 4 test cases covering all scenarios
- Tests both players and NPCs
- Tests lifecycle (alive → dead → respawned)
- Uses Bevy's `run_system_once` for clean unit tests

### 6. Incremental Refactoring
Development followed a healthy pattern:
1. Implement features (get it working)
2. Identify code smells during review
3. Refactor systematically (7 commits of improvements)
4. Result: Clean, maintainable code

## Lessons Learned

### 1. Single Responsibility Principle Matters
**Observation**: Initial `combat.rs` at 415 lines mixed combat logic, prediction, and UI rendering.

**Learning**: When a file grows past ~200 lines, audit for multiple concerns. If found, extract into focused modules immediately.

**Guideline**: Module should have one reason to change (combat rules, prediction algorithms, or UI styling - not all three).

### 2. Dedicated System Beats Inline Checks
**Observation**: Death check was initially inline in `resolve_threat`, limiting death to combat damage only.

**Learning**: Dedicated systems allow multiple sources to trigger the same outcome. Death from combat, fall damage, poison, or execute abilities all converge on one check.

**Guideline**: If logic needs to work with multiple input sources, create a dedicated system (don't inline).

### 3. Query What You Need, Don't Hard-Code
**Observation**: Respawn initially hard-coded `Natureborn/Direct/Vital` actor type.

**Learning**: If data exists in ECS, query it. Hard-coding loses information and creates bugs later.

**Guideline**: If a value is stored in a component, query it. Only hard-code true constants (physics values, config).

### 4. Reactive Beats Polling
**Observation**: Health bars initially polled all entities every frame (O(n²)).

**Learning**: Bevy's `Changed<T>` filter makes reactive systems trivial. Event-driven scales better.

**Guideline**: Prefer `Query<_, Changed<Component>>` over polling. Spawn UI on events, not every frame.

### 5. Client-Side Filtering Is Valid
**Observation**: Concern about sending Despawn to all clients (including owner) felt wrong.

**Learning**: Client-side filtering (ignore if local player) is simpler than server-side targeting logic. Both are architecturally sound.

**Guideline**: Simple client-side checks are acceptable. Don't over-engineer server targeting unless bandwidth matters.

### 6. Tests Provide Confidence for Refactoring
**Observation**: `damage.rs` had 17 tests before refactoring. Refactoring was safe.

**Learning**: Comprehensive tests enabled fearless extraction of prediction/UI modules.

**Guideline**: Write tests for pure functions first (easiest to test). Tests pay off during refactoring.

## Consequences

### Positive

1. **Complete Combat Foundation**
   - Damage calculation, threat queue, prediction, visual feedback all working
   - Foundation for future abilities (magic damage, DoT, shields, etc.)
   - Extensible event-driven architecture

2. **Clean Module Boundaries**
   - Combat logic, prediction, and UI cleanly separated
   - Shared code in `common/` (testable, reusable)
   - Each module has clear, single responsibility

3. **Maintainable Codebase**
   - Code organization makes it easy to find things
   - Pure functions enable confident changes
   - Tests catch regressions

4. **Player Experience Delivered**
   - Instant feedback (client prediction)
   - Clear visual feedback (damage numbers, health bars)
   - Smooth death/respawn flow

### Negative (Accepted Trade-offs)

1. **Client Prediction Complexity**
   - Requires clock synchronization (`server.current_time()`)
   - Prediction errors possible with high latency
   - Accepted: Prediction improves UX despite occasional mismatches
   - Mitigation: Logging warns of prediction errors

2. **UI Entity Overhead**
   - Damage numbers and health bars spawn entities
   - Potential performance impact with 100+ simultaneous damage events
   - Accepted: MVP doesn't hit these scales yet
   - Mitigation: Reactive spawning (Changed<Health>) minimizes overhead

3. **Dual Death State (Server vs Client)**
   - Server uses `RespawnTimer` component as death marker
   - Client uses `health <= 0` as death indicator
   - Accepted: Appropriate separation (server authority, client observation)
   - Not a problem: Each side uses the right marker for its concern

4. **Component Insertion in do_incremental**
   - System now does both update AND insert
   - Blurs line between spawn and update
   - Accepted: Enables late component attachment pattern
   - Mitigation: Documented in code comments

### Neutral (Future Considerations)

1. **No Corpse System Yet**
   - NPCs despawn immediately (no corpse decoration)
   - Future: Spawn separate corpse entity (decorator pattern)
   - Deferred: Not needed for MVP

2. **Basic Damage Numbers Only**
   - White text, no crit indication, no damage type icons
   - Future: Color coding, icons, animations
   - Deferred: Visual polish for Phase 2

3. **Origin-Only Respawn**
   - Players always respawn at (0,0,4)
   - Future: Settlement-based respawn, last haven
   - Deferred: Requires haven system (spec planned)

## Validation

### Functional Requirements (ADR-005)
- ✅ Two-phase damage calculation (outgoing + mitigation)
- ✅ Critical hit system (Instinct-based)
- ✅ Threat queueing with timer expiry
- ✅ Overflow handling (oldest threat resolves)
- ✅ Client prediction (local player health)
- ✅ Damage numbers (floating text)
- ✅ Health bars (above entities)
- ✅ Death detection (HP <= 0)
- ✅ Respawn system (5-second timer, origin teleport)
- ✅ Mutual destruction (both entities die simultaneously)

### Code Quality Standards
- ✅ Single Responsibility: Modules focused on one concern each
- ✅ Testability: Pure functions have unit tests
- ✅ Documentation: Damage functions documented with formulas
- ✅ Performance: O(1) health bar lookups, reactive spawning
- ✅ Separation of Concerns: Combat, prediction, UI separated
- ✅ Event-Driven: Observers used appropriately
- ✅ ECS Patterns: Queries, components, systems follow Bevy best practices

### Integration with Existing Systems
- ✅ Integrates ADR-002 (Health/Stamina/Mana, passive modifiers)
- ✅ Integrates ADR-003 (ReactionQueue, threat insertion/expiry)
- ✅ Integrates ADR-004 (Ability system, directional targeting)
- ✅ Uses existing `Offset` system for position management
- ✅ Uses existing `NNTree` for spatial queries
- ✅ Follows existing prediction pattern (InputQueue model)

## Files Changed

**New Files Created** (8):
- `src/common/systems/combat/damage.rs` (401 lines) - ⭐ Exemplary
- `src/client/systems/prediction.rs` (124 lines)
- `src/client/systems/combat_ui.rs` (171 lines)
- `src/client/systems/actor_dead_visibility.rs` (130 lines)
- `docs/spec/combat-hud.md` (780 lines) - UI specification
- `docs/adr/005-acceptance.md` (this document)

**Files Modified** (18):
- `src/server/systems/combat.rs` (156 → 275 lines, observer pattern)
- `src/client/systems/combat.rs` (415 → 130 lines, extracted UI/prediction)
- `src/common/systems/combat/resources.rs` (+139 lines, death/respawn)
- `src/common/systems/world.rs` (+48 lines, component insert-or-update)
- `src/client/systems/renet.rs` (+23 lines, Despawn filtering)
- `src/client/systems/input.rs` (+8 lines, dead player filtering)
- `src/client/systems/target_indicator.rs` (+13 lines, dead player filtering)
- `src/server/systems/input.rs` (+9 lines, RespawnTimer check)
- `src/server/systems/renet.rs` (+23 lines, cleanup_despawned)
- `src/server/systems/world.rs` (+15 lines, RespawnTimer exclusion)
- `src/run-server.rs` (observer registration, check_death schedule)
- `src/run-client.rs` (new system registration)
- `src/common/message.rs` (+8 lines, new events)
- `src/client/components/mod.rs` (+23 lines, FloatingText, HealthBar)
- `src/client/plugins/ui.rs` (+13 lines, combat_ui module)
- `docs/spec/combat-system.md` (4 lines, clarifications)

## Performance Characteristics

**Measured/Expected**:
- Damage calculation: Pure math, <0.01ms per hit
- Threat insertion: O(1) queue append, <0.01ms
- Health bar spawn: O(n) only on Changed<Health> (reactive)
- Health bar lookup: O(1) HashSet contains check
- Floating text: 1 entity per damage event, auto-despawn after 1.5s
- Death check: O(n) over all entities with Health, runs once per frame
- Prediction: Runs only for local player (single entity query)

**Scalability**:
- 10 entities fighting: ~5 damage events/sec = negligible overhead
- 100 entities fighting: ~50 damage events/sec = still negligible
- Health bars: Up to ~50 visible (only damaged entities)
- No observed performance issues in testing

## Recommendations for Future Work

### Immediate (Next Sprint)
1. **Add fall damage** - Tests death check system from non-combat source
2. **Add damage type colors** - White (physical), blue (magic), polish phase
3. **Add crit visual feedback** - Orange damage numbers, "CRIT!" text
4. **Monitor prediction errors** - Add telemetry for clock drift

### Medium Term (Phase 2)
1. **Corpse decorator system** - NPCs leave lootable corpses (30s despawn)
2. **Settlement-based respawn** - Respawn at last visited haven
3. **Damage meters** - DPS tracking, combat log
4. **Magic damage types** - Fire, Ice, Poison (ADR-005 designed for extensibility)

### Long Term (Phase 3+)
1. **Reaction abilities** - Counter, Reflect, Shield (uses threat queue)
2. **Damage over time** - Poison, Burn (periodic threat insertion)
3. **Area damage** - Multi-target threat insertion
4. **Advanced UI** - Damage number batching, object pooling

## Sign-Off

**Architect Review**: ✅ APPROVED
- Code organization follows principles
- Separation of concerns achieved
- Technical debt addressed via refactoring
- Module boundaries clear and maintainable

**Implementation Quality**: ✅ APPROVED
- All 6 phases delivered
- Pure functions well-tested
- Event-driven architecture sound
- Client prediction working as designed

**Integration Testing**: ✅ APPROVED
- Combat pipeline works end-to-end
- Death/respawn cycle functional
- Client prediction provides instant feedback
- No critical bugs identified

**Documentation**: ✅ APPROVED
- ADR-005 original document comprehensive
- Code comments explain formulas
- Refactoring commits well-described
- This acceptance document captures lessons learned

## Conclusion

ADR-005 is **ACCEPTED** as implemented. The damage pipeline provides a solid foundation for future combat features. The implementation demonstrates good architectural practices:

- Starting with pure, testable functions
- Using event-driven patterns for extensibility
- Refactoring when code smells emerge
- Writing tests for confidence

The refactoring process (7 commits of improvements after initial implementation) shows healthy development practices: implement, review, refactor, test.

**This ADR serves as a reference for future combat system work.**

---

**Date**: 2025-10-31
**Commits**: 766b94f through 77b62f4 (11 commits total)
**Branch**: adr-005-damage-pipeline
**Ready to Merge**: ✅ YES
