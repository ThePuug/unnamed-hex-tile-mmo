# RFC-017: Combat Balance Overhaul

## Status

**Implemented** - 2026-02-09

## Feature Request

### Player Need

From player perspective: **Higher-level characters should feel meaningfully stronger than lower-level ones** - Currently, three level-0 NPCs can overwhelm a level-10 player because stats scale linearly while threat count scales multiplicatively.

**Current Problem:**
Without combat balance corrections:
- A level-10 player only has ~2× the stats of a level-0 entity (linear scaling)
- Three level-0 NPCs together deal 3× base damage and fill the reaction queue 3× faster
- Higher levels don't provide the survivability advantage players expect from progression
- Queue capacity doesn't reflect investment strategy (raw Focus points lose meaning as levels rise)
- Reaction windows don't reward leveling (a level-10 defender reacts no faster against level-0 mobs than against level-10 mobs)
- No way to efficiently manage low-priority threats in the reaction queue

**We need a system that:**
- Makes stat scaling super-linear so higher-level entities are proportionally stronger
- Ties queue capacity to investment ratio rather than raw attribute points
- Rewards level advantage with wider reaction windows
- Provides a "dismiss" verb for efficient queue management against trivial threats

### Desired Experience

Players should experience:
- **Power fantasy:** A level-10 player feels dominant against level-0 enemies, not threatened
- **Investment payoff:** Putting points into Focus matters relative to total investment, not just raw count
- **Tactical breathing room:** Fighting weaker enemies gives more reaction time
- **Queue management:** Ability to triage threats efficiently instead of being overwhelmed by volume
- **Meaningful progression:** Each level feels more impactful than the last

### Specification Requirements

**System 1 — Super-Linear Stat Scaling:**
- Polynomial level multiplier applied after existing linear formulas
- `effective_stat = linear_stat × (1 + level × k)^p`
- HP scales less aggressively (k=0.1, p=1.5) to preserve danger
- Damage scales more aggressively (k=0.15, p=2.0) to reward offense
- Level 0 multiplier always equals 1.0 (backward compatible)

**System 2 — Queue Capacity by Commitment Ratio:**
- Replace raw-Focus queue scaling with investment ratio
- `commitment_ratio = focus_reach / (total_level × 7)` where 7 = number of attributes × points per level
- Thresholds: <33% → 1 slot, 33-49% → 2 slots, 50-65% → 3 slots, 66%+ → 4 slots
- High commitment to Focus rewards more queue capacity regardless of absolute level

**System 3 — Reaction Window Level Gap:**
- `reaction_window = instinct_base × (1 + max(0, defender_level - attacker_level) × scaling_factor)`
- Defender above attacker: wider windows (more time to react to weaker threats)
- Defender below attacker: narrower windows (stronger enemies are harder to read)
- Cap at reasonable maximum (e.g., 3× base)

**System 4 — Dismiss Mechanic:**
- New verb: skip front queue item, taking full unmitigated damage
- No lockout, no GCD — always available
- Frees queue slot immediately for more important threats
- Rationale: queue bandwidth optimization, not damage avoidance

### MVP Scope

**Phase 1 includes:**
- Super-linear level multiplier formulas in resource calculation
- Commitment-ratio queue capacity
- Reaction window level gap scaling
- Dismiss mechanic (message, server handler, client input)

**Phase 1 excludes:**
- Data-driven tuning UI (constants hardcoded for MVP)
- Per-attribute-type exponent customization (uniform exponents per stat category)
- Dismiss animation/VFX (basic functionality only)
- AI usage of dismiss (player-only for MVP)

### Priority Justification

**HIGH PRIORITY** - Core combat balance is broken; progression feels meaningless without super-linear scaling.

**Why high priority:**
- Fundamental balance problem (linear vs multiplicative) undermines all combat testing
- Attribute investment feels unrewarding (raw points lose relative value)
- Reaction queue overwhelm from mob count makes multi-enemy combat frustrating
- Blocks meaningful playtesting of spatial difficulty system (RFC-014)

**Benefits:**
- Progression feels meaningful (each level compounds)
- Investment strategy matters (commitment ratio, not raw points)
- Multi-enemy combat manageable (dismiss + wider windows)
- Foundation for future difficulty tuning (knobs, not code changes)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Four-System Combat Balance Overhaul**

#### Core Mechanism

**System 1 — Super-Linear Level Multiplier:**

```
effective_stat = linear_stat × level_multiplier(level, k, p)
level_multiplier(level, k, p) = (1 + level × k)^p
```

| Stat Category | k | p | Level 0 | Level 3 | Level 5 | Level 10 |
|---------------|------|------|---------|---------|---------|----------|
| HP | 0.10 | 1.5 | 1.00 | 1.48 | 1.84 | 2.83 |
| Damage | 0.15 | 2.0 | 1.00 | 2.10 | 3.06 | 6.25 |
| Reaction stats | 0.10 | 1.2 | 1.00 | 1.37 | 1.64 | 2.30 |

Applied in `resources.rs` after existing linear derivation (existing formulas unchanged).

**System 2 — Commitment-Ratio Queue Capacity:**

```
commitment_ratio = focus_reach / (total_level × 7)
```

| Commitment Ratio | Queue Slots |
|------------------|-------------|
| < 33% | 1 |
| 33% – 49% | 2 |
| 50% – 65% | 3 |
| ≥ 66% | 4 |

Where `focus_reach` is total Focus investment and `total_level × 7` represents total possible attribute points (7 attributes × 1 point per level). A level-10 with Focus 5 (of 10 total points) = 5/70 ≈ 7% → 1 slot. A level-10 with Focus 7 = 7/70 = 100% of Focus axis → depends on reach calculation.

**System 3 — Reaction Window Level Gap:**

```
reaction_window = instinct_base × (1 + max(0, defender_level - attacker_level) × 0.15)
```

- Defender 10 levels above attacker: `instinct_base × 2.5` (150% bonus)
- Defender equal level: `instinct_base × 1.0` (no bonus)
- Defender below attacker: `instinct_base × 1.0` (no penalty below base, capped at 1.0 minimum multiplier)
- Maximum multiplier cap: 3.0×

**System 4 — Dismiss Mechanic:**

- New message variant: `Try::Dismiss { ent: Entity }`
- Server handler: resolve front threat at full unmitigated damage, free slot
- No GlobalRecovery created (no lockout)
- No GCD interaction (always available)
- Client: keybind (e.g., `D` key or context action)

#### Performance Projections

All four systems add negligible overhead:
- System 1: One extra multiply per stat derivation (runs on spawn/level change, not per frame)
- System 2: One division + threshold check per queue capacity calculation
- System 3: One subtraction + multiply per reaction timer insertion
- System 4: One queue pop per dismiss (same cost as timer expiry)

**Development Time:**
- Phase 1 (Super-linear multiplier): 2-3 hours
- Phase 2 (Commitment-ratio queue): 2-3 hours
- Phase 3 (Reaction window gap): 1-2 hours
- Phase 4 (Dismiss mechanic): 3-4 hours
- **Total: 8-12 hours**

#### Technical Risks

**1. Tuning Balance**
- *Risk:* Exponent values may make high-level entities too strong or too weak
- *Mitigation:* All constants isolated as named constants, easy to adjust via playtesting
- *Impact:* Balancing issue, not technical blocker

**2. Queue Capacity Edge Cases**
- *Risk:* Commitment ratio thresholds may create cliff effects
- *Mitigation:* Thresholds are constants, can be adjusted; consider smoothing if needed
- *Impact:* Low — threshold-based design is intentionally discrete

**3. Dismiss Spam**
- *Risk:* Players might dismiss everything reflexively instead of reacting
- *Mitigation:* Full unmitigated damage makes dismiss costly; it's a triage tool, not a solution
- *Impact:* Self-balancing — overuse is punished by damage taken

### System Integration

**Affected Systems:**
- `resources.rs` — Stat derivation (Systems 1, 3)
- `queue.rs` — Queue capacity calculation (System 2)
- `reaction_queue.rs` — Timer insertion (System 3)
- `message.rs` — New Try::Dismiss variant (System 4)
- `combat.rs` (server) — Dismiss handler (System 4)
- `combat.rs` (client) — Dismiss input binding (System 4)

**Compatibility:**
- ✅ Level 0 multiplier = 1.0 (no regression for existing content)
- ✅ Existing linear formulas preserved (multiplier applied after)
- ✅ Dismiss uses existing queue infrastructure (pop front)
- ✅ Reaction window scaling extends existing Instinct-based timer
- ✅ Queue capacity formula replaces existing Focus-based formula (same interface)

### Alternatives Considered

#### Alternative 1: Flat Stat Bonuses Per Level

Add fixed bonus stats per level instead of polynomial scaling.

**Rejected because:**
- Still linear (doesn't solve the core multiplicative-threat problem)
- Requires different bonus amounts per stat type
- Less tunable than exponent-based approach

#### Alternative 2: Damage Reduction by Level Gap

High-level entities take reduced damage from lower-level attackers.

**Rejected because:**
- Feels bad for the attacker ("why am I doing no damage?")
- Opaque to players (hidden reduction vs visible stat advantage)
- Doesn't solve queue overwhelm problem

#### Alternative 3: Hard Queue Immunity

Ignore threats from entities N levels below defender.

**Rejected because:**
- Binary (immune or not), no gradation
- Removes content entirely (enemies become non-threats)
- Doesn't feel like combat (ignoring attacks is unsatisfying)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** The four systems address different facets of the same root problem (linear vs multiplicative scaling). Each system is independently valuable but they compound when combined:

- Super-linear scaling makes individual stats meaningful at high levels
- Commitment ratio makes investment strategy meaningful regardless of level
- Reaction window gap gives breathing room against weaker threats
- Dismiss provides active queue management for remaining overflow

**Extensibility:**
- Exponent constants can become data-driven (per-creature-type, per-zone)
- Commitment ratio thresholds can be tied to gear/perks
- Dismiss could gain variants (partial-damage dismiss, multi-dismiss)
- Level gap scaling could apply to other mechanics (accuracy, crit chance)

**Formula verification (spot-check):**
- Level 10, HP: `(1 + 10 × 0.1)^1.5 = 2^1.5 = 2.83` ✓
- Level 10, Damage: `(1 + 10 × 0.15)^2.0 = 2.5^2.0 = 6.25` ✓
- Level 5, HP: `(1 + 5 × 0.1)^1.5 = 1.5^1.5 = 1.837` ✓

### PLAYER Validation

**From player perspective:**

**Retained Concepts:**
- ✅ Progression should feel powerful (spec philosophy)
- ✅ Combat decisions should be tactical, not overwhelming (spec "Conscious but Decisive")
- ✅ Build investment should be meaningful (attribute system spec)

**Success Criteria:**
- Level-10 player survives 3× level-0 NPCs comfortably
- Focus investment at 50%+ commitment gives meaningful queue capacity
- Reaction windows against weaker enemies feel generous
- Dismiss is intuitive and tactically interesting (triage, not avoidance)

---

## Approval

**Status:** Approved

**Approvers:**
- ARCHITECT: ✅ Feasible, four independent systems with clean integration points
- PLAYER: ✅ Solves progression feel and queue overwhelm problems

**Scope Constraint:** Fits in one SOW (8-12 hours for 4 phases)

**Dependencies:**
- ADR-005: Derived combat stats (existing — multiplier applied after)
- ADR-006: Server-authoritative reaction queue (existing — dismiss uses queue)
- ADR-017: Universal lockout architecture (existing — dismiss bypasses lockout)
- RFC-014: Spatial difficulty system (existing — provides level-varied enemies to test against)

**Next Steps:**
1. ✅ Created design doc (`docs/00-spec/combat-balance.md`) with full formula reference
2. ✅ Created ADR-020 (super-linear multiplier), ADR-021 (commitment-ratio queue), ADR-022 (dismiss mechanic)
3. ✅ Created SOW-017 with phased implementation plan

**Date:** 2026-02-09
