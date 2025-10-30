# Combat System

## Core Philosophy

**"Conscious but Decisive"** - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

**Design Pillars:**
* Directional combat (face your enemies, position matters, no cursor required)
* Hex-based resolution (abilities affect hexes, not pixel-perfect hitboxes)
* Reaction-based defense (incoming damage enters a queue with time to respond)
* Resource management over cooldown juggling (stamina/mana costs, minimal cooldowns)
* Build identity shapes playstyle (attributes determine reaction capacity and offensive power)

**Inspiration:** MOBA-style ability targeting meets Monster Hunter's deliberate combat pacing, adapted for hex grid MMO.

---

## Offensive Layer

### Movement and Heading

**Movement Controls:**
* **Arrow keys** to move between adjacent hexes
* **Left/Right:** East/West movement (absolute directions)
* **Up/Down:** Context-sensitive diagonal movement for pointy-top hex grid
  - Movement direction depends on your current axis
  - Enables full 6-directional hex movement with 4 arrow keys
* Movement automatically updates your heading (facing direction)
* Your position on the hex shifts to face the direction you're moving
* No "turn in place" command - heading only updates through movement
* No mouse required - fully keyboard controlled

**Heading Mechanics:**
* Heading persists after you stop moving (you continue facing that direction)
* **Facing cone: 60 degrees** (one hex-face direction in hexagonal grid)
* Heading determines:
  - Character rotation (visual facing)
  - Position on hex (tactical micro-positioning)
  - Target selection (what's "in front" of you within 60° cone)
  - Ability direction (projectiles, lines, cones)

**Visual Clarity:**
* Character sprite rotates to face heading
* Character positioned on hex to indicate facing (not centered if moving)
* Optional facing cone indicator (60° arc overlay) shows targeting area

### Targeting System

All combat is **directional** - you face a direction and abilities target based on your heading and proximity.

**Player Heading:**
* Players always face a direction after their first move
* Heading determines both rotation AND position on hex (tactical positioning)
* No manual "turn in place" - movement updates heading
* Your facing is always visually clear (character orientation, position on hex)

**Target Selection:**
* Your "target" is determined by direction + proximity
* Basic attacks hit the **nearest hostile within range in the direction you're facing**
* No clicking, no cursor - just face enemies and attack

**Target Indicators:**

Players see TWO target indicators:
* 🔴 **Hostile target** (red): One hostile based on current tier and facing direction
* 🟢 **Ally target** (green): Nearest ally in facing direction

Only ONE hostile is targeted at a time. Indicator shows which enemy/ally will be affected by abilities.

**Range Tiers:**

Targeting searches within range bands:
* **Tier 1 (Close)**: 1-2 hexes
* **Tier 2 (Mid)**: 3-6 hexes
* **Tier 3 (Far)**: 7+ hexes

**Tier Selection:**

*Automatic (Default):*
* Default tier = nearest hostile in facing direction (any range)
* Geometric tiebreaker: closest to exact facing angle (most "in front" of you)
* No button presses needed for standard flow

*Manual (Tier Lock):*
* Press number key to temporarily lock to a specific tier:
  - **1** = Search for targets in Close range (1-2 hexes)
  - **2** = Search for targets in Mid range (3-6 hexes)
  - **3** = Search for targets in Far range (7+ hexes)
* Target indicator switches to nearest hostile within that tier
* **If no targets in tier:** Lock remains active, continuously searching that range
  - Visual feedback: Tier range highlighted (shows empty search area)
  - When enemy enters tier range, they become target immediately
* **Tier lock drops after 1 ability use** → returns to default (nearest any range)
* Allows quick target switching without repositioning

*Example Flow:*
```
Scenario: Warrior fighting NPC dog (range 1), hostile player approaches (range 7)

1. Default: Red indicator on dog (nearest, range 1)
2. Press "3" (far tier lock) → indicator switches to player (nearest in far tier)
3. Press "Q" (Charge gap closer) → charges at player
4. Tier lock drops → indicator returns to nearest (now player at range 1)
5. Press "W" (Overpower) → hits player immediately
```

*Manual (TAB Cycle):*
* TAB cycles through all valid targets in current tier
* If multiple hostiles exist in same tier, TAB lets you choose which
* Manual TAB lock persists until:
  - You press ESC (clear override, return to default)
  - Selected target dies or becomes invalid
  - You move/rotate significantly (changes valid target list)
  - You change tier (press 1/2/3)

**Design Rationale:**
* Single indicator = clear, unambiguous targeting
* Tier lock solves "caster wants backline" problem
* Single-ability tier lock prevents "stuck" feeling
* TAB handles equidistant edge cases
* Geometric default rewards positioning
* Works without responsive movement (tier lock as fallback)

**Visual Feedback:**
* Default targeting: Red indicator on nearest hostile
* Tier lock active: Indicator shows tier number/icon (small "3" badge on far target)
* Tier lock with no targets: Tier range highlighted in facing cone (shows empty search area)
* TAB lock: Additional border/marker to show manual selection
* Target out of ability range: Indicator dims or shows range error on cast attempt
* Facing cone: Optional 60° arc overlay to show targeting area

**Ability Patterns:**
* **Single Target** - Hits the indicated target (nearest in direction + range)
* **Self Target** - Affects caster only (buffs, self-heals) - no targeting required, press key to cast
* **Line Pattern** - Affects N hexes in a line from you in your facing direction
* **Radius Pattern** - Affects all hexes within distance R from you
* **Adjacent** - Affects hexes directly adjacent to you (all 6 or frontal arc)
* **Point-Blank AOE (PBAoE)** - Affects area centered on you - no targeting required

**Hit Detection:** If an entity occupies the targeted hex when the ability resolves, they are hit.

### Attack Execution Patterns

**Instant Attacks:**
* Resolve immediately on cast
* Target based on facing direction + proximity at moment of cast
* Typically melee/adjacent hex abilities
* Example: Basic sword strike, Charge (Direct signature)

**Projectile Attacks:**
* Projectile spawns and travels toward target hex
* Targets the hostile/ally indicated at moment of cast
* Projectile travels in straight line toward target's location (snapshot)
* If target moves after cast, projectile continues to original hex (dodgeable!)
* Provides visual warning before impact (see it coming)
* Speed varies by ability (arrow fast, fireball slow)
* Hit detection: Damages entities at impact hex when projectile arrives
* Example: Volley (Distant signature), basic ranged attack
* **Player Interaction:** Face target, press ability → projectile fires at indicated target

**Ground Effects:**
* Telegraph appears on target hexes before damage resolves
* Target area determined by ability pattern + your heading:
  - Single target: Telegraphs at indicated target's hex
  - Radius: Telegraphs radius around target hex
  - Line: Telegraphs line of hexes in your facing direction
* Fixed delay before resolution (1-3 seconds)
* Entities can move off telegraphed hexes to avoid damage
* Damage applies to any entity occupying hex when telegraph expires
* Example: Eruption (radiates outward), Trap (Ambushing signature)
* **Player Interaction:** Face direction/target, press ability → telegraph appears → damage after delay

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

### Enemy Targeting

Enemies use simplified directional targeting:
* Face toward target player (heading updates on movement)
* Target nearest hostile player in facing direction (same as player targeting)
* No tier lock or TAB cycling (AI uses geometric default only)
* Abilities fire at indicated target based on enemy's heading

### Basic Attack Pattern

**Simple Melee Enemy (Wild Dog):**
1. Detect player within aggro radius (10 hexes)
2. Face toward player, pathfind to adjacent hex
3. When adjacent and facing player, attack every 2-3 seconds
4. Attack enters player's reaction queue
5. If player moves away, turn to face and pursue
6. Leash if player exceeds 30 hex distance

**AI Behavior:**
* Continuously updates facing to track player
* Must be facing player to attack (adds slight delay if player circles)
* Creates opportunity for player to use positioning (stay behind enemy)

**Ranged Enemy (Forest Sprite):**
1. Detect player within aggro radius (15 hexes)
2. Face toward player
3. Maintain distance of 5-8 hexes (kite if player approaches)
4. Attack every 3-4 seconds (projectile with travel time)
5. If player closes within 3 hexes, disengage (move away while maintaining facing)

**AI Behavior:**
* Kiting enemies back-pedal while maintaining facing (harder to flank)
* Projectiles snapshot player position (player can dodge by moving)

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
* **Basic Attack (Q key)**
  - Instant, no cost, no cooldown
  - Range: Adjacent hex (1 hex away, close tier)
  - Targeting: Nearest hostile in facing direction (60° cone) within range
  - Damage: 20 physical, scales with Might
  - Visual: Simple attack animation, swing weapon toward target
  - Audio: Weapon swoosh + impact sound
  - Player interaction: Face enemy with arrow keys, press Q to attack

**Defensive:**
* **Dodge (E key)**
  - Clear entire queue, 30 stamina, 0.5s GCD
  - Self-target ability (no targeting required)
  - No movement (advanced version later will have dash)
  - Visual: Blur/ghost effect (you evade but stay in place)
  - Audio: Whoosh sound
  - Player interaction: Press E when queue has threats to clear them

### Enemy Type

**Wild Dog:**
* HP: 100
* Damage: 15 physical
* Attack speed: 2 seconds
* Behavior: Aggro at 10 hexes, melee pursuit, basic attack

### Systems Required

1. **Movement and Heading:**
   - WASD movement between hexes
   - Heading tracking (persists after movement stops)
   - Character rotation to match heading
   - Position on hex indicates facing direction
   - Visual facing indicator (optional arrow/cone)

2. **Directional Targeting:**
   - Target indicator system (red hostile, green ally)
   - Geometric target selection (nearest in facing direction + angle tiebreaker)
   - Range tier system (close 1-2, mid 3-6, far 7+)
   - Tier lock with number keys (1/2/3, drops after 1 ability)
   - TAB cycling through valid targets
   - ESC to clear manual targeting
   - Target indicator visual feedback (tier badges, lock markers)

3. **Combat State Management:**
   - Enter/exit combat triggers
   - Combat UI activation

4. **Reaction Queue:**
   - Queue component (stores threats + timers)
   - UI rendering (circular icons with timers)
   - Queue insertion on incoming damage
   - Queue resolution (timer expiry, overflow, reaction ability)

5. **Attributes Integration:**
   - Instinct → reaction window duration
   - Focus → queue capacity
   - Vitality → stamina pool
   - Might → outgoing damage

6. **Resources:**
   - Stamina bar UI
   - Passive stamina regen
   - Resource cost on ability use

7. **Damage Application:**
   - Passive modifiers (armor from Vitality)
   - Health bar updates
   - Death state

8. **Enemy AI:**
   - Aggro detection
   - Directional targeting (face player)
   - Basic melee pursuit with facing
   - Attack cycle (every 2s)

### Success Criteria

* Player can move with WASD and heading updates correctly
* Target indicator shows nearest hostile in facing direction
* Player can face Wild Dog and see red indicator on it
* Player can Basic Attack and hit indicated target
* Dog's attacks enter player's reaction queue with visible timer
* Player can Dodge to clear queue (stamina cost applied)
* If player doesn't react, damage applies with armor reduction
* Positioning matters: player can reposition to change target
* Target indicator updates smoothly as player moves/rotates
* Combat feels responsive and clear (no confusion about what's happening)
* Player can win or lose based on resource management, reactions, AND positioning

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

## Controls and Keybindings

**Two-Handed Keyboard Layout (No Mouse Required):**

### Movement (Right Hand)
* **Arrow Keys**: Move between adjacent hexes, updates heading
* **←/→**: East/West movement (absolute directions)
* **↑/↓**: Context-sensitive diagonal movement (pointy-top hex grid)
  - Full 6-directional movement with 4 keys
  - Direction depends on your current movement axis
* Your character faces the direction you last moved

### Combat Abilities (Left Hand)
* **Q**: Ability slot 1 (Example: Basic Attack)
* **W**: Ability slot 2 (Example: Secondary ability)
* **E**: Ability slot 3 (Example: Dodge/Reaction ability)
* **R**: Ability slot 4 (Example: Ultimate/Special ability)
* Abilities target current hostile/ally indicator or self (depending on ability)

### Targeting (Left Hand)
* **Automatic:** Target indicator shows nearest hostile in facing direction (60° cone)
* **1**: Tier lock close range (1-2 hexes), persists until 1 ability used
* **2**: Tier lock mid range (3-6 hexes), persists until 1 ability used
* **3**: Tier lock far range (7+ hexes), persists until 1 ability used
* **TAB**: Cycle through valid targets in current tier (manual lock until ESC or target invalid)
* **ESC**: Clear manual targeting, return to automatic

### Visual Indicators
* **Red indicator:** Current hostile target (what you'll attack)
* **Green indicator:** Current ally target (for friendly abilities)
* **Tier badge:** Small number (1/2/3) shows tier lock active
* **Tier highlight:** Range visualization when tier lock has no valid targets
* **Lock marker:** Additional border shows TAB manual lock active

**Note:**
* Exact keybindings configurable, these are defaults
* Design specifically avoids mouse for accessibility and controller support
* Detailed movement mechanics for pointy-top hex grid can be documented separately if needed

---

## Open Questions

**UI/UX:**
* Where should reaction queue display on screen? (above character? bottom center?)
* How prominent should target indicators be? (subtle outline vs big icon vs ground marker?)
* Should target indicators include distance markers? (show hex count to target)
* How to show facing/heading clearly? (character rotation sufficient? add arrow/cone?)
* Should tier lock show range visualization? (highlight valid hexes in tier)
* How to distinguish between auto-target and manual-lock visually?
* Should target indicators be color-coded AND have symbols? (colorblind accessibility)
* How to distinguish unavoidable attacks visually? (different color? sound?)
* Should abilities show "out of range" warning before cast? (indicator dims if too far)

**Balance:**
* Are base resource pools (100 stamina/mana) correct?
* Is 0.5s GCD too punishing or too lenient?
* Should armor cap at 75% reduction or lower/higher?
* Are range tiers correct? (1-2 close, 3-6 mid, 7+ far)
* Should enemies get facing bonus/penalty? (backstab damage, frontal armor)

**Directional Combat:**
* ✅ **Facing cone: 60 degrees** (decided - one hex-face direction)
* Should moving backwards be slower than forward? (incentivize facing enemies) - Needs playtesting
* Should abilities have facing requirements? (some abilities only work if target is in front)
* How much does heading/position on hex matter geometrically? (for tiebreakers)
* Movement speed/responsiveness? - Needs playtesting to balance feel vs tactical play

**Scope:**
* Should MVP include health bars for enemies? (assumed yes)
* Should MVP include death animations/loot drops? (or just despawn?)
* Should stamina regen be visible (floating numbers) or just bar fill?
* Should MVP include target indicators? (yes - critical for directional combat)
* Should MVP include tier lock system? (or just auto-target for simplicity)

---

## Design Goals Achieved

* ✅ **Conscious but decisive** - Reaction windows give time to think, GCD demands commitment
* ✅ **No twitch mechanics** - Directional targeting and timed reactions, not pixel-perfect aiming
* ✅ **Positioning matters** - Facing, heading, and geometric targeting reward tactical positioning
* ✅ **Build identity matters** - Instinct/Focus directly shape defensive playstyle
* ✅ **Resource management is tactical** - Stamina/mana costs create meaningful decisions
* ✅ **Mutual destruction possible** - Emergent drama from simultaneous lethal damage
* ✅ **Clear feedback** - Queue UI and target indicators show exactly what's happening
* ✅ **Skill expression** - Mastery comes from reading fights, managing resources, and positioning
* ✅ **No cursor required** - Fully playable with keyboard, controller-friendly design
* ✅ **Emergent tactics** - Tier lock and geometric targeting create depth without complexity
