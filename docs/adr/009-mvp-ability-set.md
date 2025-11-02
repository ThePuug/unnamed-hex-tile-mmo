# ADR-009: MVP Ability Set - Auto-Attack and Stamina-Only Combat

## Status

Proposed

## Context

### Current System State

Based on accepted and proposed ADRs:

1. **ADR-002: Combat Foundation** - Resources (Health, Stamina, Mana), combat state, attribute scaling
2. **ADR-003: Reaction Queue System** - Threat queueing, timer resolution, reaction abilities
3. **ADR-004: Ability System and Targeting** - Directional targeting, tier lock, ability patterns
4. **ADR-005: Damage Pipeline** - Damage calculation, passive modifiers, server authority
5. **ADR-006: AI Behavior** - Enemy behaviors, directional targeting for NPCs

### Game Design Requirements (from combat-system.md)

**Combat Philosophy:**
> "Conscious but Decisive" - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

**MVP Success Criteria:**
- Playable combat loop with Wild Dog enemy
- Positioning matters (not just button mashing)
- Resource management creates decisions
- Reaction queue system functional
- Combat feels responsive and tactical

### Problem: MVP Ability Set Undefined

The spec documents many potential abilities (Charge, Dodge, Counter, Parry, Fortify, Ward, etc.) across different Triumvirate signatures, but doesn't specify which subset to implement for MVP.

**Key Questions:**
1. Which abilities create the minimal viable tactical experience?
2. How many abilities to implement before adding progression systems?
3. Should MVP include both physical and magic damage types?
4. How to incentivize positioning beyond "kite and dodge"?
5. What abilities best test the reaction queue system?

### Design Constraints

**Technical Constraints:**
- No attribute progression system yet (all players start at 0 for all attributes)
- No Triumvirate selection UI (can't choose Approach/Resilience)
- Limited enemy variety (Wild Dog only for Phase 1)
- No magic damage type implemented (physical only for MVP)

**Player Experience Constraints:**
- Must feel engaging with minimal content
- Must teach core combat loop quickly
- Must demonstrate positioning importance
- Must create resource management decisions

## Decision

We will implement a **stamina-only, auto-attack focused** MVP ability set with 4 active abilities and 1 passive ability.

### Ability Set

| Key | Ability | Type | Range | Cost | Effect | Triumvirate Origin |
|-----|---------|------|-------|------|--------|--------------------|
| **Passive** | Auto-Attack | Offensive | 1 hex | Free | 20 dmg/1.5s | N/A (universal) |
| **Q** | Lunge | Offensive | 4 hexes | 20 stam | Gap closer + 40 dmg | Direct signature |
| **W** | Overpower | Offensive | 1 hex | 40 stam | Heavy 80 dmg strike | Overwhelming signature |
| **E** | Knockback | Utility | 2 hexes | 30 stam | Push target 1 hex back | New (positioning tool) |
| **R** | Deflect | Defensive | Self | 50 stam | Clear all queued threats | Hardened (simplified) |

**All abilities cost stamina only** (no mana usage in MVP).

### Core Design Decisions

#### Decision 1: Add Auto-Attack as Passive DPS

**Rationale:**
- **Incentivizes melee engagement**: Free damage when adjacent creates risk/reward (stay close = DPS, but danger)
- **Reduces button mashing**: No need to spam Q for basic attacks
- **Creates positioning incentive**: Staying in melee is valuable (not just "hit once and kite")
- **Frees up ability slot**: Q key available for more interesting gap closer

**Mechanics:**
- Triggers every 1.5 seconds while adjacent to hostile target
- Targets nearest hostile in facing direction (60° cone)
- Pauses when not adjacent (creates decision: chase for DPS or kite for safety)
- 20 physical damage (100% base), scales with Might attribute

**Tradeoff Accepted:**
- More complex than manual basic attack
- Requires "pause when not adjacent" logic to prevent kiting abuse
- Players must learn that melee = passive value

#### Decision 2: Lunge as Primary Offensive Tool (Not Basic Attack)

**Rationale:**
- **Gap closer critical for positioning**: Spec emphasizes positional combat; gap closers enable that
- **Tests directional targeting at range**: Requires facing + tier lock understanding
- **Differentiates from auto-attack**: Lunge = intentional gap close, auto-attack = sustained DPS
- **From Direct signature**: Ties to Triumvirate (teaches build identity later)

**Mechanics:**
- Range 4 hexes (mid-tier) - can close from outside Wild Dog aggro range
- Instantly teleport adjacent to target + deal 40 damage (200% base)
- Costs 20 stamina (cheap enough to use frequently)
- Damage scales with Vitality (Direct approach primary attribute)

**Tradeoff Accepted:**
- Higher complexity than basic melee attack
- Requires teleport implementation (already exists in codebase patterns)

#### Decision 3: Overpower as Heavy Finisher

**Rationale:**
- **High damage option**: Creates "when to use" decision (Overpower now or save stamina?)
- **Adjacent only**: Must be in melee (pairs with auto-attack incentive)
- **From Overwhelming signature**: Demonstrates second Triumvirate style (AoE/dominance theme)
- **Cooldown introduces timing**: 2s cooldown creates pacing (not just resource cost)

**Mechanics:**
- Range 1 hex (adjacent only)
- 80 physical damage (400% base) - highest single-target burst
- Costs 40 stamina (expensive - ~27% of base stamina pool)
- 2 second cooldown prevents spam
- Damage scales with Presence (Overwhelming approach primary attribute)

**Tradeoff Accepted:**
- Two stamina-based damage abilities might feel redundant
- High cost (40 stamina) might make it "never use" if Deflect costs 50

#### Decision 4: Knockback as Positioning Tool

**Rationale:**
- **Creates space without clearing threats**: Pushes enemy back but doesn't clear reaction queue
- **Enables kiting tactics**: Allows tactical retreat when low resources
- **Tests projectile-enemy interactions**: Pushes enemy → reposition → different combat flow
- **Lower cost than Deflect**: 30 stamina vs 50 = more accessible positioning option

**Mechanics:**
- Range 2 hexes (can push before enemy reaches melee)
- Push target 1 hex away from caster
- Does NOT deal damage (pure utility)
- If terrain blocks push, still costs stamina (prevents spamming against walls)
- Cooldown 1.5s (prevents spam, allows tactical usage)

**Tradeoff Accepted:**
- New ability (not from Triumvirate signatures) - might confuse build identity later
- Push mechanics require collision detection implementation
- Doesn't help with reaction queue (only prevents future attacks)

#### Decision 5: Deflect as Expensive Full Queue Clear (Remove Dodge)

**Rationale:**
- **Remove Dodge entirely**: Dodge at 30 stamina becomes "always correct answer" (panic button)
- **Make defense expensive**: Deflect at 50 stamina forces positioning as primary defense
- **Simplify for MVP**: Full queue clear (not partial like signature Deflect) reduces complexity
- **Force risk/reward**: "Use Deflect now (expensive) or reposition (risky)?"

**Mechanics:**
- Self-target (no directional requirement)
- Clears ALL queued threats (simplified from Hardened signature which clears 1 physical threat)
- Costs 50 stamina (~33% of base pool) - very expensive
- 0.5s GCD (standard reaction ability GCD)
- Visual: Shield block + defensive posture

**Why Remove Dodge:**
| Aspect | With Dodge (30 stam) | With Deflect Only (50 stam) |
|--------|----------------------|------------------------------|
| Reaction to full queue | "Press Space, cleared" | "Deflect (expensive) or reposition?" |
| Resource pressure | Low (can Dodge 5 times) | High (can Deflect 3 times) |
| Positioning importance | Low (Dodge solves everything) | High (positioning prevents needing Deflect) |
| Skill expression | Low (panic button spam) | High (must plan positioning) |

**Tradeoff Accepted:**
- More punishing for new players (no forgiving panic button)
- Deflect at 50 stamina might feel "never use" (too expensive)
- Removes Evasive signature ability from game (but it wasn't in MVP scope anyway)

### Resource Economy

**Base Stamina Pool:** 100 + (Might × 0.5) + (Vitality × 0.3) = **100 stamina** at 0 attributes (MVP)

**Stamina Regeneration:** 10/sec (base rate, may scale with attributes later)

**Ability Costs:**
```
Lunge:     20 stamina (20% of pool) - gap closer
Overpower: 40 stamina (40% of pool) - heavy damage
Knockback: 30 stamina (30% of pool) - positioning
Deflect:   50 stamina (50% of pool) - emergency defense
```

**Full Rotation Stamina:**
- Lunge (20) → Overpower (40) → Knockback (30) = **90 stamina**
- Leaves 10 stamina (not enough for Deflect)
- **Forces choice:** "All-in burst OR save stamina for defense"

**Regeneration Time:**
- Empty → Full: 10 seconds at 10/sec
- After Deflect (50 spent): 5 seconds to recover
- After Lunge + Overpower (60 spent): 6 seconds to recover

**Combat Scenario (Wild Dog):**
```
Start: 100 stamina, 200 HP
Auto-attack active (free DPS at 13 dmg/1.5s)

Wild Dog attacks → 15 dmg threat in queue
I Lunge → 80 stamina, dog at 60 HP
Auto-attack dealing damage (staying in melee)
Dog attacks → 15 dmg threat (30 total in queue)
I Overpower → 40 stamina, dog at 0 HP (dead)

Queue resolves → I take 30 damage (170 HP)
Result: Won at 170 HP, 40 stamina remaining
```

**Without careful positioning:**
```
Start: 100 stamina, 200 HP

Dog attacks → 15 dmg threat
I Lunge → 80 stamina
Dog attacks → 15 dmg threat (30 total)
Dog attacks → 15 dmg threat (45 total)
I panic → Deflect → 30 stamina, queue cleared
Dog attacks → 15 dmg threat
Dog attacks → 15 dmg threat (30 total)
I'm out of stamina for Deflect
I Knockback → 0 stamina, dog pushed back
Regenerate while kiting (10/sec)
Dog catches up, attacks → 15 dmg threat (45 total)
Queue resolves → I take 45 damage (155 HP)

Result: Won but resource-starved, took more damage
```

### Integration with Existing Systems

#### Integration with ADR-002 (Combat Foundation)

**Resource Consumption:**
- Lunge/Overpower/Knockback/Deflect all consume stamina via existing stamina component
- Server validates stamina cost before executing ability
- Client predicts stamina reduction locally (instant feedback)
- Server confirms via `Event::Incremental` stamina update

**Combat State:**
- Using any offensive ability sets `CombatState.in_combat = true`
- Auto-attack triggers when `in_combat == true` AND adjacent to hostile
- Combat state exit after 5s without damage dealt/taken (per ADR-002)

#### Integration with ADR-003 (Reaction Queue)

**Deflect Clears Queue:**
- Uses existing `clear_all_threats()` function from reaction queue system
- Triggers 0.5s GCD on all reaction abilities (per ADR-003 spec)
- Visual feedback: queue UI clears instantly

**Knockback Prevents Future Threats (Indirectly):**
- Pushes enemy → distance increases → enemy must pathfind back
- Gives player time to regenerate stamina
- Doesn't clear existing queue (threats still resolve)

#### Integration with ADR-004 (Ability System)

**Lunge Targeting:**
- Uses directional targeting (nearest hostile in 60° facing cone)
- Range: 4 hexes (mid-tier)
- Can use tier lock (press "2") to target mid-range enemies specifically
- Instant execution pattern (no projectile)

**Overpower Targeting:**
- Uses directional targeting (adjacent hostile in facing cone)
- Range: 1 hex (close-tier)
- Instant execution pattern

**Knockback Targeting:**
- Uses directional targeting (nearest hostile in 60° cone)
- Range: 2 hexes
- Instant push effect (no projectile travel time)

#### Integration with ADR-005 (Damage Pipeline)

**Auto-Attack Damage:**
- 20 base physical damage × (1.0 + might / 100.0) × (1.0 - target_armor)
- Uses existing damage pipeline (server calculates, client predicts)
- Enters target's reaction queue (standard threat queueing)

**Lunge Damage:**
- 40 base physical damage × (1.0 + vitality / 100.0) × (1.0 - target_armor)
- Scales with Vitality (Direct approach primary attribute from attribute-system.md)

**Overpower Damage:**
- 80 base physical damage × (1.0 + presence / 100.0) × (1.0 - target_armor)
- Scales with Presence (Overwhelming approach primary attribute)

#### Integration with ADR-006 (AI Behavior)

**Enemy AI Continues Attacking:**
- Wild Dog continues melee pursuit (existing behavior)
- Knockback → Enemy re-pathfinds to player
- Auto-attack doesn't change enemy behavior (passive on player side)

### Component Structure

**No new components required.** All functionality built on existing components:

- `Stamina` (from ADR-002) - resource consumption
- `ReactionQueue` (from ADR-003) - Deflect clears queue
- `Heading` (existing) - directional targeting for all abilities
- `Loc` (existing) - range checks, Knockback push, Lunge teleport
- `CombatState` (from ADR-002) - triggers auto-attack

### System Additions

**New Systems Required:**

1. **Auto-Attack System** (`common/systems/auto_attack.rs`):
   - Query entities with `(Stamina, CombatState, Heading, Loc)`
   - Filter to entities `in_combat == true`
   - Every 1.5 seconds, if adjacent hostile in facing direction exists:
     - Deal 20 base damage (uses damage pipeline)
     - Reset auto-attack timer
   - If not adjacent, pause timer

2. **Ability Execution System Updates** (extend existing from ADR-004):
   - Add `ExecuteLunge` handler: Validate range → teleport → deal damage
   - Add `ExecuteOverpower` handler: Validate adjacent → deal damage → trigger cooldown
   - Add `ExecuteKnockback` handler: Validate range → calculate push direction → move target
   - Add `ExecuteDeflect` handler: Clear queue → consume stamina → trigger GCD

**Modified Systems:**

- **GCD System** (existing): Add cooldown tracking for Overpower (2s), Knockback (1.5s)
- **Targeting System** (from ADR-004): Use for Lunge/Overpower/Knockback range validation

## Consequences

### Positive Consequences

**1. Positioning Becomes Mandatory**
- Auto-attack rewards staying in melee (free DPS)
- Knockback creates space when pressured
- Lunge closes gaps to re-engage
- Deflect is too expensive to spam → must position to avoid damage

**Result:** Players learn positional combat naturally (not just "face and attack").

**2. Resource Management Creates Decisions**
- Can't spam Deflect (50 stamina = 1/2 pool)
- Must choose: "All-in burst (Lunge + Overpower) OR save stamina for defense (Deflect)?"
- Regeneration time (5-10s) creates pacing (can't spam abilities)

**Result:** Combat has tactical depth despite simple enemy AI.

**3. Skill Expression Through Efficiency**
- Good players: Use Knockback + positioning → rarely need Deflect → high stamina efficiency
- Bad players: Spam Deflect → run out of stamina → take more damage
- Great players: Stay in melee for auto-attack DPS → use Knockback proactively → win fast

**Result:** Clear skill ceiling exists even with 4 abilities.

**4. Stamina-Only Simplifies MVP**
- No mana pool UI needed yet
- No magic damage type implementation
- All abilities use same resource (easier to balance)
- Can add mana/magic in Phase 2 (Ward, Fireball, etc.)

**Result:** Faster iteration, simpler testing, cleaner MVP scope.

**5. Auto-Attack Tests Combat Foundation**
- Validates damage pipeline (frequent small attacks)
- Tests reaction queue under sustained pressure
- Tests client prediction (frequent resource changes)
- Tests server authority (auto-attack timing validation)

**Result:** Better system validation before adding complex abilities.

### Negative Consequences

**1. Deflect May Feel Too Expensive**
- 50 stamina = half the base pool
- Players might "never use it" (hoarding resources)
- If too punishing, combat becomes frustrating

**Mitigation:**
- Playtest extensively
- Be willing to reduce cost to 40 stamina if 50 is too harsh
- Consider "partial Deflect" variant (clear first 2 threats for 30 stamina)

**2. Knockback Is Not From Triumvirate**
- Hardcoded utility ability (doesn't tie to build identity)
- Might confuse players when Triumvirate selection added later
- Feels like "everyone has this" (not build-defining)

**Mitigation:**
- Document as "universal utility" in spec
- When adding Triumvirate selection, either:
  - Keep Knockback as universal (like dodge roll in Dark Souls)
  - Replace with signature abilities (Evasive gets Disorient, etc.)

**3. Only 2 Damage Abilities**
- Lunge + Overpower might feel limited
- No ranged option (all melee)
- No AoE (single-target only)

**Mitigation:**
- MVP scope is intentionally limited (test core systems)
- Phase 2 adds Trap (AoE), Volley (ranged), Mark (DoT)
- Current set validates combat loop before adding complexity

**4. Auto-Attack Complexity**
- Requires timer tracking per entity
- Requires "pause when not adjacent" logic
- More complex than "press Q to attack"

**Mitigation:**
- Timer tracking already exists in codebase (GCD, regen, etc.)
- Complexity is in implementation, not player-facing (transparent)
- Benefits (positioning incentive) outweigh implementation cost

**5. Removing Dodge Is Polarizing**
- New players may struggle without panic button
- "Deflect is too expensive" feedback likely
- Some players prefer forgiving mechanics

**Mitigation:**
- Playtesting will reveal if too punishing
- Can add back cheaper defensive option if needed (Fortify at 40 stamina?)
- Document intent: "Positioning over panic buttons" philosophy

### Neutral Impacts

**1. Attribute Scaling Won't Be Felt in MVP**
- All players at 0 attributes → no damage scaling differences
- Lunge scales with Vitality, Overpower scales with Presence, but no one has those stats yet
- Auto-attack scales with Might, but base damage is the same for everyone

**Impact:** Attribute system value not demonstrated until progression added.

**2. Triumvirate Choice Irrelevant for Now**
- Lunge from Direct, Overpower from Overwhelming, but no UI to choose
- Players can't express build identity yet
- All players have identical ability set

**Impact:** Build diversity comes in Phase 2+ (progression system).

**3. No Ranged Combat**
- All abilities melee or short-range (4 hex max)
- Can't test ranged targeting or projectile mechanics
- No kiting vs. melee tactical dynamic

**Impact:** Ranged enemies (Forest Sprite) and abilities (Volley) come in Phase 2.

## Future Considerations

### Phase 2: Add Magic Damage and Mana

**Add Abilities:**
- **Ward (E)**: Clear magic damage threats, costs 40 mana (Shielded signature)
- **Volley (Q)**: Ranged projectile, costs 30 mana (Distant signature)

**Result:**
- Introduces mana resource usage
- Tests magic damage pipeline
- Adds ranged combat option

**Integration Challenge:**
- Need to decide: Replace Knockback (E slot) with Ward, or add 5th/6th ability slots?

### Phase 3: Attribute Progression

**When attributes unlock:**
- Lunge damage increases with Vitality investment
- Overpower damage increases with Presence investment
- Stamina pool increases with Might/Vitality investment
- Build diversity emerges

**Integration Challenge:**
- Players who invested in Might (large stamina pool) vs. Vitality (high Lunge damage) will feel different
- Need to ensure both feel viable (resource efficiency vs. burst damage)

### Phase 4: Triumvirate Selection

**When Approach/Resilience choice added:**
- Players pick Direct → keep Lunge (already implemented)
- Players pick Overwhelming → keep Overpower (already implemented)
- Players pick other Approaches → need signature abilities (Trap, Volley, Counter, etc.)

**Integration Challenge:**
- Do we keep Lunge/Overpower as "universal" or make them exclusive to Direct/Overwhelming builds?
- If exclusive, need to implement 7 approaches × 2 abilities = 14 new offensive abilities
- If universal, Triumvirate selection feels less impactful

**Recommendation:** Keep Lunge/Overpower as universal "starter abilities", add signature abilities on top.

### Potential Iteration: Cheaper Deflect Alternative

**If Deflect at 50 stamina is too punishing:**

**Option A: Reduce Cost**
- Deflect → 40 stamina (same as Overpower)
- Makes defense more accessible

**Option B: Add Fortify**
- Fortify (E): Reduce all queued damage by 50%, costs 40 stamina
- Deflect (R): Clear all threats, costs 50 stamina
- Gives two defensive options (mitigation vs. negation)

**Option C: Partial Deflect**
- Deflect (R): Clear first 2 threats (not all), costs 30 stamina
- More affordable, still requires threat management

### Potential Iteration: Auto-Attack Pause Mechanic

**Current Design:** Auto-attack pauses when not adjacent.

**Alternative:** Auto-attack pauses when moving.

**Rationale:**
- Prevents "circle strafing" while attacking
- Creates "stand and fight" vs. "move and reposition" decision
- More obvious feedback (moving = no DPS)

**Tradeoff:**
- Less obvious (players might not understand why DPS stopped)
- Punishes movement (discourages positional play)

**Recommendation:** Playtest both, choose based on feel.

## Validation Criteria

**Implementation Complete When:**

1. ✅ Auto-attack system triggers every 1.5s when adjacent to hostile
2. ✅ Lunge teleports player adjacent to target (range 4) and deals 40 damage
3. ✅ Overpower deals 80 damage to adjacent target with 2s cooldown
4. ✅ Knockback pushes target 1 hex away (range 2), blocks push if terrain obstacle
5. ✅ Deflect clears entire reaction queue, costs 50 stamina, triggers 0.5s GCD
6. ✅ All abilities cost stamina (no mana usage)
7. ✅ Stamina pool = 100 at 0 attributes, regenerates at 10/sec
8. ✅ Resource costs validated server-side (prevents negative stamina)
9. ✅ Client predicts stamina changes instantly (responsive feel)

**System Validation (Integration Tests):**

1. **Auto-Attack Timing:**
   - Spawn player + Wild Dog adjacent
   - Verify auto-attack triggers every 1.5s (±0.1s tolerance)
   - Move player away → verify auto-attack pauses
   - Move player back → verify auto-attack resumes

2. **Resource Economy:**
   - Execute full rotation: Lunge (20) → Overpower (40) → Knockback (30) → Deflect (50)
   - Verify stamina progression: 100 → 80 → 40 → 10 → fails (insufficient stamina)
   - Wait 5 seconds → verify stamina = 60 (regenerated 50)

3. **Knockback Positioning:**
   - Lunge to Wild Dog (adjacent)
   - Use Knockback → verify dog at distance 1
   - Verify auto-attack pauses (not adjacent anymore)
   - Verify Wild Dog re-pathfinds back to player

4. **Deflect Queue Clear:**
   - Let Wild Dog attack 3 times (3 threats in queue)
   - Use Deflect → verify queue empty
   - Verify stamina reduced by 50
   - Verify GCD active for 0.5s (can't use other reaction abilities)

**Player Experience Validation (Playtest):**

1. "Does staying in melee feel valuable?" (auto-attack DPS)
2. "Does Deflect feel too expensive or appropriately costly?" (resource pressure)
3. "Does Knockback create interesting tactical moments?" (spacing control)
4. "Does combat feel responsive and tactical?" (overall flow)

## Related ADRs

- **ADR-002: Combat Foundation** - Resources, stamina scaling, combat state
- **ADR-003: Reaction Queue System** - Deflect integration, threat clearing
- **ADR-004: Ability System and Targeting** - Directional targeting for all abilities
- **ADR-005: Damage Pipeline** - Damage calculations for auto-attack, Lunge, Overpower
- **ADR-006: AI Behavior** - Enemy response to Knockback positioning

## References

- `docs/spec/combat-system.md` - Combat system design specification
- `docs/spec/attribute-system.md` - Attribute scaling formulas
- `docs/spec/triumvirate.md` - Approach/Resilience signature skills
