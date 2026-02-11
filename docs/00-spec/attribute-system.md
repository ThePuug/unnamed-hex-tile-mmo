# Attribute System

## Core Philosophy

The attribute system uses **three bipolar pairs** of attributes, managed through the **Axis/Spectrum/Shift** input model, with **three scaling modes** layered on top. Each derived attribute value feeds into absolute (progression), relative (build matchup), and commitment (build identity) scaling independently.

**Key Principles:**
- **Bipolar Pairs:** Might ↔ Grace, Vitality ↔ Focus, Instinct ↔ Presence — the Axis/Spectrum/Shift model remains the core input mechanism
- **Three Scaling Modes:** Absolute (progression), Relative (build matchup), Commitment (build identity)
- **Commitment Tiers:** Percentage-based discrete tiers (20/40/60%) create hard build choices
- **Decoupled from Triumvirate:** Attributes, Triumvirate, and equipment are independent layers
- **Attribute Formulas:** Axis×16, Spectrum×12, Shift×12, Axis=0×6 (smooth 160→120 stat curve)

**Related Documents:**
- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-026: Three Scaling Modes](../02-adr/026-three-scaling-modes.md)
- [ADR-027: Commitment Tiers](../02-adr/027-commitment-tiers.md)
- [ADR-028: Attribute-Triumvirate Decoupling](../02-adr/028-attribute-triumvirate-decoupling.md)
- [ADR-029: Relative Stat Contests](../02-adr/029-relative-stat-contests.md)
- [ADR-020: Super-Linear Level Multiplier](../02-adr/020-super-linear-level-multiplier.md)
- [Combat Balance Design Doc](combat-balance.md)

---

## The Six Attributes

| Pair | Left | Right |
|------|------|-------|
| **Might ↔ Grace** | Raw power — the energy behind every action | Technique and precision — efficiency of execution |
| **Vitality ↔ Focus** | Physical constitution — capacity to endure | Mental discipline — clarity under pressure |
| **Instinct ↔ Presence** | Gut sense — reading and adapting to the battlefield | Gravitational force — commanding the space around you |

These six attributes are organized as three **bipolar pairs** using the existing **Axis/Spectrum/Shift** model:

- **Axis:** Base allocation between the two attributes in a pair (determines the bias)
- **Spectrum:** Width of the reachable range (determines flexibility)
- **Shift:** Fine-tuning within the spectrum (runtime adjustment via character panel drag)

The derived values (`might()`, `grace()`, `vitality()`, `focus()`, `instinct()`, `presence()`) serve as input to all three scaling modes below.

---

## Three Scaling Modes

Every attribute has three scaling modes. Each mode serves a different design purpose and scales differently. All modes use the **derived attribute values** from the bipolar model as input.

### Absolute — Progression

Absolute values are the **progression metric**. They grow as you level. Super-linear polynomial scaling means level difference dominates quickly. These stats matter most in even-level fights and are quickly overshadowed by level differences.

| Attribute | Absolute | Stat | Description |
|-----------|----------|------|-------------|
| Might | **Force** | Damage | How hard your attacks hit. Scales super-linearly with level. |
| Grace | **Technique** | — | Raw skill and practiced ability. Grows with experience. |
| Vitality | **Constitution** | HP | Health pool. Scales super-linearly with level. HP exponent > Damage exponent, ensuring higher-level fights have more exchanges. |
| Focus | **Discipline** | — | Trained mental sharpness. Grows with experience. |
| Instinct | **Intuition** | — | Deepening gut sense. Grows with experience. |
| Presence | **Gravitas** | — | Weight of your presence on the battlefield. Grows with power. |

**Super-linear scaling:** Applied as a polynomial level multiplier after computing base stats from attribute points. The multiplier is uniform across all entities at the same level, preserving balance ratios. HP scales with a higher exponent than Damage, ensuring more exchanges at higher levels. See [ADR-020](../02-adr/020-super-linear-level-multiplier.md) for formula details.

**Open absolute stats:** Technique, Discipline, Intuition, and Gravitas do not yet have concrete mechanical stats mapped. They are named and defined as progression categories. Specific stats will be assigned as gameplay testing reveals needs.

### Relative — Build Benefit

Relative values are the **build benefit**. They do not scale with level. Instead, they compare raw stat values between attacker and defender. A higher level likely has more points to invest (giving a natural advantage), but the scaling is on the **stat difference**, not the level difference.

This means a lower-level player who committed heavily to one attribute can still win relative contests against a higher-level player who neglected the opposing stat. Build choices matter even across level gaps.

Relative stats operate in opposing pairs:

| Pair | Attacker Stat | Defender Stat | Interaction |
|------|--------------|---------------|-------------|
| **Grace vs Vitality** | **Precision**: Crit chance | **Toughness**: Mitigation | Your technique finding gaps in their constitution. Applies to unmitigated/dismissed threat resolution. |
| **Might vs Focus** | **Impact**: — | **Composure**: Recovery reduction | Your raw force against their mental discipline. Applies to active reaction resolution. |
| **Presence vs Instinct** | **Dominance**: Recovery pushback | **Cunning**: Reaction window | Your gravitational control against their instinctive adaptation. Applies to tempo control. |

See [ADR-029](../02-adr/029-relative-stat-contests.md) for contest resolution details.

### Commitment — Build Identity

Commitment values are the **build identity**. They are determined by the **percentage of maximum possible** for that attribute: `derived_value / (total_level × 10) × 100`. Commitment defines playstyle at every level, in every fight. It does not inflate with level — it is a permanent statement about who you are as a fighter.

Commitment scales in **discrete tiers**, not linearly:

| Tier | Threshold | Meaning |
|------|-----------|---------|
| Tier 0 | < 20% | No commitment identity for this attribute |
| Tier 1 | ≥ 20% | Unlocks the identity |
| Tier 2 | ≥ 40% | Deepens it |
| Tier 3 | ≥ 60% | Defines you |

**Build constraints with 10 points:**
- **Dual T3** (5+5 axis): 80+80 = 160 total (80% each)
- **T3+2×T2** (4+3+3 axis): 64+48+48 = 160 total
- **4×T2** (pure spectrum): Not achievable (30% each = T1)
- **6×T1** (spectrum spread): Not achievable (dilutes below 20%)

See [ADR-027](../02-adr/027-commitment-tiers.md) for tier calculation details.

| Attribute | Commitment | Stat | Description |
|-----------|-----------|------|-------------|
| Might | **Ferocity** | — | The internal aggression driving your force. |
| Grace | **Poise** | Evasion | Physical grace under fire. Chance to avoid incoming threats entirely. |
| Vitality | **Grit** | — | Mental stubbornness behind physical toughness. Refusing to go down. |
| Focus | **Concentration** | Queue capacity | Sustained mental effort. Determines how many pending threats you can hold in the reaction queue before auto-resolution. |
| Instinct | **Flow** | — | Surrendering conscious thought, letting instinct guide action. |
| Presence | **Intensity** | Cadence | Raw energy projection. Determines auto-attack speed. |

**Open commitment stats:** Ferocity, Grit, and Flow do not yet have concrete mechanical stats mapped. Specific stats will be assigned as gameplay testing reveals needs.

---

## Complete Terminology Matrix

Each attribute has three named sub-attributes, one per scaling mode:

| Attribute | Absolute (progression) | Relative (build benefit) | Commitment (build identity) |
|-----------|----------------------|---------------------------|----------------------------|
| Might | Force: Damage | Impact | Ferocity |
| Grace | Technique | Precision: Crit chance | Poise: Evasion |
| Vitality | Constitution: HP | Toughness: Mitigation | Grit |
| Focus | Discipline | Composure: Recovery reduction | Concentration: Queue capacity |
| Instinct | Intuition | Cunning: Reaction window | Flow |
| Presence | Gravitas | Dominance: Recovery pushback | Intensity: Cadence |

### Framing Sentences

- **Might**: Force is the raw energy generated which grows as you grow stronger. How significant that force is when applied to a target is its impact. The amount of effort you put into generating that force is your ferocity.
- **Grace**: Technique is the practiced skill that grows as you grow more experienced. How effectively that technique finds a target's weakness is its precision. The dedication to honing that technique into fluid movement is your poise.
- **Vitality**: Constitution is the raw endurance that grows as you grow hardier. How well that constitution absorbs punishment from a source is its toughness. The mental stubbornness behind that physical resilience is your grit.
- **Focus**: Discipline is the trained mental strength that grows as you grow sharper. How well that discipline holds under pressure from an opponent is its composure. The dedication to sustaining that mental effort is your concentration.
- **Instinct**: Intuition is the gut sense that deepens as you grow wiser. How well that intuition reads and adapts to the field is your cunning. Letting go of conscious thought and letting that intuition guide your actions is your flow.
- **Presence**: Gravitas is the weight of your presence that grows as you grow more powerful. How effectively that gravitas controls the space around a target is your dominance. The raw energy you project into every action is your intensity.

---

## Interaction with Equipment

Weapons and armor can have these sub-attributes on them. A sword might have +3 Force, +2 Impact, +1 Ferocity — each translating to concrete stat modifications within its scaling mode. This means equipment contributes to progression (absolute), build matchups (relative), and build identity (commitment) independently.

The equipment/weapon system is a **separate RFC** and is not scoped here. This document defines the attribute framework that equipment will modify.

---

## Interaction with the Triumvirate

The Triumvirate (Origin/Approach/Resilience) and the attribute system are **fully independent axes**. A Direct/Vital creature could invest in any attribute spread. A Patient/Hardened creature could be Might-primary or Focus-primary.

The Triumvirate defines behavior and skill kit. Attributes define stat scaling. Equipment defines stat modifiers. All three are independent layers composing a complete entity.

See [ADR-028](../02-adr/028-attribute-triumvirate-decoupling.md) for decoupling rationale.

---

## Interaction with Combat Systems

The following systems interact with this attribute system:

- **Reaction queue (ADR-003/006)**: Queue capacity is driven by Focus → Concentration commitment tier. Reaction window duration is driven by Instinct → Cunning relative stat vs attacker's Presence → Dominance.
- **Universal lockout (ADR-017)**: Recovery reduction (Focus → Composure) and recovery pushback (Presence → Dominance) affect the lockout/recovery timeline.
- **Dismiss mechanic (ADR-022)**: Dismissed threats resolve at full damage minus passive defenses. Crit chance (Grace → Precision) vs mitigation (Vitality → Toughness) determines damage outcome on unmitigated threats.
- **Spatial difficulty (RFC-014)**: Distance from haven determines NPC level, which determines absolute stat scaling.
- **Auto-attack timeline**: One action timeline per entity. Cadence (Presence → Intensity commitment tier) determines auto-attack speed. Recovery pushback/reduction affects the timeline between actions.
- **Super-linear scaling (ADR-020)**: Polynomial level multiplier applied to absolute stats. Level 0 multiplier = 1.0 (backward compatible).
- **Character panel**: Displays bipolar pairs with Axis/Spectrum/Shift visualization. Shift drag redistributes within pairs. Three scaling modes are derived from these values.

---

## What Remains Open

These are intentionally left as design space for future iteration:

**Empty absolute stats:**
- Technique (Grace absolute) — no specific stat yet
- Discipline (Focus absolute) — no specific stat yet
- Intuition (Instinct absolute) — no specific stat yet
- Gravitas (Presence absolute) — no specific stat yet

**Empty relative stats:**
- Impact (Might relative) — no specific stat yet

**Empty commitment stats:**
- Ferocity (Might commitment) — no specific stat yet
- Grit (Vitality commitment) — no specific stat yet
- Flow (Instinct commitment) — no specific stat yet

**Empty commitment tier definitions:**
- Specific tier breakpoints for Poise (evasion %), Concentration (queue slot count), and Intensity (cadence values) are tuning knobs to be determined through playtesting.

**Super-linear scaling exponents:**
- HP and Damage exponents are tuning knobs. HP exponent must be > Damage exponent. Specific values defined in [ADR-020](../02-adr/020-super-linear-level-multiplier.md).

Do **not** fill open cells — document them as explicitly open design space.

---

## Design Goals

- **No dump stats** — Every attribute contributes to all three scaling modes
- **Meaningful choices** — Budget constraint forces hard trade-offs at commitment tier boundaries
- **Build diversity** — Bipolar pairs with A/S/S × three commitment levels × three relative contest pairs
- **Clear identity** — Players can describe their build in 2-3 words
- **Progression fantasy** — Super-linear absolute scaling makes levels feel impactful
- **Tactical matchups** — Relative stat contests create strategic depth between builds
- **Equipment-ready** — Three modes create rich itemization space for future equipment system
- **Triumvirate-independent** — Attribute choices orthogonal to Approach/Resilience choices
- **Preserved UX** — Character panel bipolar bars, shift drag, spectrum/reach all remain

---

**Document Version:** 2.0
**Last Updated:** 2026-02-10
**Maintained By:** Development team
**Extends:** Version 1.0 (adds three scaling modes on top of existing Axis/Spectrum/Shift system)
