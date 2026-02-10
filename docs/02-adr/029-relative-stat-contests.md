# ADR-029: Relative Stats as Stat-vs-Stat Contests

## Status

Proposed - 2026-02-10

## Context

**Related RFC:** [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)

The relative scaling mode (ADR-026) compares raw stat values between attacker and defender. This ADR defines how those comparisons work: which stats oppose which, what the contests determine, and how they interact with existing combat systems.

The key design goal: build investment should matter across level gaps. A lower-level player who committed heavily to one attribute should be able to win relative contests against a higher-level player who neglected the opposing stat. Relative stats deliberately have **no level scaling** — only raw stat difference matters.

**References:**
- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md) — Relative as one of three modes
- [ADR-020: Super-Linear Level Multiplier](020-super-linear-level-multiplier.md) — Absolute scaling (complementary, not overlapping)
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md) — Queue mechanics
- [ADR-017: Universal Lockout + Synergy Architecture](017-universal-lockout-synergy-architecture.md) — Recovery timeline

## Decision

Three relative stat pairs create attacker-vs-defender contests. Each pair maps to a specific combat interaction domain. Contest outcome depends on the **raw stat difference** between attacker and defender — no level multiplier is applied.

### Core Mechanism

**The Three Pairs:**

| Pair | Attacker Stat | Defender Stat | Domain | Interaction |
|------|--------------|---------------|--------|-------------|
| **Grace vs Vitality** | Precision | Toughness | Threat resolution | Crit chance vs damage mitigation on unmitigated/dismissed threats |
| **Might vs Focus** | Impact | Composure | Recovery | Attacker's force vs defender's recovery reduction |
| **Presence vs Instinct** | Dominance | Cunning | Tempo control | Recovery pushback vs reaction window duration |

**Contest Resolution:**

```
contest_delta = attacker_relative_stat - defender_relative_stat
effect = base_effect × modifier(contest_delta)
```

The `modifier` function converts the stat difference into a concrete combat effect. The exact function shape (linear, sigmoid, stepped) is a tuning knob — the architecture supports any monotonic function of the delta.

**Pair 1 — Precision vs Toughness (Grace vs Vitality):**

Applies to unmitigated threat resolution (dismissed threats, expired threats, threats that bypass active reactions):

- Precision (attacker's Grace) → crit chance on the threat
- Toughness (defender's Vitality) → passive damage mitigation
- Higher Precision vs lower Toughness → higher crit rate, less mitigation
- Lower Precision vs higher Toughness → lower crit rate, more mitigation

**Pair 2 — Dominance vs Cunning (Presence vs Instinct):**

Applies to tempo control — the pacing of combat between actions:

- Dominance (attacker's Presence) → recovery pushback (extends target's recovery timer after a hit)
- Cunning (defender's Instinct) → reaction window duration (how long to read and respond to threats)
- High Presence + high Cadence (Intensity commitment) → snowball: frequent hits that each push back recovery, locking down low-Instinct targets
- High Instinct → resists disruption, maintains action tempo

**Pair 3 — Impact vs Composure (Might vs Focus):**

Applies to active reaction resolution:

- Impact (attacker's Might) → not yet mapped to a specific mechanic (open design space)
- Composure (defender's Focus) → recovery reduction (resists Presence → Dominance recovery pushback)
- Composure directly contests Dominance: pushback tries to slow you down, composure keeps you acting

### No Level Scaling

Relative stats use **raw attribute values only**. No level multiplier is applied:

```
// Absolute (progression): uses level multiplier
absolute_damage = raw_might × level_multiplier(level, DAMAGE_K, DAMAGE_P)

// Relative (build matchup): raw stats only
precision_delta = attacker.grace - defender.vitality
```

This means a level-5 entity with Grace 8 has the same Precision as a level-10 entity with Grace 8. The level-10 entity likely has more points to invest (natural advantage), but the scaling is on stat difference, not level difference.

## Rationale

**Why three pairs, not free-form contests:**
- Three pairs map cleanly to three combat domains (threat resolution, recovery, tempo)
- Each pair creates a rock-paper-scissors dynamic between two attributes
- Easier to balance than arbitrary NxN stat interactions
- Players can reason about matchups: "their build has high Grace, I need Vitality"

**Why no level scaling on relative stats:**
- Absolute mode already handles level-based power (ADR-020)
- Adding level scaling to relative would make it redundant with absolute
- Build investment should matter even across level gaps — this is the whole point of relative mode
- A level-5 Focus specialist should resist stagger better than a level-10 who dumped Focus

**Why Precision vs Toughness (not Grace vs Vitality directly):**
- Named sub-attributes create clearer design vocabulary
- "Precision" communicates what Grace does in combat better than "Grace relative"
- Equipment can modify "Precision" without modifying "Grace absolute" or "Poise commitment"
- Designers can reason about "Precision vs Toughness balance" independently

**Why Impact is open:**
- Impact (Might relative) needs a mechanic that isn't redundant with Force (Might absolute, which is damage)
- Candidates: armor penetration, stagger magnitude, knockback distance — all need playtesting to determine which feels best
- Better to leave open than assign a mechanic that needs to change

**Why Composure specifically contests Dominance:**
- Dominance pushes back recovery timers (extending lockout after being hit)
- Composure reduces recovery duration (resisting lockout extension)
- This creates a clear attacker-defender dynamic in the tempo domain
- Focus-invested defenders can maintain their action tempo against Presence-heavy attackers

## Consequences

**Positive:**
- Build matchups create strategic depth (counter-building is meaningful)
- Level-independent — build choices matter at every level gap
- Three distinct combat domains with clear pair assignments
- Extensible — modifier functions can be tuned independently per pair
- Equipment can target specific relative stats

**Negative:**
- Impact (Might relative) is undefined — one of three pairs is incomplete
- Contest functions (modifier shape) are not yet defined — tuning work needed
- Three pairs may not cover all combat interactions (future pairs possible)
- Players must understand which stats oppose which (learning curve)

**Mitigations:**
- Impact being open is explicitly documented; assign when testing reveals the right mechanic
- Contest functions are isolated as tuning knobs — can iterate without structural changes
- Three pairs cover the main combat domains; additional pairs can be added as new mechanics emerge
- UI can show effective matchup rating (abstract "your defense vs their offense" indicator)

## Implementation Notes

**Contest Resolution Timing:**
- Precision vs Toughness: resolved when a threat is resolved (dismissed, expired, or bypassed active reactions)
- Dominance vs Cunning: resolved when inserting a threat (window duration) and when applying a hit (recovery pushback)
- Impact vs Composure: resolved alongside Dominance vs Cunning (recovery-related)

**Integration with Existing Systems:**
- Reaction queue (ADR-006): Cunning affects window duration at threat insertion; replaces level-gap-only modifier (ADR-020)
- Universal lockout (ADR-017): Dominance adds recovery pushback; Composure reduces recovery duration
- Damage pipeline (ADR-010): Precision/Toughness affects crit and mitigation at resolution phase
- Dismiss mechanic (ADR-022): Dismissed threats still subject to Precision vs Toughness for crit determination

**Relationship to ADR-020 Reaction Window Gap:**
- ADR-020 defines a level-gap reaction window modifier
- ADR-029 adds a stat-based modifier (Cunning vs Dominance)
- Both can coexist: `window = base × level_gap_modifier × stat_contest_modifier`
- Or the stat-based modifier can subsume the level-gap modifier if desired (level advantage is already captured by having more stat points to invest)

**Files Affected:**
- Damage resolution pipeline — Precision/Toughness contest
- Threat insertion — Cunning-based reaction window
- Recovery system — Dominance pushback, Composure reduction
- Reaction queue timer — Window duration modification

## References

- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md)
- [ADR-020: Super-Linear Level Multiplier](020-super-linear-level-multiplier.md)
- [ADR-006: Server-Authoritative Reaction Queue](006-server-authoritative-reaction-queue.md)
- [ADR-017: Universal Lockout + Synergy Architecture](017-universal-lockout-synergy-architecture.md)
- [ADR-010: Damage Pipeline Two-Phase Calculation](010-damage-pipeline-two-phase-calculation.md)

## Date

2026-02-10
