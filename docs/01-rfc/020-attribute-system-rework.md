# RFC-020: Attribute System Rework

## Status

**Draft** - 2026-02-10

## Feature Request

### Player Need

From player perspective: **Build identity should emerge from meaningful scaling modes, not a single linear number** — The current attribute system has the right bipolar structure (Axis/Spectrum/Shift) but uses a single scaling mode that conflates progression, build matchups, and identity into one number. Archetype-attribute coupling also constrains build diversity.

**Current Problem:**
Without attribute rework:
- Linear stat scaling fails at level gaps — three level-0 NPCs overwhelm a level-10 player because math doesn't create meaningful power differences
- Single scaling mode conflates progression, build benefit, and identity into one number
- Derived stats (18 total across 6 attributes) are mostly unimplemented
- Archetype-attribute coupling hard-codes which stats each Triumvirate class invests in, constraining build diversity

**We need a system that:**
- Separates progression (how strong you are) from build benefit (how your stats compare to opponents) from identity (who you are as a fighter)
- Preserves the existing bipolar Axis/Spectrum/Shift model as the input mechanism
- Scales super-linearly with level so progression feels meaningful
- Creates build identity through commitment percentage, not raw point totals
- Decouples attributes from Triumvirate so any Origin/Approach/Resilience can invest freely
- Defines clear stat-vs-stat contests for relative combat outcomes

### Desired Experience

Players should experience:
- **Clear identity:** "I'm a Might/Presence build" — commitment tiers at 30/45/60% create distinct identities with hard budget constraints
- **Meaningful progression:** Super-linear absolute stats make each level feel more impactful than the last
- **Build diversity:** Three scaling modes per attribute create a rich combinatorial space on top of the existing bipolar pairs
- **Tactical matchups:** Relative stats (Grace vs Vitality, Might vs Focus, Presence vs Instinct) create attacker-defender contests where build choices matter
- **Equipment anticipation:** Weapons and armor will eventually modify all three scaling modes independently

### Specification Requirements

**Six Attributes (Three Bipolar Pairs):**
- Might ↔ Grace, Vitality ↔ Focus, Instinct ↔ Presence
- Retained from existing Axis/Spectrum/Shift model
- Each attribute's derived value (from its bipolar pair) serves as input to three scaling modes

**Three Scaling Modes Per Attribute:**

**Absolute (Progression):**
- Grows with level via super-linear polynomial multiplier
- Maps to familiar RPG stats: Force (damage), Constitution (HP), etc.
- Level difference dominates quickly — progression metric
- HP exponent > Damage exponent (more exchanges at higher levels)

**Relative (Build Benefit):**
- Compares raw stat values between attacker and defender
- No level scaling — stat difference matters, not level difference
- Three opposing pairs: Grace/Vitality (precision vs toughness), Might/Focus (impact vs composure), Presence/Instinct (dominance vs cunning)
- Lower-level player with heavy investment can win relative contests against higher-level player who neglected opposing stat

**Commitment (Build Identity):**
- Percentage of total budget invested in attribute
- Discrete tiers: T0 (<30%), T1 (30%), T2 (45%), T3 (60%)
- Budget math forces hard choices: T3+T1 = 90%, dual T2 = 90%, triple T1 = 90%
- Does not scale with level — permanent statement of identity
- Concrete stats: Poise (evasion), Concentration (queue capacity), Intensity (cadence)

**Attribute-Triumvirate Decoupling:**
- Triumvirate defines behavior and skill kit
- Attributes define stat scaling
- Equipment defines stat modifiers
- All three are independent layers

### MVP Scope

**Phase 1 includes:**
- Three scaling modes layered on top of existing bipolar attribute model
- Commitment tier calculation from percentage thresholds
- Absolute stat derivation with super-linear level multiplier (reuse existing ADR-020)
- Relative stat contest framework (attacker vs defender)
- Queue capacity driven by Focus → Concentration commitment tier
- Cadence driven by Presence → Intensity commitment tier
- Evasion driven by Grace → Poise commitment tier
- Reaction window driven by Instinct → Cunning relative stat
- Recovery pushback driven by Presence → Dominance relative stat
- Removal of archetype-attribute coupling (NPC spreads become data, not system-derived)

**Phase 1 excludes:**
- Specific tier breakpoint values for Poise/Concentration/Intensity (tuning knobs TBD)
- Super-linear scaling exponent values (tuning knobs TBD, reuse ADR-020 constants)
- Open stats (Technique, Discipline, Intuition, Gravitas, Impact, Ferocity, Grit, Flow)
- Equipment attribute modification system (separate RFC)
- UI for attribute selection/investment
- Prestige/respec system
- Changes to Axis/Spectrum/Shift mechanics or character panel

### Priority Justification

**HIGH PRIORITY** — The attribute system is the foundation for all combat math, equipment design, and build identity.

**Why high priority:**
- Current attribute spec (Axis/Spectrum/Shift) is 89% unimplemented and the sliding-scale model adds complexity without depth
- Combat balance overhaul (RFC-017) already introduced commitment-ratio concepts that this system formalizes and extends
- Every future system (equipment, abilities, PvP balance) depends on stable attribute definitions
- Build diversity is a core game pillar — players need meaningful attribute choices before content scales
- Decoupling from Triumvirate unlocks the full combinatorial design space (7 Approaches × 7 Resiliences × N attribute builds)

**Benefits:**
- Clean foundation for all future combat and progression systems
- Three scaling modes create natural design knobs (tune absolute for progression feel, relative for matchup depth, commitment for identity expression)
- Budget-constrained commitment tiers produce emergent build archetypes without hard-coding them
- Relative stat contests create player-vs-player and player-vs-NPC strategic depth
- Independent from Triumvirate — can iterate either system without breaking the other

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Three-Mode Attribute Architecture**

#### Core Mechanism

**Scaling Modes — Three Sub-Attributes Per Attribute:**

The existing bipolar model (Axis/Spectrum/Shift) produces six derived attribute values (might, grace, vitality, focus, instinct, presence). Three scaling modes are layered on top:

```
Absolute = derived_value × level_multiplier(total_budget, k, p)
Relative = derived_value (compared against opponent's opposing stat at contest time)
Commitment = tier_from_percentage(derived_value / total_budget)
```

**Commitment Tier Calculation:**

```
tier(percentage) = match percentage {
    p if p >= 0.60 => Tier3,
    p if p >= 0.45 => Tier2,
    p if p >= 0.30 => Tier1,
    _ => Tier0,
}
```

Budget constraint analysis:
- T3 + T1 = 60% + 30% = 90% → viable (10% spare)
- T2 + T2 = 45% + 45% = 90% → viable (10% spare)
- T1 + T1 + T1 = 30% + 30% + 30% = 90% → viable (10% spare)
- T3 + T2 = 60% + 45% = 105% → impossible
- T1 × 4 = 120% → impossible

**Relative Stat Contests:**

Three opposing pairs resolved at combat event time:
- Precision (Grace) vs Toughness (Vitality): crit/mitigation on unmitigated threats
- Impact (Might) vs Composure (Focus): recovery-related contest on active reactions
- Dominance (Presence) vs Cunning (Instinct): tempo control (recovery pushback vs reaction window)

Contest outcome = `f(attacker_stat - defender_stat)` — exact formula is a tuning knob.

#### Performance Projections

- **Commitment tier:** One division + threshold check per entity per level-up/respec — negligible
- **Absolute derivation:** Reuses existing level_multiplier (ADR-020) — no new per-frame cost
- **Relative contests:** One subtraction per combat event per pair — negligible
- **Data model:** No struct change — adds methods on existing ActorAttributes

**Development Time:**
- Phase 1 (CommitmentTier + total_budget foundation): 2–3 hours
- Phase 2 (Absolute stat derivation with level multiplier): 3–5 hours
- Phase 3 (Commitment-driven stats: queue, cadence, evasion): 4–6 hours
- Phase 4 (Relative stat contest framework): 4–6 hours
- Phase 5 (Decoupling + integration tests): 3–5 hours
- **Total: 16–25 hours**

#### Technical Risks

**1. NPC Attribute Assignment**
- *Risk:* NPC archetypes currently have hard-coded attribute spreads tied to Triumvirate type
- *Mitigation:* NPCs still have attribute spreads, just no longer forced by Triumvirate. Migration assigns equivalent spreads that happen to match current behavior.
- *Impact:* Low — behavioral equivalence maintained during migration

**2. Commitment Tier Cliff Effects**
- *Risk:* 29% vs 30% investment creates binary identity unlock
- *Mitigation:* Intentional design — discrete tiers are clearer to players than continuous scaling. 30/45/60 thresholds (not 33/50/66) provide buffer.
- *Impact:* None — by design

**4. Open Stats Create Incomplete System**
- *Risk:* Many sub-attributes (Ferocity, Grit, Flow, Impact, etc.) have no concrete stats yet
- *Mitigation:* Explicitly documented as open design space. System is functional without them — concrete stats fill in as gameplay testing reveals needs.
- *Impact:* Low — system works with concrete stats only

### System Integration

**Affected Systems:**
- `src/common/components/` — ActorAttributes (add CommitmentTier, total_budget, scaling mode methods)
- `src/common/systems/combat/resources.rs` — Stat derivation (absolute mode integration)
- `src/common/systems/combat/queue.rs` — Queue capacity (commitment tier for Concentration)
- `src/server/systems/combat.rs` — Damage pipeline (relative stat contests)
- `src/server/systems/ai.rs` — NPC attribute assignment (decouple from archetype)
- Combat balance constants — Refactored for three-mode system

**Compatibility:**
- ✅ Super-linear level multiplier (ADR-020) — reused for absolute mode
- ✅ Commitment-ratio queue capacity (ADR-021) — generalized into commitment tiers
- ✅ Reaction queue (ADR-003/006) — queue capacity now via Concentration tier
- ✅ Universal lockout (ADR-017) — recovery pushback/reduction via relative stats
- ✅ Dismiss mechanic (ADR-022) — unmitigated damage uses absolute stats
- ✅ Triumvirate (spec) — independent axis, no changes needed to Triumvirate

### Alternatives Considered

#### Alternative 1: Replace Bipolar Pairs with Independent Attributes

Remove Axis/Spectrum/Shift entirely. Store six independent u16 fields.

**Rejected because:**
- The bipolar model (Axis/Spectrum/Shift) provides meaningful trade-off mechanics within pairs
- Shift drag in the character panel is a core UX interaction
- Spectrum/Reach concepts add depth to build planning
- Replacing the data model is high-risk with no proportional gameplay benefit — the three scaling modes can layer on top without structural changes

#### Alternative 2: Continuous Commitment Scaling (No Tiers)

Add scaling modes but commitment scales linearly (no discrete tiers).

**Rejected because:**
- Continuous scaling makes build identity fuzzy ("am I a Might build at 25% or 35%?")
- No clear breakpoints for players to target
- Harder to balance around ("every percentage matters" vs "three tiers to tune")
- Discrete tiers are more readable in UI and more memorable for players

#### Alternative 3: Keep Archetype-Attribute Coupling

Add scaling modes but each Triumvirate Approach/Resilience still defines primary/secondary/tertiary.

**Rejected because:**
- Constrains build diversity (Direct always means Vitality-primary)
- Prevents emergent builds (a Might-primary Direct fighter is impossible)
- Couples two independent systems unnecessarily
- Players expect freedom to build "their way" within a chosen playstyle

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** The three scaling modes address three distinct design questions: "How strong am I?" (absolute), "How do I match up against this opponent?" (relative), and "What kind of fighter am I?" (commitment). The existing bipolar model's single linear derivation conflated all three into one number. By layering three modes on the *derived values* from A/S/S, we separate these concerns without replacing the input mechanism.

**Relationship to RFC-017 (Combat Balance):**
- RFC-017's commitment-ratio queue capacity (ADR-021) is a specific instance of the general commitment tier system
- RFC-017's super-linear multiplier (ADR-020) is reused directly for absolute mode
- RFC-017's reaction window gap is refined — now driven by Instinct → Cunning relative stat vs Presence → Dominance
- This RFC generalizes and formalizes patterns that RFC-017 introduced ad-hoc

**Relationship to RFC-018 (NPC Engagement):**
- NPC recovery timers (SOW-018) are independent of attribute system
- Recovery pushback (Presence → Dominance) and recovery reduction (Focus → Composure) interact with lockout timeline
- Hex assignment and positioning strategies are unaffected

**Extensibility:**
- Equipment system (future RFC) plugs into all three modes independently: +Force (absolute), +Precision (relative), +Poise tier (commitment)
- New stats can be assigned to open slots without structural changes
- Commitment tier thresholds are tuning knobs (30/45/60 can shift)
- Relative contest functions are pluggable (linear difference, ratio, sigmoid)

**Supersession Scope:**
- Extends: attribute-system.md (adds three scaling modes; preserves Axis/Spectrum/Shift and bipolar pairs)
- Supersedes: ADR-021 queue capacity formula (generalized into commitment tiers, though ADR-021 remains valid as a specific application)
- Supersedes: Triumvirate-attribute coupling tables in attribute-system.md
- Does NOT supersede: Axis/Spectrum/Shift mechanics, bipolar pair structure, character panel
- Does NOT supersede: ADR-020 (level multiplier reused), ADR-003/006 (queue architecture), ADR-017 (lockout architecture)

### PLAYER Validation

**From player perspective:**

**Retained Concepts:**
- ✅ No dump stats — every attribute contributes to all three scaling modes
- ✅ Meaningful choices — budget constraint forces hard trade-offs at commitment tier boundaries
- ✅ Build diversity — bipolar pairs with A/S/S × three commitment levels × relative stat matchups
- ✅ Progression feels powerful — super-linear absolute scaling (ADR-020)
- ✅ Tactical depth — relative stat contests create rock-paper-scissors dynamics between builds

**Success Criteria:**
- Player can describe their build in 2-3 words ("Might/Presence specialist", "triple T1 generalist")
- Commitment tier choice visibly affects combat (queue capacity, auto-attack speed, evasion)
- Relative stat contests create meaningful matchups (high-Grace attacker vs high-Vitality defender)
- Level progression feels impactful through absolute stat growth
- No Triumvirate class feels "locked" into specific attributes
- Existing character panel (bipolar bars, shift drag, spectrum/reach) preserved

---

## Approval

**Status:** Draft

**Approvers:**
- ARCHITECT: ⏳ Pending
- PLAYER: ⏳ Pending

**Scope Constraint:** Fits in one SOW (16–25 hours for 5 phases)

**Dependencies:**
- ADR-020: Super-linear level multiplier (implemented — reused for absolute mode)
- ADR-021: Commitment-ratio queue capacity (implemented — generalized into commitment tiers)
- ADR-003/006: Reaction queue (implemented — queue capacity driven by Concentration tier)
- ADR-017: Universal lockout (implemented — recovery pushback/reduction via relative stats)
- Triumvirate spec (implemented — decoupled, no changes needed)

**Next Steps:**
1. ✅ Create RFC-020 (this document)
2. Create design doc update (`docs/00-spec/attribute-system.md`) — supersede old system
3. Create ADR-026 (three scaling modes), ADR-027 (commitment tiers), ADR-028 (attribute-Triumvirate decoupling), ADR-029 (relative stat contests)
4. Create SOW-020 with phased implementation plan
5. Update attribute-system-feature-matrix.md

**Date:** 2026-02-10
