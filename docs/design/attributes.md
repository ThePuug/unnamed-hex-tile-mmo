# Attribute System

## Core Philosophy

The attribute system uses **three bipolar pairs** of attributes, managed through the **Axis/Spectrum/Shift** input model, with **three scaling modes** layered on top. Each derived attribute value feeds into absolute (progression), relative (build matchup), and commitment (build identity) scaling independently.

**Key Principles:**
- **Bipolar Pairs:** Might ↔ Grace, Vitality ↔ Focus, Instinct ↔ Presence — the Axis/Spectrum/Shift model remains the core input mechanism
- **Three Scaling Modes:** Absolute (progression), Relative (build matchup), Commitment (build identity)
- **Commitment Tiers:** Percentage-based discrete tiers (20/40/60%) create hard build choices
- **Decoupled from Triumvirate:** Attributes, Triumvirate, and equipment are independent layers
- **Attribute Formulas:** Axis×16, Spectrum×12, Shift×12, Axis=0×6 (smooth 160→120 stat curve)

---

## The Six Attributes

| Pair | Left | Right |
|------|------|-------|
| **Might ↔ Grace** | Raw power — offensive ability potency | Technique and precision — defensive ability potency |
| **Vitality ↔ Focus** | Physical constitution — HP buffer | Mental discipline — sustained exertion (Endurance) |
| **Instinct ↔ Presence** | Gut sense — reading and adapting to the battlefield | Gravitational force — commanding the space around you |

These six attributes are organized as three **bipolar pairs** using the existing **Axis/Spectrum/Shift** model:

- **Axis:** Base allocation between the two attributes in a pair (determines the bias)
- **Spectrum:** Width of the reachable range (determines flexibility)
- **Shift:** Fine-tuning within the spectrum (runtime adjustment via character panel drag)

The derived values (`might()`, `grace()`, `vitality()`, `focus()`, `instinct()`, `presence()`) serve as input to all three scaling modes below.

**Shift constraints:** Shift direction is locked by axis sign (positive axis forces negative shift, vice versa). Pure spectrum builds cannot shift. Shift magnitude is clamped to spectrum value.

---

## Three Scaling Modes

Every attribute has three scaling modes. Each mode serves a different design purpose and scales differently. All modes use the **derived attribute values** from the bipolar model as input.

**Computation timing:** Absolute and commitment values are cached at spawn/level-up/respec (they don't change during combat). Relative values are computed per combat event (each contest is evaluated at event time).

### Absolute — Progression

Absolute values are the **progression metric**. They grow as you level. Super-linear polynomial scaling means level difference dominates quickly. These stats matter most in even-level fights and are quickly overshadowed by level differences.

| Attribute | Absolute | Stat | Description |
|-----------|----------|------|-------------|
| Might | **Force** | Offensive ability potency | Scales the potency of all offensive abilities and auto-attacks — all outgoing offensive effects (Lunge, Overpower, Knockback damage, DoTs, future offensive abilities). Scales super-linearly with level. |
| Grace | **Technique** | Defensive ability potency | Scales the effectiveness of all active defensive responses: Counter, Deflect, Parry, Ward, and all future reaction abilities. Does NOT affect passive mitigation (Toughness). Scales super-linearly with level. |
| Vitality | **Constitution** | HP | Health pool. Scales super-linearly with level. HP exponent > Damage exponent, ensuring higher-level fights have more exchanges. |
| Focus | **Discipline** | Endurance pool | A combat resource that depletes through active exertion. Resets out of combat. Penalties ramp on a continuous curve as Endurance depletes (recovery times lengthen, reaction windows compress). Scales super-linearly with level. |
| Instinct | **Intuition** | — | Deepening gut sense. Grows with experience. |
| Presence | **Gravitas** | — | Weight of your presence on the battlefield. Grows with power. |

**Super-linear scaling:** Applied as a polynomial level multiplier `(1 + level × k)^p` after computing base stats from attribute points. The multiplier is uniform across all entities at the same level, preserving balance ratios. HP scales with a higher exponent than Damage, ensuring more exchanges at higher levels. See [combat-balance.md](combat-balance.md) for formula details and constants.

**Force vs Technique opposition:** Might builds hit hard but active defenses are weak. Grace builds hit lighter but active responses are exceptionally effective. This creates three independent survival axes: Toughness (things you ignore), Technique (things you actively respond to), Endurance (how long you can sustain either approach).

**Discipline vs Constitution opposition:** HP pool vs Endurance pool. Tanky builds absorb punishment but gas out faster. High-Focus builds sustain peak performance longer but have less HP buffer.

**Endurance depletion rules:**
- Offensive ability use: Endurance cost proportional to damage dealt
- Defensive ability use (Counter, Deflect, Parry, Ward): Endurance cost proportional to damage mitigated by the ability
- Dismiss: zero Endurance cost (no active exertion)
- Threat timer expiry: zero Endurance cost (passive, player didn't act)
- Passive mitigation (Toughness): no Endurance interaction
- Penalty curve: continuous (no discrete tiers) — specific curve shape TBD through playtesting

**Open absolute stats:** Intuition and Gravitas do not yet have concrete mechanical stats mapped. They are named and defined as progression categories. Specific stats will be assigned as gameplay testing reveals needs.

### Relative — Build Benefit

Relative values are the **build benefit**. They do not scale with level. Instead, they compare raw stat values between attacker and defender. A higher level likely has more points to invest (giving a natural advantage), but the scaling is on the **stat difference**, not the level difference.

This means a lower-level player who committed heavily to one attribute can still win relative contests against a higher-level player who neglected the opposing stat. Build choices matter even across level gaps.

Relative stats operate in opposing pairs with **rotated oppositions** (different from absolute/commitment layers to prevent single counter-builds):

| Pair | Attacker Stat | Defender Stat | Mechanical Layer | Contest |
|------|--------------|---------------|------------------|---------|
| **Might vs Focus** | **Impact**: Recovery pushback | **Composure**: Recovery reduction | Recovery timeline | Impact extends enemy recovery duration via contest. Composure passively reduces own recovery tick rate. |
| **Grace vs Instinct** | **Finesse**: Synergy recovery reduction | **Cunning**: Reaction window | Lockout-vs-window | Finesse tightens synergy burst sequences via contest. Cunning extends reaction window duration as a multiplier on base window. |
| **Presence vs Vitality** | **Dominance**: Healing reduction (aura) | **Toughness**: Mitigation | Sustain ratio | Dominance reduces healing within range (worst-effect-wins aura). Toughness provides flat damage reduction per hit. |

**Rotated opposition rationale:** Each scaling mode uses a different opposition map to prevent single counter-builds. A build that counters someone's relative stats doesn't automatically counter their absolute or commitment stats.

**Critical hits removed:** The previous Precision (crit chance) mechanic has been removed entirely. Damage is now deterministic and contest-driven.

**Contest resolution:** Two contest patterns implemented via `contest_factor()` and `reaction_contest_factor()`:
- **Nullifying** (`contest_factor`): Effect scales from 0 (equal or losing) to full (large advantage). Used by mitigation, pushback, healing reduction, synergy reduction, recovery speed.
- **Baseline+Bonus** (`reaction_contest_factor`): Baseline preserved at equal stats, bonus scales with advantage. Used by reaction window to ensure a playable floor.

### Commitment — Build Identity

Commitment values are the **build identity**. They are determined by the **percentage of maximum possible** for that attribute: `derived_value / (total_level × 10) × 100`. Commitment defines playstyle at every level, in every fight. It does not inflate with level — it is a permanent statement about who you are as a fighter.

**Why `total_level × 10` (not `total_budget`):** This prevents spectrum builds from being penalized in tier calculation. Axis and spectrum builds with the same derived points compare fairly against a consistent maximum.

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

| Attribute | Commitment | Stat | Description |
|-----------|-----------|------|-------------|
| Might | **Ferocity** | — | The internal aggression driving your force. |
| Grace | **Poise** | — | Physical grace under fire. Currently open. |
| Vitality | **Grit** | — | Mental stubbornness behind physical toughness. Refusing to go down. |
| Focus | **Concentration** | Queue visibility window | Sustained mental effort. Determines how many threats the player can see and interact with in the reaction queue. The queue is unbounded — all threats enter regardless. Threats outside the visibility window still resolve on their timers but the player cannot see or respond to them. |
| Instinct | **Flow** | Threat stacking/compression | Surrendering conscious thought, letting instinct guide action. Similar incoming threats can appear as a single combined threat in the reaction queue. Flow commitment tiers gate how aggressively threats compress. Opposes Concentration (Instinct↔Focus commitment axis). Specific tier definitions TBD through playtesting. |
| Presence | **Intensity** | AoE projection | Raw energy projection. Extends attack ranges and enables hex-native cone targeting at higher tiers. |

**Commitment stat mechanics:**

- **Concentration:** Higher tiers reveal more threats in the visibility window. The queue is unbounded — Concentration controls how many the player can perceive.
- **Intensity tiers:**

| Tier | Targets | Cone | Range Bonus | Damage per Target |
|------|---------|------|-------------|-------------------|
| T0 | 1 (single target) | None | +0 | 100% |
| T1 | 2 | 60° (1 hex direction) | +1 | 80% each when hitting 2 |
| T2 | 3 | 180° (3 hex directions, front half) | +2 | 60% each when hitting 3 |
| T3 | 4 | 300° (5 hex directions, everything except behind) | +3 | 50% each when hitting 4 |

  - All targets in cone take equal damage including the primary target
  - Damage reduction is based on **actual targets hit** — hitting only 1 target at any tier deals 100% damage
  - Cone angles are hex-native: 60° = 1 hex direction, 180° = 3 hex directions, 300° = 5 hex directions

**Open commitment stats:** Ferocity, Poise, and Grit do not yet have concrete mechanical stats mapped. Specific stats will be assigned as gameplay testing reveals needs.

---

## Complete Terminology Matrix

Each attribute has three named sub-attributes, one per scaling mode:

| Attribute | Absolute (progression) | Relative (build benefit) | Commitment (build identity) |
|-----------|----------------------|---------------------------|----------------------------|
| Might | Force: Offensive ability potency | Impact: Recovery pushback | Ferocity |
| Grace | Technique: Defensive ability potency | Finesse: Synergy recovery reduction | Poise |
| Vitality | Constitution: HP | Toughness: Mitigation | Grit |
| Focus | Discipline: Endurance pool | Composure: Recovery reduction | Concentration: Queue visibility window |
| Instinct | Intuition | Cunning: Reaction window | Flow: Threat stacking/compression |
| Presence | Gravitas | Dominance: Healing reduction (aura) | Intensity: AoE projection |

### Framing Sentences

- **Might**: Force is the raw offensive potency that grows as you grow stronger — it drives all offensive abilities and auto-attacks. The tempo pressure that force exerts on an opponent's actions is its impact. The amount of effort you put into generating that force is your ferocity.
- **Grace**: Technique is the practiced defensive skill that grows as you grow more experienced — it determines the power of active defensive responses like Counter, Deflect, Parry, and Ward. How fluidly that technique chains abilities together is its finesse. The physical grace you maintain under fire is your poise.
- **Vitality**: Constitution is the raw durability that grows as you grow hardier. How well that constitution absorbs punishment from a source is its toughness. The mental stubbornness behind that physical resilience is your grit.
- **Focus**: Discipline is the trained mental endurance that grows as you grow sharper — it fuels sustained exertion through the Endurance pool. How well that discipline maintains composure and recovers quickly is its composure. The dedication to sustaining that mental effort is your concentration — it determines how many threats you can perceive and react to.
- **Instinct**: Intuition is the gut sense that deepens as you grow wiser. How well that intuition reads incoming threats and extends your reaction time is your cunning. Letting go of conscious thought and letting that intuition compress and merge similar threats is your flow.
- **Presence**: Gravitas is the weight of your presence that grows as you grow more powerful. How effectively that gravitas suppresses healing in the space around you is your dominance. The raw energy you project outward, striking multiple foes at once, is your intensity.

---

## Interaction with Equipment

Weapons and armor can have these sub-attributes on them. A sword might have +3 Force, +2 Impact, +1 Ferocity — each translating to concrete stat modifications within its scaling mode. This means equipment contributes to progression (absolute), build matchups (relative), and build identity (commitment) independently.

The equipment/weapon system is not scoped in this document. This defines the attribute framework that equipment will modify.

---

## Interaction with the Triumvirate

The Triumvirate (Origin/Approach/Resilience) and the attribute system are **fully independent axes**. A Direct/Vital creature could invest in any attribute spread. A Patient/Hardened creature could be Might-primary or Focus-primary.

The Triumvirate defines behavior and skill kit. Attributes define stat scaling. Equipment defines stat modifiers. All three are independent layers composing a complete entity.

**NPC attribute spreads are data-driven, not system-derived.** NPCs may have characteristic attribute distributions (e.g., Berserkers tend toward Might-heavy) but these are configured in data, not enforced by their Triumvirate class. There are no "suggested leanings" by design — suggested leanings become de facto requirements and create false choice.

---

## Interaction with Combat Systems

The following systems interact with this attribute system:

- **Offensive abilities**: Force (Might → absolute) scales the potency of all offensive abilities and auto-attacks: Lunge, Overpower, Knockback damage, DoTs, and all future offensive effects.
- **Defensive abilities**: Technique (Grace → absolute) scales all active defensive responses: Counter, Deflect, Parry, Ward, and all future reaction abilities. Does NOT affect passive mitigation (Toughness).
- **Endurance pool**: Discipline (Focus → absolute) provides the Endurance resource pool. Depletes through offensive ability use (cost proportional to damage dealt) and defensive ability use (cost proportional to damage mitigated). Dismiss and threat timer expiry cost zero. Penalties ramp on a continuous curve as Endurance depletes.
- **Reaction queue**: Queue visibility window is driven by Focus → Concentration commitment tier. The queue is unbounded — all threats enter. Concentration determines how many the player can see and interact with. Threats outside the window still resolve on their timers. Reaction window duration is driven by Instinct → Cunning relative stat (extends threat timer duration).
- **Threat compression**: Flow (Instinct → commitment tier) gates how aggressively similar threats merge in the reaction queue.
- **AoE projection**: Intensity (Presence → commitment tier) extends attack ranges (+1/+2/+3 hex at T1/T2/T3) and enables hex-native cone targeting (60°/180°/300°) that hits multiple targets with damage split based on actual targets hit.
- **Universal lockout**: Recovery reduction (Focus → Composure) and recovery pushback (Might → Impact) affect the lockout/recovery timeline. Synergy recovery reduction (Grace → Finesse) vs reaction window (Instinct → Cunning) creates the lockout-vs-window equation: `chain_gap + reaction_window > lockout`.
- **Dismiss mechanic**: Dismissed threats resolve at full damage minus passive defenses. Zero Endurance cost. Vitality → Toughness provides mitigation.
- **Healing system**: Healing reduction aura (Presence → Dominance) opposes mitigation (Vitality → Toughness) on the sustain ratio layer.
- **Spatial difficulty**: Distance from haven determines NPC level, which determines absolute stat scaling.
- **Auto-attack timeline**: One action timeline per entity. Recovery pushback/reduction affects the timeline between actions. Cadence (auto-attack speed) is currently a legacy implementation with no attribute home — see homeless mechanics.
- **Super-linear scaling**: Polynomial level multiplier applied to absolute stats. Level 0 multiplier = 1.0 (backward compatible).
- **Character panel**: Displays bipolar pairs with Axis/Spectrum/Shift visualization. Shift drag redistributes within pairs. Three scaling modes are derived from these values.

---

## What Remains Open

These are intentionally left as design space for future iteration:

**Open absolute stats:**
- Intuition (Instinct absolute) — no specific stat yet
- Gravitas (Presence absolute) — no specific stat yet

**Open commitment stats:**
- Ferocity (Might commitment) — no specific stat yet
- Poise (Grace commitment) — no longer evasion, currently open
- Grit (Vitality commitment) — no specific stat yet

**Homeless mechanics:**
- **Crit** — removed from relative layer in ADR-031, no new home assigned
- **Cadence** — removed from Intensity commitment, no new home assigned
- **Evasion** — removed from Poise commitment, no new home assigned

**Super-linear scaling:**
- HP exponent must be > Damage exponent (ensures more exchanges at higher levels). See [combat-balance.md](combat-balance.md) for current values.

**Endurance depletion curve:**
- Continuous penalty ramp shape TBD through playtesting

**Flow tier definitions:**
- Threat compression aggressiveness per tier TBD through playtesting

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

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Approach/Resilience leanings | Attributes have suggested leanings per Triumvirate choice | No leanings — attributes fully decoupled from Triumvirate | ADR-028; prevents forced builds |
| 2 | 18-stat derived table | Each pair produces 6 named derived stats | Three scaling modes with named sub-attributes per mode | Cleaner model, same expressive power |
| 3 | Prestige redistribution | Prestige system for reallocating attributes | Not implemented | Deferred to future design |
| 4 | Overclock mechanic | Temporary stat boost beyond normal limits | Not implemented | Deferred to future design |
| 5 | Critical hit system | Instinct drives crit chance/multiplier | Removed entirely (ADR-031) | Damage now deterministic and contest-driven |
| 6 | NPC attribute generation | Suggested leanings per archetype | Data-driven from EnemyArchetype, no leanings | ADR-028; archetype defines stats directly |
| 7 | Concentration | Queue capacity (hard limit with overflow) | Queue visibility window (unbounded queue, ADR-030) | Overflow punishment replaced by visibility mechanic |
| 8 | Intensity | AoE projection (cone targeting, range extension) | Cadence (auto-attack speed) still implemented as legacy behavior | AoE projection designed but not yet built; cadence is a homeless mechanic |
| 9 | Poise | Open (no mechanic assigned) | Evasion (dodge chance) still implemented as legacy behavior | Evasion is a homeless mechanic; Poise awaiting new assignment |

## Implementation Gaps

**Medium:** Attribute-Triumvirate decoupling migration (remove archetype-attribute coupling code)

**Not yet implemented (absolute stats):** Endurance pool (Focus → Discipline) — depletion through ability use, continuous penalty curve

**Not yet implemented (commitment stats):** Intensity AoE projection (cone targeting, range extension), Flow threat compression (tier-gated merging)

**Legacy commitment stats still active:** Cadence (auto-attack speed from Intensity tier) and Evasion (dodge chance from Poise tier) remain implemented and actively used. These are intended to be replaced by AoE projection and a new Poise mechanic respectively, but the legacy implementations are the current live behavior.

**Open design space:** Intuition, Gravitas (absolute); Ferocity, Poise, Grit (commitment) — intentionally unmapped, see "What Remains Open" above

**Homeless mechanics:** Crit (removed from relative layer, ADR-031), Cadence (removed from Intensity commitment), Evasion (removed from Poise commitment) — no new homes assigned

**Post-MVP:** Equipment attribute modifiers, full healing system, Endurance depletion curve tuning, Flow compression tier definitions

---

**Cross-reference updates needed:**
- [combat.md](combat.md) lines 570, 851, 867, 981, 1197 reference "cadence from Intensity tier" — need separate update to reflect cadence as homeless mechanic

**Related Design Documents:**
- [Combat System](combat.md) — How attributes feed into combat mechanics
- [Combat Balance](combat-balance.md) — Super-linear scaling formulas, tuning constants
- [Triumvirate](triumvirate.md) — Independent classification system (decoupled from attributes)
