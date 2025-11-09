# SOW-014: Spatial Difficulty System

## Status

**Proposed** - 2025-11-07

## References

- **RFC-014:** [Spatial Difficulty System (Level-Based Enemy Variety)](../01-rfc/014-spatial-difficulty-system.md)
- **Spec:** [Haven System Specification](../00-spec/haven-system.md) (full system, this is MVP subset)
- **Branch:** (proposed)
- **Implementation Time:** 6.5-9.5 hours

---

## Implementation Plan

### Phase 1: Core Infrastructure + Dynamic Engagement Spawning

**Goal:** Level calculation, archetype system, dynamic engagement spawning with budget management

**Deliverables:**
- Spatial difficulty module in `common/` (level/archetype calculation)
- Engagement components (`Engagement`, `EngagementMember`, `ZoneId`)
- Engagement budget resource (zone tracking, HashMap-based)
- Engagement spawner system (chunk-triggered, multi-stage validation)
- Engagement cleanup system (completion + abandonment conditions)
- Haven location constant (world origin as spawn point)

**Architectural Constraints:**
- Level calculation: `distance_from_haven / 100` (clamped to 0-10)
- Directional zones: North (315°-45°), East (45°-135°), South (135°-225°), West (225°-315°)
- Four archetypes: Berserker (North), Juggernaut (East), Kiter (South), Defender (West)
- Attribute distribution: Alternating allocation (odd levels → axis1, even levels → axis2)
  - Berserker: -Might (odd), -Instinct (even)
  - Juggernaut: -Vitality (odd), +Presence (even)
  - Kiter: +Grace (odd), +Focus (even)
  - Defender: +Focus (odd), -Instinct (even)
- Engagement spawning trigger: Hook `send_chunk_to_player()` in chunk management
- Spawn validation pipeline:
  1. 50% probability gate (not every chunk)
  2. Zone budget check (max 5 per 500-tile zone)
  3. Player proximity check (min 30 tiles from any player)
  4. Engagement spacing check (min 50 tiles from other engagements)
- Engagement structure: Parent entity + 1-3 NPC children (random group size)
- NPCs leashed to engagement location (15 tile radius, existing Leash component)
- Budget tracking: HashMap<ZoneId, usize> (zone_id → active count)
- Cleanup conditions:
  - All NPCs killed → despawn engagement, free budget slot
  - No players within 100 tiles for 60s → despawn all, free budget slot
- Integration: Reuse existing Leash, AI behaviors (Chase/Kite), chunk management

**Success Criteria:**
- Level 0 enemies spawn at 0-99 tiles from origin
- Level 5 enemies spawn at 500-599 tiles from origin
- Level 10 enemies spawn at 1000+ tiles from origin
- North spawns Berserkers, East spawns Juggernauts, South spawns Kiters, West spawns Defenders
- Berserker level 5 has -3 Might, -2 Instinct (alternating allocation verified)
- Chunk receipt triggers 50% spawn chance (probability gate working)
- Max 5 engagements per 500-tile zone (budget enforced)
- No spawns within 30 tiles of players (proximity check working)
- No spawns within 50 tiles of other engagements (spacing check working)
- 1-3 NPCs per engagement (random group size)
- Engagement despawns when all NPCs killed (completion cleanup)
- Engagement despawns after 60s with no players within 100 tiles (abandonment cleanup)
- Budget slot freed when engagement despawns

**Duration:** 4-5 hours

---

### Phase 2: Counter Ability (Replaces Knockback)

**Goal:** Implement Counter ability as reactive defensive option for Defender archetype

**Deliverables:**
- Counter ability implementation in `server/systems/combat/abilities/counter.rs`
- Remove Knockback ability (delete file, update enum)
- Update ability bar assignments (Counter replaces Knockback)
- Preserve ADR-012 synergy (Overpower → Counter uses same ability type as Knockback)

**Architectural Constraints:**
- Counter = defensive reaction ability (not offensive)
- Validation: Caster must have entries in own ReactionQueue (front entry)
- Validation: Adjacent to target (melee range, 1 hex)
- Effect:
  1. Pop front threat from caster's ReactionQueue
  2. Negate threat damage completely (caster takes 0 damage)
  3. Queue 50% of threat damage as new threat in origin entity's ReactionQueue
- Costs: 30 stamina
- Recovery: 1.2s
- Ability type: Same as Knockback (preserves Overpower synergy from ADR-012)
- Integration: Works with existing ReactionQueue system (ADR-003)
- Reflection: NOT direct damage, queues threat (attacker must react)

**Success Criteria:**
- Counter only usable when caster has entries in ReactionQueue
- Counter fails if ReactionQueue empty (validation working)
- Counter pops FRONT threat (not just any threat)
- Counter negates threat damage (caster takes 0)
- Counter queues 50% reflected damage to origin entity's ReactionQueue
- Reflected damage appears as threat in origin's queue (can be reacted to)
- Counter costs 30 stamina (resource check working)
- Counter has 1.2s recovery (ADR-012 integration)
- Overpower → Counter synergy works (same ability type as Knockback)
- Knockback enum removed from codebase
- Knockback file deleted
- Ability bars updated (Counter in place of Knockback)

**Duration:** 1-2 hours

---

### Phase 3: AI Integration (Minimal - Reuse Existing)

**Goal:** Assign AI behaviors to spawned NPCs per archetype

**Deliverables:**
- AI behavior assignment in engagement spawner (part of NPC creation)
- Counter ability validation (check ReactionQueue before use)

**Architectural Constraints:**
- Berserker archetype → Chase AI (existing behavior, close to melee + use Lunge)
- Juggernaut archetype → Chase AI (existing behavior, close to melee + use Overpower)
- Kiter archetype → Kite AI (existing Forest Sprite behavior, maintain 3-6 hex distance + use Volley)
- Defender archetype → Chase AI (existing behavior, close to melee + use Counter when queue has threats)
- NO new AI behaviors required (all existing)
- Counter validation: AI checks for ReactionQueue entries before attempting Counter (if empty, fail gracefully and retry)
- Ability usage: Existing `UseAbilityIfAdjacent` and `UseAbilityIfInRange` behavior tree nodes

**Success Criteria:**
- Berserkers chase players and use Lunge when in range (1-4 hexes)
- Juggernauts chase players and use Overpower when adjacent (1 hex)
- Kiters maintain 3-6 hex distance and use Volley when in range
- Defenders chase players and use Counter when adjacent AND ReactionQueue has threats
- Counter attempt fails gracefully if ReactionQueue empty (AI retries later)
- All archetypes use existing AI behavior trees (no new behaviors implemented)

**Duration:** 0.5 hours

---

### Phase 4: UI and Feedback

**Goal:** Players understand spatial difficulty system through UI indicators

**Deliverables:**
- Distance indicator UI (show distance from haven, current zone, expected enemy level)
- Enemy nameplate enhancements (show level, archetype name, difficulty color)
- Optional: Danger warnings (screen border color, audio cues)

**Architectural Constraints:**
- Distance indicator shows:
  - Distance from haven (in tiles)
  - Current directional zone (North/East/South/West)
  - Expected enemy level (calculated from current distance)
- Enemy nameplates show:
  - Enemy level (numeric)
  - Archetype name (Berserker/Juggernaut/Kiter/Defender)
  - Color-coded by difficulty (relative to player level - future feature, placeholder for MVP)
- UI location: HUD element (persistent, always visible)
- Update frequency: Every frame (or when player moves significantly)
- Optional danger warnings:
  - Screen border glow (red intensity scales with zone level)
  - Audio cues when entering high-level zones (threshold: level 7+)
- Integration: Client-side UI, reads player Loc component, calculates from haven location

**Success Criteria:**
- Distance indicator visible in HUD
- Distance indicator shows accurate distance from haven (matches level calculation)
- Current zone displayed correctly (North/East/South/West)
- Expected enemy level matches actual spawned enemy levels
- Enemy nameplates show level number
- Enemy nameplates show archetype name
- Nameplates color-coded (placeholder implementation for MVP)
- New players can identify safe zones (low distance = low level)
- New players can identify dangerous zones (high distance = high level)
- Optional: Screen border glows red in high-level zones
- Optional: Audio cue plays when entering level 7+ zones

**Duration:** 1-2 hours

---

## Acceptance Criteria

**Functional:**
- Spatial difficulty gradient works (level 0 near haven, level 10 far)
- Four archetypes spawn in correct directional zones
- Attribute distributions correct (alternating allocation per archetype)
- Dynamic spawning works (chunk-triggered, multi-stage validation)
- Budget system enforced (max 5 per zone)
- Fair spawning (30 tiles from players, 50 from engagements)
- Cleanup lifecycle works (completion + abandonment)
- Counter ability works (reflection mechanic via ReactionQueue)
- Knockback removed from codebase
- AI behaviors assigned correctly per archetype

**UX:**
- Smooth difficulty curve (no sudden spikes)
- Spatial variation obvious (different directions feel different)
- Tactical variety (four archetypes require different approaches)
- Self-directed challenge selection (players choose where to go)
- Fair spawning (never surprised by spawns on top of you)
- Exploration rewarding (movement finds content)
- UI communicates distance/zone/level clearly

**Performance:**
- Level calculation: Negligible overhead (once per spawn)
- Dynamic spawning: < 0.1ms per chunk receipt
- Budget tracking: Fast HashMap lookups
- Cleanup: < 0.1ms per periodic tick
- No performance regression from static spawner baseline

**Code Quality:**
- Level/archetype calculation isolated in spatial_difficulty module
- Engagement spawning isolated in dedicated system
- Cleanup isolated in dedicated system
- Counter ability follows pipeline pattern (if ADR-018 implemented, else standard pattern)
- Static spawner system deprecated (marked for removal)

---

## Discussion

*This section will be populated during implementation with questions, decisions, and deviations.*

### Implementation Note: Spec Deviation

This SOW implements a **simplified subset** of the haven system spec:
- Single haven (not three)
- Direct distance calculation (not influence radius)
- No encroachment/siege mechanics
- No biome-specific placement

**Rationale:** Combat prototyping needs variety NOW (6.5-9.5 hours), full haven system is ~40+ hours. Clean migration path to full system later.

### Implementation Note: Static Spawner Deprecation

Static spawner system will remain in codebase during Phase 1 (allows A/B comparison). Mark as deprecated, plan removal in future cleanup pass.

---

## Acceptance Review

*This section will be populated after implementation is complete.*

---

## Sign-Off

**Reviewed By:** (pending)
**Date:** (pending)
**Decision:** (pending)
