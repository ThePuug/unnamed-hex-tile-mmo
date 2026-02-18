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
| **Might ↔ Grace** | Raw power — offensive damage scaling | Technique and precision — defensive damage scaling |
| **Vitality ↔ Focus** | Physical constitution — capacity to endure | Mental discipline — clarity under pressure |
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
| Might | **Force** | Offensive Damage | How hard your offensive attacks hit. Scales super-linearly with level. |
| Grace | **Technique** | Defensive Damage | How hard your defensive/reactive abilities hit. Scales super-linearly with level. |
| Vitality | **Constitution** | HP | Health pool. Scales super-linearly with level. HP exponent > Damage exponent, ensuring higher-level fights have more exchanges. |
| Focus | **Discipline** | — | Trained mental sharpness. Grows with experience. |
| Instinct | **Intuition** | — | Deepening gut sense. Grows with experience. |
| Presence | **Gravitas** | — | Weight of your presence on the battlefield. Grows with power. |

**Super-linear scaling:** Applied as a polynomial level multiplier `(1 + level × k)^p` after computing base stats from attribute points. The multiplier is uniform across all entities at the same level, preserving balance ratios. HP scales with a higher exponent than Damage, ensuring more exchanges at higher levels. See [combat-balance.md](combat-balance.md) for formula details and constants.

**Open absolute stats:** Discipline, Intuition, and Gravitas do not yet have concrete mechanical stats mapped. They are named and defined as progression categories. Specific stats will be assigned as gameplay testing reveals needs.

### Relative — Build Benefit

Relative values are the **build benefit**. They do not scale with level. Instead, they compare raw stat values between attacker and defender. A higher level likely has more points to invest (giving a natural advantage), but the scaling is on the **stat difference**, not the level difference.

This means a lower-level player who committed heavily to one attribute can still win relative contests against a higher-level player who neglected the opposing stat. Build choices matter even across level gaps.

Relative stats operate in opposing pairs with **rotated oppositions** (different from absolute/commitment layers to prevent single counter-builds):

| Pair | Attacker Stat | Defender Stat | Mechanical Layer | Contest Equation |
|------|--------------|---------------|------------------|------------------|
| **Might vs Focus** | **Impact**: Recovery pushback | **Composure**: Recovery reduction | Recovery timeline | Impact extends enemy recovery by 25% of max duration × contest_modifier. Composure reduces own recovery tick rate passively. |
| **Grace vs Instinct** | **Finesse**: Synergy recovery reduction | **Cunning**: Reaction window | Lockout-vs-window | Finesse tightens synergy burst sequences via contest. Cunning extends reaction window duration (2ms per point, capped at 600ms). |
| **Presence vs Vitality** | **Dominance**: Healing reduction (aura) | **Toughness**: Mitigation | Sustain ratio | Dominance reduces healing within 5 hex radius (worst-effect-wins). Toughness provides flat damage reduction per hit. |

**Rotated opposition rationale:** Each scaling mode uses a different opposition map to prevent single counter-builds. A build that counters someone's relative stats doesn't automatically counter their absolute or commitment stats.

**Critical hits removed:** The previous Precision (crit chance) mechanic has been removed entirely. Damage is now deterministic and contest-driven.

**Contest resolution function:** `contest_modifier()` uses clamped linear scaling [0.5, 1.5] with K=200, reused for all pairs.

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
| Grace | **Poise** | Evasion | Physical grace under fire. Chance to avoid incoming threats entirely. |
| Vitality | **Grit** | — | Mental stubbornness behind physical toughness. Refusing to go down. |
| Focus | **Concentration** | Queue capacity | Sustained mental effort. Determines how many pending threats you can hold in the reaction queue before auto-resolution. |
| Instinct | **Flow** | — | Surrendering conscious thought, letting instinct guide action. |
| Presence | **Intensity** | Cadence | Raw energy projection. Determines auto-attack speed. |

**Implemented commitment stat values:**

| Stat | T0 | T1 | T2 | T3 |
|------|-----|-----|-----|-----|
| Poise (evasion %) | 0% | 10% | 20% | 30% |
| Concentration (queue slots) | 1 | 2 | 3 | 4 |
| Intensity (cadence ms) | 3000 | 2500 | 2000 | 1500 |

**Open commitment stats:** Ferocity, Grit, and Flow do not yet have concrete mechanical stats mapped. Specific stats will be assigned as gameplay testing reveals needs.

---

## Complete Terminology Matrix

Each attribute has three named sub-attributes, one per scaling mode:

| Attribute | Absolute (progression) | Relative (build benefit) | Commitment (build identity) |
|-----------|----------------------|---------------------------|----------------------------|
| Might | Force: Offensive Damage | Impact: Recovery pushback | Ferocity |
| Grace | Technique: Defensive Damage | Finesse: Synergy recovery reduction | Poise: Evasion |
| Vitality | Constitution: HP | Toughness: Mitigation | Grit |
| Focus | Discipline | Composure: Recovery reduction | Concentration: Queue capacity |
| Instinct | Intuition | Cunning: Reaction window | Flow |
| Presence | Gravitas | Dominance: Healing reduction (aura) | Intensity: Cadence |

### Framing Sentences

- **Might**: Force is the raw energy generated which grows as you grow stronger. The tempo pressure that force exerts on an opponent's actions is its impact. The amount of effort you put into generating that force is your ferocity.
- **Grace**: Technique is the practiced skill that grows as you grow more experienced — it determines the power of defensive and reactive abilities like Counter. How fluidly that technique chains abilities together is its finesse. The dedication to honing that technique into fluid movement is your poise.
- **Vitality**: Constitution is the raw endurance that grows as you grow hardier. How well that constitution absorbs punishment from a source is its toughness. The mental stubbornness behind that physical resilience is your grit.
- **Focus**: Discipline is the trained mental strength that grows as you grow sharper. How well that discipline maintains composure and recovers quickly is its composure. The dedication to sustaining that mental effort is your concentration.
- **Instinct**: Intuition is the gut sense that deepens as you grow wiser. How well that intuition reads incoming threats and extends your reaction time is your cunning. Letting go of conscious thought and letting that intuition guide your actions is your flow.
- **Presence**: Gravitas is the weight of your presence that grows as you grow more powerful. How effectively that gravitas suppresses healing in the space around you is your dominance. The raw energy you project into every action is your intensity.

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

- **Offensive abilities**: Force (Might → absolute) scales offensive abilities: Lunge (100% Force), Overpower (150% Force), AutoAttack (50% Force), Volley (100% Force).
- **Defensive abilities**: Technique (Grace → absolute) scales defensive/reactive abilities: Counter reflected damage (20% Technique base + 30% threat damage, capped at 200% Technique).
- **Reaction queue**: Queue capacity is driven by Focus → Concentration commitment tier. Reaction window duration is driven by Instinct → Cunning relative stat (extends threat timer duration).
- **Universal lockout**: Recovery reduction (Focus → Composure) and recovery pushback (Might → Impact) affect the lockout/recovery timeline. Synergy recovery reduction (Grace → Finesse) vs reaction window (Instinct → Cunning) creates the lockout-vs-window equation: `chain_gap + reaction_window > lockout`.
- **Dismiss mechanic**: Dismissed threats resolve at full damage minus passive defenses. Vitality → Toughness provides mitigation.
- **Healing system**: Healing reduction aura (Presence → Dominance) opposes mitigation (Vitality → Toughness) on the sustain ratio layer.
- **Spatial difficulty**: Distance from haven determines NPC level, which determines absolute stat scaling.
- **Auto-attack timeline**: One action timeline per entity. Cadence (Presence → Intensity commitment tier) determines auto-attack speed. Recovery pushback/reduction affects the timeline between actions.
- **Super-linear scaling**: Polynomial level multiplier applied to absolute stats. Level 0 multiplier = 1.0 (backward compatible).
- **Character panel**: Displays bipolar pairs with Axis/Spectrum/Shift visualization. Shift drag redistributes within pairs. Three scaling modes are derived from these values.

---

## What Remains Open

These are intentionally left as design space for future iteration:

**Empty absolute stats:**
- Discipline (Focus absolute) — no specific stat yet
- Intuition (Instinct absolute) — no specific stat yet
- Gravitas (Presence absolute) — no specific stat yet

**Relative stats (not yet implemented):**
- Impact (Might relative) — Recovery pushback on hit
- Finesse (Grace relative) — Synergy recovery reduction via contest
- Composure (Focus relative) — Recovery reduction (passive tick rate modifier)
- Cunning (Instinct relative) — Reaction window extension
- Dominance (Presence relative) — Healing reduction aura (5 hex radius, worst-effect-wins)
- Toughness (Vitality relative) — Flat damage mitigation per hit (implemented)

**Empty commitment stats:**
- Ferocity (Might commitment) — no specific stat yet
- Grit (Vitality commitment) — no specific stat yet
- Flow (Instinct commitment) — no specific stat yet

**Super-linear scaling exponents:**
- HP and Damage exponents are tuning knobs. HP exponent must be > Damage exponent. See [combat-balance.md](combat-balance.md) for current values.

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

## Implementation Gaps

**Medium:** Attribute-Triumvirate decoupling migration (remove archetype-attribute coupling code)

**Not yet implemented (relative stats):** Impact/Composure (recovery timeline), Finesse/Cunning (lockout-vs-window), Dominance/Toughness (sustain ratio) — all designed in ADR-031, awaiting implementation

**Open design space:** Discipline, Intuition, Gravitas (absolute); Ferocity, Grit, Flow (commitment) — intentionally unmapped, see "What Remains Open" above

**Post-MVP:** Equipment attribute modifiers, commitment tier tuning for Poise/Intensity breakpoints, full healing system

---

**Related Design Documents:**
- [Combat System](combat.md) — How attributes feed into combat mechanics
- [Combat Balance](combat-balance.md) — Super-linear scaling formulas, queue capacity thresholds
- [Triumvirate](triumvirate.md) — Independent classification system (decoupled from attributes)
