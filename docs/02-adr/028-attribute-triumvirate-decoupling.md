# ADR-028: Attribute-Triumvirate Decoupling

## Status

Proposed - 2026-02-10

## Context

**Related RFC:** [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)

The previous attribute system defines **attribute leanings** per Triumvirate Approach and Resilience. Each Approach has a primary/secondary/tertiary attribute, and each Resilience likewise. This creates a 7×7 matrix of Approach×Resilience combinations, each with a pre-determined attribute profile.

**Example (old system):**
- Direct Approach → Primary: Vitality, Secondary: Might, Tertiary: Instinct
- Vital Resilience → Primary: Vitality, Secondary: Might, Tertiary: Instinct
- Direct/Vital Berserker → Vitality/Might/Instinct (no build freedom)

This coupling constrains the design space: a Direct fighter *must* invest in Vitality. A Might-primary Direct fighter is impossible by definition. The attribute system and the Triumvirate system are entangled — changing one requires updating the other.

**References:**
- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [Triumvirate Spec](../00-spec/triumvirate.md) — Origin/Approach/Resilience classification
- [Attribute System Spec](../00-spec/attribute-system.md) — Previous coupling tables (Lines 262–329)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md) — New attribute architecture

## Decision

**Fully decouple the attribute system from the Triumvirate.** The three systems are independent layers:

| Layer | Defines | Example |
|-------|---------|---------|
| **Triumvirate** | Behavior and skill kit | Direct/Vital → Charge, Lunge, Regenerate, Endure |
| **Attributes** | Stat scaling (absolute, relative, commitment) | Might-primary, Presence-secondary |
| **Equipment** | Stat modifiers (future RFC) | +3 Force, +2 Impact, +1 Ferocity |

### Core Mechanism

**Before (coupled):**
```
Entity creation:
  1. Choose Origin (Evolved)
  2. Choose Approach (Direct) → locks attribute leanings (Vitality/Might/Instinct)
  3. Choose Resilience (Vital) → locks attribute leanings (Vitality/Might/Instinct)
  4. Attribute spread pre-determined by Approach+Resilience
```

**After (decoupled):**
```
Entity creation:
  1. Choose Origin (Evolved)
  2. Choose Approach (Direct) → determines skill kit (Charge, Lunge)
  3. Choose Resilience (Vital) → determines skill kit (Regenerate, Endure)
  4. Configure bipolar pairs (Axis/Spectrum/Shift) freely — no Triumvirate constraint
```

**For NPCs:**
- NPC archetypes still have characteristic attribute spreads — a Berserker (Direct/Vital) still *tends* toward Might/Vitality
- But this is a data configuration choice, not a system constraint
- Different Berserker variants can have different attribute profiles (a Might-heavy Berserker, a Presence-heavy Berserker)
- The assignment table moves from the attribute system spec to NPC configuration data

**For players (future):**
- Players choose Approach and Resilience for skill kit
- Players configure bipolar pairs (Axis/Spectrum/Shift) independently of Triumvirate
- "What skills do I have?" is separate from "what stats am I good at?"

## Rationale

**Why decouple:**
- **Build freedom:** A Direct fighter can be Might-primary (bruiser), Grace-primary (precision striker), or Presence-primary (crowd controller) — all valid
- **Combinatorial depth:** 7 Approaches × 7 Resiliences × N attribute builds (vs 7 × 7 = 49 fixed profiles)
- **Independent iteration:** Can add new Approach types without updating attribute tables, can rebalance attributes without touching Triumvirate
- **Equipment design space:** If equipment modifies attributes, decoupling means any equipment works with any Triumvirate class

**Why not keep "suggested" leanings:**
- "Suggested" leanings become de facto requirements (optimal builds follow suggestions)
- Creates false choice ("you can build anything, but this is best")
- Better to let emergent meta-game determine optimal builds per playstyle

**Why NPCs still have characteristic spreads:**
- NPCs need defined attributes for balance testing
- Characteristic spreads create recognizable combat patterns (Berserkers hit hard, Defenders are tough)
- Data-driven, not system-enforced — can be changed per encounter zone or variant

## Consequences

**Positive:**
- Full combinatorial build space unlocked (Approach × Resilience × Attributes)
- Attribute system and Triumvirate can evolve independently
- Equipment itemization applies uniformly (no "this weapon is for Direct builds only")
- NPC variety increases (same Triumvirate class, different attribute profiles for difficulty tiers)
- Simpler attribute system spec (no coupling tables to maintain)

**Negative:**
- No built-in "attribute guide" for new players choosing Approach/Resilience
- NPC attribute spreads must be explicitly configured (no auto-derivation from Triumvirate)
- Balance testing surface expands (more valid combinations to consider)

**Mitigations:**
- UI can show "recommended" builds as a tutorial without enforcing them
- NPC configuration is data-driven (spreadsheet/config, not hardcoded)
- Balance testing focuses on commitment tier archetypes (specialist/dual/generalist), not individual attribute combinations

## Implementation Notes

**Removed from spec:**
- Attribute leanings tables (Approach → Primary/Secondary/Tertiary, Resilience → Primary/Secondary/Tertiary)
- Opposing pair analysis (Direct ↔ Distant attribute mirrors)
- Signature skill attribute scaling from leanings

**Preserved:**
- Signature skills themselves (Charge, Lunge, Regenerate, Endure — from Triumvirate spec)
- Skill damage/effect scaling (now uses entity's actual attributes, not leaning-prescribed attributes)
- Triumvirate spec itself (unchanged — Origin/Approach/Resilience definitions)

**Files Affected:**
- `src/common/components/` — Remove archetype-attribute mapping if any exists in code
- NPC spawn/configuration — Attribute spreads become data, not derived from Triumvirate type
- `docs/00-spec/attribute-system.md` — Remove Triumvirate integration section
- `docs/00-spec/triumvirate.md` — No changes needed (Triumvirate is unchanged)

**Migration Strategy:**
- Existing NPC attribute values can be preserved as-is (same numbers, just no longer system-derived)
- Player attribute system (if any exists) gains freedom — no regression
- Tests that assert "Direct NPCs have Vitality primary" should assert specific configured values instead

## References

- [RFC-020: Attribute System Rework](../01-rfc/020-attribute-system-rework.md)
- [Triumvirate Spec](../00-spec/triumvirate.md)
- [Attribute System Spec](../00-spec/attribute-system.md) — Previous coupling (superseded)
- [ADR-026: Three Scaling Modes](026-three-scaling-modes.md)

## Date

2026-02-10
