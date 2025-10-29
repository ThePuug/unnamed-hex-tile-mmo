# Combat System

## Core Philosophy

**"Conscious but Decisive"** - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

**Design Pillars:**
* Hex-based targeting (abilities target hexes, not pixel-perfect hitboxes)
* Reaction-based defense (incoming damage enters a queue with time to respond)
* Resource management over cooldown juggling (stamina/mana costs, minimal cooldowns)
* Build identity shapes playstyle (attributes determine reaction capacity and offensive power)

**Inspiration:** MOBA-style ability targeting meets Monster Hunter's deliberate combat pacing, adapted for hex grid MMO.

---

## Offensive Layer

### Targeting System

All offensive abilities target **hexes or hex patterns**, not entities directly.

**Targeting Types:**
* **Single Hex** - Click target hex, ability affects that hex
* **Line Pattern** - Affects N hexes in a line from caster
* **Radius Pattern** - Affects all hexes within distance R from target hex
* **Adjacent** - Affects hexes directly adjacent to caster

**Hit Detection:** If an entity occupies the targeted hex when the ability resolves, they are hit.

### Attack Execution Patterns

**Instant Attacks:**
* Resolve immediately on cast
* Typically melee/adjacent hex abilities
* Example: Basic sword strike, Charge (Direct signature)

**Projectile Attacks:**
* Visible projectile travels across hexes
* Provides visual warning before impact
* Speed varies by ability (arrow fast, fireball slow)
* Example: Volley (Distant signature), basic ranged attack

**Ground Effects:**
* Telegraph appears on target hexes before damage
* Fixed delay before resolution (1-3 seconds)
* Example: Eruption (radiates outward), Trap (Ambushing signature)

**Unavoidable Attacks:**
* Bypass reaction queue system entirely
* Apply damage immediately with passive modifiers only
* Rare, expensive, ultimate-tier abilities
* Distinct visual/audio cues (cannot be mistaken for normal attacks)
* Example: True Strike, Piercing Shot

---

## Defensive Layer: Reaction Queue System

### Core Mechanic

When damage would hit an entity, it enters their **reaction queue** before applying damage.

**Queue Properties:**
* Each queued threat has an independent timer (circular progress indicator)
* Queue capacity = f(Focus attribute)
* Timer duration = f(Instinct attribute)
* When queue is full, **oldest threat resolves immediately** with passive modifiers
* Entity can use **reaction abilities** to clear threats before timers expire

### Queue Display

**Visual Representation:**
* Row of circular icons with depleting timers
* Left to right = order of resolution (soonest first)
* Each icon shows attack type/source (physical icon, fire icon, etc.)
* Timer ring depletes clockwise around icon

**Example:**
```
[⚔️ ●●○○○] [🔥 ●●●○○] [⚔️ ●●●●○]
  0.4s left   0.8s left   1.2s left
```

### Attribute Scaling

**Instinct (Reaction Speed):**
```
reaction_window = base_window * (1.0 + instinct / 200.0)

Examples:
- Instinct = -100: 0.5s window
- Instinct = 0: 1.0s window
- Instinct = 100: 1.5s window
```

**Focus (Mental Clarity):**
```
queue_capacity = base_capacity + floor(focus / 33.0)

Examples:
- Focus = -100: 1 threat capacity (everything else instant)
- Focus = 0: 3 threat capacity
- Focus = 100: 6 threat capacity
```

### Queue Resolution

**When timer expires (no reaction):**
* Damage applies with passive modifiers (armor, resistance, etc.)
* Threat removed from queue
* Next threat begins resolving

**When queue fills (new threat arrives):**
* **Oldest threat (leftmost) resolves immediately** with passive modifiers
* New threat takes rightmost position in queue
* All other threats shift left visually

**When reaction ability used:**
* Ability determines what clears (see Reaction Abilities below)
* Cleared threats removed from queue
* Remaining threats shift left to fill gaps

---

## Reaction Abilities

Abilities that interact with the reaction queue. Tied to Triumvirate signatures (build-dependent, not universal).

### Full Clear Abilities

Clear **entire queue** when activated.

**Dodge (Evasive signature):**
* Cost: 30 stamina
* Effect: Evade all queued threats
* Visual: Character dash/blur effect
* Audio: Whoosh sound

**Ward (Shielded signature):**
* Cost: 40 mana
* Effect: Magic shield absorbs all queued magic damage
* Visual: Glowing barrier appears
* Audio: Crystalline chime

**Fortify (Hardened signature):**
* Cost: 40 stamina
* Effect: Reduce all queued physical damage by 50%, then apply
* Visual: Character braces, metallic sheen
* Audio: Metal clang

### Selective Clear Abilities

Clear **first N threats** in queue (leftmost).

**Counter (Patient signature):**
* Cost: 35 stamina
* Effect: Reflect first queued threat back to attacker
* Clears: 1 threat (leftmost)
* Visual: Parry animation, attack bounces back
* Audio: Clashing metal

**Deflect (Hardened signature):**
* Cost: 30 stamina
* Effect: Negate first queued physical attack
* Clears: 1 threat (leftmost, physical only)
* Visual: Shield block animation
* Audio: Impact thud

**Parry (Primal signature):**
* Cost: 25 stamina
* Effect: Negate first queued attack, brief stagger on attacker
* Clears: 1 threat (leftmost)
* Visual: Weapon parry, enemy recoils
* Audio: Sharp clang + grunt

### Modification Abilities

Do **not clear queue**, but modify outcome.

**Endure (Vital signature):**
* Cost: 20 stamina
* Effect: +50% stagger resist, damage still applies but no interrupt
* Clears: 0 threats
* Visual: Character glows with determination
* Audio: Deep breath

**Dispel (Mental signature):**
* Cost: 30 mana
* Effect: Remove all debuffs from self, does not affect damage queue
* Clears: 0 threats
* Visual: Shimmering aura cleanses status effects
* Audio: Purifying tone

### Global Cooldown (GCD)

All reaction abilities share a **0.5 second global cooldown** to prevent spam.

* Using any reaction ability triggers GCD
* During GCD, no other reaction abilities can be used
* Queue timers continue during GCD (risk of overflow)

---

## Resources

### Stamina

**Purpose:** Physical actions (dodges, blocks, physical abilities)

**Attributes:**
* Pool size scales with **Might** (primary) and **Vitality** (secondary)
* Regeneration rate: Base 10/sec (may scale with attributes later)
* Regenerates in and out of combat

**Base Values:**
```
stamina_pool = 100 + (might * 0.5) + (vitality * 0.3)

Examples:
- Might=0, Vitality=0: 100 stamina
- Might=100, Vitality=0: 150 stamina
- Might=100, Vitality=100: 180 stamina
```

### Mana

**Purpose:** Magic actions (spells, wards, mental abilities)

**Attributes:**
* Pool size scales with **Focus** (primary) and **Presence** (secondary)
* Regeneration rate: Base 8/sec (may scale with attributes later)
* Regenerates in and out of combat

**Base Values:**
```
mana_pool = 100 + (focus * 0.5) + (presence * 0.3)

Examples:
- Focus=0, Presence=0: 100 mana
- Focus=100, Presence=0: 150 mana
- Focus=100, Presence=100: 180 mana
```

---

## Damage Calculation

### Outgoing Damage

**Physical Damage:**
```
damage = base_damage * (1.0 + might / 100.0) * (1.0 - target_armor)
```

**Magic Damage:**
```
damage = base_damage * (1.0 + focus / 100.0) * (1.0 - target_resistance)
```

**Critical Hits:**
```
crit_chance = base_crit + (instinct / 200.0)
crit_multiplier = 1.5 + (instinct / 200.0)

If crit: damage *= crit_multiplier
```

### Passive Modifiers (No Reaction)

When damage resolves without reaction ability, passive modifiers apply:

**Armor (Physical Reduction):**
```
armor = base_armor + (vitality / 200.0)

Examples:
- Vitality=-100: 0% reduction
- Vitality=0: 0% reduction
- Vitality=100: 50% reduction (capped at 75% max)
```

**Resistance (Magic Reduction):**
```
resistance = base_resistance + (focus / 200.0)

Examples:
- Focus=-100: 0% reduction
- Focus=0: 0% reduction
- Focus=100: 50% reduction (capped at 75% max)
```

**Stagger Resist:**
```
stagger_resist = vitality / 100.0

Determines likelihood of being interrupted during cast/channel.
```

---

## Combat State

### Engagement Triggers

Entity enters "in combat" state when:
* Deals damage to another entity
* Takes damage from another entity
* Is within aggro radius of hostile entity
* Uses offensive ability (even if it misses)

### Combat State Effects

While "in combat":
* Cannot mount/fast travel
* Stamina/mana regen rates apply
* Combat music plays
* Cannot interact with friendly NPCs/vendors
* UI shows combat elements (reaction queue, resource bars)

### Leaving Combat

Combat state ends when:
* No hostile entities within 20 hex radius
* 5 seconds have passed since last damage dealt/taken
* Entity dies

---

## Enemy AI Integration

### Basic Attack Pattern

**Simple Melee Enemy (Wild Dog):**
1. Detect player within aggro radius (10 hexes)
2. Pathfind to adjacent hex
3. When adjacent, attack every 2-3 seconds
4. Attack enters player's reaction queue
5. If player moves away, pursue
6. Leash if player exceeds 30 hex distance

**Ranged Enemy (Forest Sprite):**
1. Detect player within aggro radius (15 hexes)
2. Maintain distance of 5-8 hexes (kite if player approaches)
3. Attack every 3-4 seconds (projectile with travel time)
4. If player closes within 3 hexes, disengage (move away)

### Telegraph System

Enemies broadcast intent before major attacks:

**Visual Telegraph:**
* Ground indicator on target hexes (red outline or fill)
* Enemy wind-up animation (arm raise, charging effect)
* Duration: 0.5-2.0 seconds depending on attack power

**Purpose:**
* Gives player time to reposition (move off targeted hex)
* Creates skill expression (reading patterns, baiting attacks)
* Distinguishes dangerous abilities from basic attacks

---

## Mutual Destruction

**Scenario:** Both combatants have lethal damage queued simultaneously.

**Outcome:** Both entities die.

**Design Intent:**
* Rewards aggressive play with inherent risk
* Creates dramatic "we both went for it" moments
* No arbitrary "tie-breaker" rules

**Example:**
1. Player A casts Fireball at Player B (lethal damage)
2. Player B casts Lightning at Player A (lethal damage)
3. Both projectiles travel and hit simultaneously
4. Both enter reaction queues
5. Neither player reacts (out of resources or committed to trade)
6. Both die

---

## MVP Scope (Phase 1)

**Goal:** Playable combat loop with one enemy type and minimal abilities.

### Player Abilities

**Offensive:**
* Basic Attack (instant, adjacent hex, costs 0)

**Defensive:**
* Dodge (clear entire queue, 30 stamina, 0.5s GCD)

### Enemy Type

**Wild Dog:**
* HP: 100
* Damage: 15 physical
* Attack speed: 2 seconds
* Behavior: Aggro at 10 hexes, melee pursuit, basic attack

### Systems Required

1. **Combat State Management:**
   - Enter/exit combat triggers
   - Combat UI activation

2. **Reaction Queue:**
   - Queue component (stores threats + timers)
   - UI rendering (circular icons with timers)
   - Queue insertion on incoming damage
   - Queue resolution (timer expiry, overflow, reaction ability)

3. **Attributes Integration:**
   - Instinct → reaction window duration
   - Focus → queue capacity
   - Vitality → stamina pool
   - Might → outgoing damage

4. **Resources:**
   - Stamina bar UI
   - Passive stamina regen
   - Resource cost on ability use

5. **Damage Application:**
   - Passive modifiers (armor from Vitality)
   - Health bar updates
   - Death state

6. **Enemy AI:**
   - Aggro detection
   - Basic melee pursuit
   - Attack cycle (every 2s)

### Success Criteria

* Player can engage Wild Dog
* Dog's attacks enter player's reaction queue with visible timer
* Player can Dodge to clear queue (stamina cost applied)
* If player doesn't react, damage applies with armor reduction
* Combat feels responsive and clear (no confusion about what's happening)
* Player can win or lose based on resource management and reactions

---

## Post-MVP Extensions

### Phase 2: Build Diversity
* Add 2-3 more reaction abilities (Counter, Parry, Ward)
* Add 2-3 offensive abilities (Charge, Fireball, Mark)
* Multiple enemy types (ranged, tank, fast)

### Phase 3: Tactical Depth
* Positional modifiers (flanking bonus, terrain advantage)
* Status effects (stun, root, DoT)
* Enemy telegraphs for major attacks

### Phase 4: Advanced Systems
* Full Triumvirate ability sets (2 Approach + 2 Resilience per build)
* Boss encounters (multi-phase, complex patterns)
* PvP combat (flagging system, duels)

---

## Open Questions

**UI/UX:**
* Where should reaction queue display on screen? (above character? bottom center?)
* What keybinds for reaction abilities? (Spacebar for Dodge? Q/E/R for others?)
* How to distinguish unavoidable attacks visually? (different color? sound?)

**Balance:**
* Are base resource pools (100 stamina/mana) correct?
* Is 0.5s GCD too punishing or too lenient?
* Should armor cap at 75% reduction or lower/higher?

**Scope:**
* Should MVP include health bars for enemies? (assumed yes)
* Should MVP include death animations/loot drops? (or just despawn?)
* Should stamina regen be visible (floating numbers) or just bar fill?

---

## Design Goals Achieved

* ✅ **Conscious but decisive** - Reaction windows give time to think, GCD demands commitment
* ✅ **No twitch mechanics** - Hex targeting and timed reactions, not pixel-perfect aiming
* ✅ **Build identity matters** - Instinct/Focus directly shape defensive playstyle
* ✅ **Resource management is tactical** - Stamina/mana costs create meaningful decisions
* ✅ **Mutual destruction possible** - Emergent drama from simultaneous lethal damage
* ✅ **Clear feedback** - Queue UI shows exactly what threats you're facing
* ✅ **Skill expression** - Mastery comes from reading fights and managing resources, not reflexes
