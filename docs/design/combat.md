# Combat System

## Core Philosophy

**"Conscious but Decisive"** - Real-time tactical combat where skill comes from positioning, reading threats, and resource management. No twitch mechanics required.

**Design Pillars:**
* Directional combat (face your enemies, position matters, no cursor required)
* Hex-based resolution (abilities affect hexes, not pixel-perfect hitboxes)
* Reaction-based defense (incoming damage enters a queue with time to respond)
* Tactical ability flow (universal lockout with variable duration + synergies reward smart sequencing, not memorized rotations)
* Resource management over cooldown juggling (stamina/mana costs are primary throttle)
* Build identity shapes playstyle (gear determines abilities, attributes determine power)

**Inspiration:** MOBA-style ability targeting meets Monster Hunter's deliberate combat pacing, adapted for hex grid MMO.

---

## Offensive Layer

### Movement and Heading

**Movement Controls:**
* **Arrow keys** to move between adjacent hexes
* **Left/Right:** East/West movement (absolute directions)
* **Up/Down:** Context-sensitive diagonal movement for flat-top hex grid
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
  - Target selection (what's "in front" of you within 60 degree cone)
  - Ability direction (projectiles, lines, cones)

**Visual Clarity:**
* Character sprite rotates to face heading
* Character positioned on hex to indicate facing (not centered if moving)
* Optional facing cone indicator (60 degree arc overlay) shows targeting area

### Targeting System

All combat is **directional** - you face a direction and abilities target based on your heading and proximity.

**Player Heading:** Determined by movement (see Movement and Heading section above).

**Target Selection:**
* Your "target" is determined by direction + proximity
* Attacks and abilities hit the **nearest hostile within range in the direction you're facing**
* No clicking, no cursor - just face enemies and use abilities
* Auto-attacks automatically target adjacent hostiles

**Target Indicators:**

Players see TWO target indicators:
* **Hostile target** (red): One hostile based on current tier and facing direction
* **Ally target** (green): Nearest ally in facing direction

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
* **Tier lock drops after 1 ability use** -> returns to default (nearest any range)
* Allows quick target switching without repositioning

*Example Flow:*
```
Scenario: Warrior fighting NPC dog (range 1), hostile player approaches (range 7)

1. Default: Red indicator on dog (nearest, range 1)
2. Press "3" (far tier lock) -> indicator switches to player (nearest in far tier)
3. Press "Q" (Lunge gap closer) -> dashes to player
4. Tier lock drops -> indicator returns to nearest (now player at range 1)
5. Press "W" (Overpower) -> hits player with heavy damage
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
* Facing cone: Optional 60 degree arc overlay to show targeting area

**Ability Patterns:**
* **Single Target** - Hits the indicated target (nearest in direction + range)
* **Self Target** - Affects caster only (buffs, self-heals) - no targeting required, press key to cast
* **Line Pattern** - Affects N hexes in a line from you in your facing direction
* **Radius Pattern** - Affects all hexes within distance R from you
* **Adjacent** - Affects hexes directly adjacent to you (all 6 or frontal arc)
* **Point-Blank AOE (PBAoE)** - Affects area centered on you - no targeting required

**Server Validation:** Server recalculates target independently using the identical algorithm (NNTree spatial query → 60° cone filter → nearest by hex distance → geometric tiebreaker). Client's target selection is never trusted — both sides must agree for ability execution to proceed.

**Hit Detection:** If an entity occupies the targeted hex when the ability resolves, they are hit.

### Attack Execution Patterns

**Instant Attacks:**
* Resolve immediately on cast
* Target based on facing direction + proximity at moment of cast
* Typically melee/adjacent hex abilities or gap closers
* Example: Lunge (Direct signature), Overpower (Overwhelming signature)

**Ranged Attacks:**
* Resolve instantly on cast (no projectile travel time)
* Damage applied immediately to target's reaction queue
* Targets the hostile/ally indicated at moment of cast
* **Attack telegraphs** provide visual feedback (not dodge warnings):
  - Yellow ball appears over attacker when ability activates
  - On hit: Line draws from attacker to target showing damage trajectory
  - Telegraphs show what happened, not what will happen (damage already queued)
* Cannot be dodged by movement (instant hit at cast moment)
* Skill expression comes from **reaction queue management** and positioning, not twitch dodging
* Example: Volley (Forest Sprite ranged attack)
* **Player Interaction:** Face target, press ability -> instant damage to target's queue + visual feedback

**Ground Effects:**
* Telegraph appears on target hexes before damage resolves
* Target area determined by ability pattern + your heading:
  - Single target: Telegraphs at indicated target's hex
  - Radius: Telegraphs radius around target hex
  - Line: Telegraphs line of hexes in your facing direction
* Fixed delay before resolution
* Entities can move off telegraphed hexes to avoid damage
* Damage applies to any entity occupying hex when telegraph expires
* Example: Eruption (radiates outward), Trap (Ambushing signature)
* **Player Interaction:** Face direction/target, press ability -> telegraph appears -> damage after delay

**Unavoidable Attacks:**
* Bypass reaction queue system entirely
* Apply damage immediately with passive modifiers only
* Rare, expensive, ultimate-tier abilities
* Distinct visual/audio cues (cannot be mistaken for normal attacks)
* Example: True Strike, Piercing Shot

**Attack Telegraphs (Visual Feedback System):**

Attack telegraphs provide **combat clarity** without requiring twitch-based dodging. They show what happened, not what will happen.

**Visual System:**
* **Yellow ball** appears over attacker when ranged ability activates
* **Line trajectory** draws from attacker to target on successful hit
* Telegraphs appear **after damage is queued** (not a dodge warning)
* Duration: Brief visual feedback, then fades

**Purpose:**
* Combat clarity - Players understand who attacked and from where
* Source identification - Track multiple ranged enemies in chaotic fights
* Damage attribution - Know which enemy to prioritize or flee from
* **Not a dodging mechanic** - By the time you see the telegraph, damage is already queued

**Differentiation from Ground Effects:**
* **Attack telegraphs** = Instant hit feedback (ranged attacks like Volley)
  - Damage already in your reaction queue
  - Shows combat history, not future threat
  - Cannot be avoided by movement
* **Ground effects** = Delayed AOE warnings (abilities like Eruption, Trap)
  - Telegraphs appear **before** damage resolves
  - Fixed delay before damage applies
  - **Can be dodged** by moving off telegraphed hexes
  - Intentional skill expression through positioning

**Design Rationale:**
* Instant hit combat eliminates bullet hell / twitch mechanics at scale
* Attack telegraphs preserve combat readability without requiring pixel-perfect reflexes
* Skill expression comes from reaction queue management and positioning, not projectile dodging
* Ground effects (delayed AOE) still provide positioning-based counterplay for appropriate abilities

---

## Defensive Layer: Reaction Queue System

### Core Mechanic

When damage would hit an entity, it enters their **reaction queue** before applying damage.

**Queue Properties:**
* Each queued threat has an independent timer (circular progress indicator)
* Queue is unbounded — all threats enter regardless
* **Visibility window** = f(Focus → Concentration commitment tier) determines how many threats the player can see and interact with
* Threats outside the visibility window still resolve on their timers but cannot be reacted to
* Timer duration = f(level gap × Cunning vs Finesse contest)
* Entity can use **reaction abilities** to clear visible threats before timers expire

### Queue Display

**Visual Representation:**
* Row of circular icons with depleting timers
* Left to right = order of resolution (soonest first)
* Each icon shows attack type/source (physical icon, fire icon, etc.)
* Timer ring depletes clockwise around icon

**Example:**
```
[sword -----] [fire ------] [sword --------]
  0.4s left    0.8s left      1.2s left
```

### Attribute Scaling

**Reaction Window (Cunning vs Finesse):**
* Base window set by level gap between defender and attacker (higher-level defenders get longer windows)
* Cunning (Instinct) vs Finesse (Grace) contest multiplier scales the window further (baseline+bonus pattern — never reduces below base)
* See [attributes.md](attributes.md) for contest pattern details

**Visibility Window (Concentration):**
* Focus → Concentration commitment tier determines how many threats the player can see and interact with
* Queue is unbounded — Concentration controls perception, not capacity

### Queue Resolution

**When timer expires (no reaction):**
* Damage applies with passive modifiers (armor, resistance, etc.)
* Threat removed from queue
* Next threat begins resolving

**When threats exceed visibility window:**
* Threats beyond the window still enter the queue and run their timers
* Player cannot see or react to threats outside the window
* Threats outside the window resolve with passive modifiers when their timers expire

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
* Cost: Stamina
* Effect: Evade all queued threats
* Visual: Character dash/blur effect
* Audio: Whoosh sound

**Ward (Shielded signature):**
* Cost: Mana
* Effect: Magic shield absorbs all queued magic damage
* Visual: Glowing barrier appears
* Audio: Crystalline chime

**Fortify (Hardened signature):**
* Cost: Stamina
* Effect: Reduce all queued physical damage, then apply
* Visual: Character braces, metallic sheen
* Audio: Metal clang

### Selective Clear Abilities

Clear **first N threats** in queue (leftmost).

**Counter (Patient signature):**
* Cost: Stamina
* Effect: Reflect all visible window threats back to their attackers
* Clears: All visible threats
* Reflected damage scales with Technique (Grace → absolute), with bonus from countered threat damage, capped
* Visual: Parry animation, attack bounces back
* Audio: Clashing metal

**Deflect (Hardened signature):**
* Cost: Stamina (MVP simplified version)
* Effect: Clear all queued threats
* Visual: Shield block animation
* Audio: Impact thud
* Note: Post-MVP will be selective (lower cost, 1 threat, physical only)

**Parry (Primal signature):**
* Cost: Stamina
* Effect: Negate first queued attack, brief stagger on attacker
* Clears: 1 threat (leftmost)
* Visual: Weapon parry, enemy recoils
* Audio: Sharp clang + grunt

### Modification Abilities

Do **not clear queue**, but modify outcome.

**Endure (Vital signature):**
* Cost: Stamina
* Effect: Increased stagger resist, damage still applies but no interrupt
* Clears: 0 threats
* Visual: Character glows with determination
* Audio: Deep breath

**Dispel (Mental signature):**
* Cost: Mana
* Effect: Remove all debuffs from self, does not affect damage queue
* Clears: 0 threats
* Visual: Shimmering aura cleanses status effects
* Audio: Purifying tone

### Ability Recovery System

**Universal lockout with variable duration replaces fixed global cooldown (GCD).** When you use an ability, ALL abilities lock for that ability's recovery duration. Each ability has a different lockout time, creating weighted commitment per choice.

**Recovery Lockout Mechanics:**
* Using ANY ability locks ALL abilities for that ability's recovery duration
* Lockout duration varies by ability (represents commitment weight)
* All ability icons grey out during lockout
* Recovery happens AFTER ability animation completes
* **Synergies allow specific abilities to unlock early** during lockout (see Tactical Synergies section)

**Recovery Duration by Commitment:**
* **Light Commitment**: Quick reactions (e.g., Knockback)
* **Medium Commitment**: Tactical positioning (e.g., Lunge, Deflect)
* **Heavy Commitment**: Powerful strikes (e.g., Overpower)

**Visual Feedback:**
* Circular timer fills around ALL ability icons during lockout
* Timer shows remaining lockout duration
* Icons grey out = locked
* **Gold glow = synergy available** (ability will unlock early)
* Gold glow appears IMMEDIATELY when triggering ability fires, not when window opens

**Architecture:** Single `GlobalRecovery` component per entity (remaining duration, total duration, triggered_by) replaces per-ability cooldown tracking. Synergies represented as `SynergyUnlock` components (ability key, unlock_at threshold, triggered_by). System execution order: ability execution → insert lockout → detect synergies → tick recovery timer → cleanup expired synergies → update UI.

**Design Intent:**
* Heavier abilities create longer lockouts (more commitment = more risk)
* Synergies break lockout for smart sequencing (reward tactical adaptation)
* No memorized rotations - multiple paths to early unlock
* Rhythmic pacing with tactical depth through synergy choices

---

### Tactical Synergies

**Certain ability sequences that make tactical sense allow early unlock during universal lockout.** Using one ability can create a "window of opportunity" where specific follow-up abilities become available before the full lockout expires.

**How Synergies Work:**

When you use an ability that sets up a tactical opportunity, synergizing abilities:
* **Unlock early** during the universal lockout (break the lockout for specific abilities)
* **Glow/highlight** immediately when trigger fires (visual "this will unlock early" indicator)
* **Stay highlighted** through early unlock until full recovery completes
* **Create urgency** - capitalize on the opening before other enemies react

**Example Synergies:**

**Gap Closer -> Strike:**
* Use Lunge (creates universal lockout)
* Overpower **glows immediately** (gold border appears right away)
* Overpower unlocks early (available during lockout)
* Tactical logic: You closed the gap, now capitalize while enemies are grouped

**Interrupt -> Exploit:**
* Use Knockback (creates universal lockout)
* Lunge **glows immediately**
* Lunge unlocks early (available during lockout)
* Tactical logic: You created an opening, close back in before they recover

**Heavy Strike -> Reposition:**
* Use Overpower (creates longer universal lockout)
* Knockback **glows immediately**
* Knockback unlocks early (available during lockout)
* Tactical logic: After committing to heavy strike, can escape early if needed

**Visual Feedback System:**

**When synergy activates:**
1. Synergizing ability icon **lights up with bright gold glow** (particle effects, gold border)
2. Glow appears IMMEDIATELY when triggering ability fires (not when window opens)
3. Ability unlocks early (turns green while others still grey)
4. Glow persists until full universal recovery completes (shows "special unlock")

**Audio feedback:**
* Satisfying "ding" or "whoosh" when synergy triggers
* Extra impact sound when using glowing ability
* Creates immediate, positive reinforcement

**Discovery Through Play:**

**No explicit combo tutorials required:**
* New players see glowing abilities and press them (feels good)
* Experimenting reveals which sequences create glows
* Natural learning: "Lunge makes Overpower glow - I should use them together"
* No wiki or guide needed to understand the system

**Multiple valid approaches:**
* Different situations call for different synergies
* Against ranged enemies: Gap closer synergies matter
* Against melee swarms: AoE -> crowd control synergies matter
* No "one true rotation" - adapt to the fight

**Chaining synergies:**
* Using a glowing ability may trigger new synergies
* Example: Lunge (glows Overpower) -> Overpower (glows Knockback) -> Knockback (glows Lunge)
* Resource costs prevent infinite loops (stamina depletes)
* Creates satisfying burst -> recovery -> burst rhythm

**Design Benefits:**

* **Rewards tactical thinking** - Right sequence for the situation, not memorized rotation
* **Feels decisive** - Once you spot the opportunity, execution flows smoothly
* **Self-teaching** - Glowing abilities guide learning without tutorials
* **Build diversity** - Different weapons/armor unlock different synergy patterns
* **Accessible depth** - Works without synergies (base recovery acceptable), better with them
* **Visible mastery** - Skilled players chain glowing abilities, creating flow state

---

## Resources

### Stamina

**Purpose:** Physical actions (dodges, blocks, physical abilities)

**Pool:** Base pool, currently flat (attribute-driven scaling TBD)

**Regeneration:** Passive regen in and out of combat

### Mana

**Purpose:** Magic actions (spells, wards, mental abilities)

**Pool:** Base pool, currently flat (attribute-driven scaling TBD)

**Regeneration:** Passive regen in and out of combat

### Health

**Purpose:** Survivability pool (reaching 0 = death)

**Pool Formula:**
```
max_health = ActorAttributes::max_health()
```
*(Calculated from vitality attribute - see [attributes.md](attributes.md))*

**Regeneration:**
- **Out of combat:** Flat rate regen (no attribute scaling)
- **In combat:** No regeneration
- **NPC leashing:** Rapid regen while returning to leash origin

**Design Rationale:**
- Out-of-combat regen creates retreat/recovery tactical gameplay
- No in-combat regen ensures fights have stakes (can't face-tank and outheal)
- Leash regen prevents chip-damage kiting exploits
- Flat rate keeps regeneration time predictable (no scaling complexity)

**Player Experience:**
- Take damage → retreat from combat → wait for combat to drop → health regenerates
- Creates "push deeper vs pull back" exploration decision-making

---

## Damage Calculation

### Outgoing Damage

**Offensive Abilities:** Scale with Force (Might → absolute). Each ability has its own multiplier against Force.

**Defensive/Reactive Abilities:** Scale with Technique (Grace → absolute). Counter reflects damage based on Technique with a bonus from the countered threat's damage, capped.

**Critical Hits:** Removed — damage is deterministic and contest-driven.

### Passive Modifiers (No Reaction)

When damage resolves without reaction ability, passive modifiers apply:

**Toughness vs Dominance (Contest Pattern):**
Passive mitigation uses the Toughness vs Dominance relative contest (nullifying pattern). Toughness provides flat damage reduction; Dominance suppresses healing. No separate physical/magic armor split — mitigation is unified through the contest system. See [attributes.md](attributes.md) for contest details.

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
* Stamina/mana regen rates apply (health does NOT regenerate)
* Combat music plays
* Cannot interact with friendly NPCs/vendors
* UI shows combat elements (reaction queue, resource bars)
* **Visual indicator: Red vignette** - Screen edges darken with red tint
  - Purpose: Clear visual feedback that health regeneration is paused
  - Fade-in transition when entering combat
  - Answers "Am I safe to heal?" at a glance

### Leaving Combat

Combat state ends when:
* No hostile entities within aggro radius
* Timeout since last damage dealt/taken
* Entity dies

**Post-Combat Effects:**
* Red vignette fades out
* Health regeneration begins
* Player can mount/interact with NPCs again
* Combat music fades to exploration theme

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
1. Detect player within aggro radius
2. Face toward player, pathfind to adjacent hex
3. When adjacent and facing player, attack periodically (cadence from Intensity tier)
4. Attack enters player's reaction queue
5. If player moves away, turn to face and pursue
6. Leash if player exceeds chase distance

**AI Behavior:**
* Continuously updates facing to track player
* Must be facing player to attack (adds slight delay if player circles)
* Creates opportunity for player to use positioning (stay behind enemy)

**Ranged Enemy (Forest Sprite):**
1. Detect player within aggro radius
2. Face toward player
3. Maintain preferred distance (kite if player approaches)
4. Attack periodically (instant hit ranged attack with visual telegraph)
5. If player closes too far inside preferred range, disengage (move away while maintaining facing)

**AI Behavior:**
* Kiting enemies back-pedal while maintaining facing (harder to flank)
* Instant hit mechanics - damage enters player's reaction queue immediately
* Attack telegraphs provide visual feedback (yellow ball -> hit line) for combat clarity
* Cannot be dodged by movement - player must use reaction abilities to defend

### Telegraph System

Enemies broadcast intent before major attacks:

**Visual Telegraph:**
* Ground indicator on target hexes (red outline or fill)
* Enemy wind-up animation (arm raise, charging effect)
* Duration: Varies by attack power

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
1. Player A casts ranged ability at Player B (lethal damage)
2. Player B casts ranged ability at Player A (lethal damage)
3. Both attacks hit instantly, damage enters reaction queues simultaneously
4. Attack telegraphs show both hits (yellow ball -> line feedback)
5. Neither player reacts (out of resources or committed to trade)
6. Both queue timers expire, both die

---

## Player Combat Build System

### Core Philosophy

Your build is defined by **3 systems working together**: Weapons (offense), Armor (defense), and Attributes (power/scaling).

**Key Principle:** Gear determines WHICH skills you have access to. Attributes determine HOW POWERFUL those skills are.

---

### Weapons = Approach Skills (Offense)

**2 Weapon Slots:** Main Hand + Off Hand

**Main Hand Options (2 approaches each):**
* **Mace:** Direct + Binding
* **Sword:** Direct + Evasive
* **Whip:** Distant + Binding
* **Revolver:** Distant + Evasive

**Off Hand Options (1 approach each):**
* **Shield:** Patient
* **Dagger:** Ambushing

**Total Combinations:** 4 main weapons x 2 off-hands = 8 unique loadouts covering all 8 approach combinations

**Skills Available:** 6 approach skills total (4 from main hand's 2 approaches + 2 from off-hand's 1 approach)

**Design Intent:**
* Changing weapons = different offensive toolkit with unique 3-approach combinations
* No skill explosion - fixed 6 approach skills per loadout
* Horizontal progression - collect weapons for different situations

---

### Armor = Resilience Skills (Defense)

**3 Armor Slots:** Binary choices between opposing pairs

**Helm: Mental OR Primal**
* **Mental** -> Focus, Dispel (clarity-based defense)
* **Primal** -> Enrage, Attune (instinct-based defense)

**Armor (chest): Shielded OR Hardened**
* **Shielded** -> Ward, Repel (magic barrier defense)
* **Hardened** -> Fortify, Deflect (physical armor defense)

**Accessory: Blessed OR Vital**
* **Blessed** -> Heal, Cleanse (restorative magic)
* **Vital** -> Regenerate, Endure (physical resilience)

**Total Combinations:** 2^3 = 8 defensive loadouts

**Skills Available:** 6 resilience skills (2 per armor slot)

**Design Intent:** Binary choices create meaningful tradeoffs with clear defensive identity (magic vs physical vs hybrid). Swap armor to counter different threats.

---

### Attributes = Power Scaling

**Fluid spectrum/axis sliders** (not locked by gear). Each attribute feeds into three scaling modes — see [attributes.md](attributes.md) for details:

* **Might** — Force (offensive potency), Impact (recovery pushback), Ferocity (commitment)
* **Grace** — Technique (defensive potency), Finesse (synergy compression), Poise (commitment)
* **Vitality** — Constitution (HP), Toughness (mitigation), Grit (commitment)
* **Focus** — Discipline (endurance), Composure (recovery reduction), Concentration (queue visibility)
* **Instinct** — Intuition (open), Cunning (reaction window), Flow (threat compression)
* **Presence** — Gravitas (open), Dominance (healing reduction), Intensity (AoE projection)

**Attributes scale your skills but don't gate access.** Same gear with different attributes creates different playstyles. Respec-friendly system encourages experimentation.

---

### Complete Build Example

**Gear Loadout:**
* **Weapons:** Sword (Direct/Evasive) + Dagger (Ambushing)
* **Armor:** Mental Helm + Hardened Chest + Vital Accessory

**Available Skills (12 total):**
* **Approach (6):** 2 Direct skills, 2 Evasive skills, 2 Ambushing skills
* **Resilience (6):** Focus, Dispel (Mental), Fortify, Deflect (Hardened), Regenerate, Endure (Vital)

**Slotted Abilities (4 at launch):**
* **Q:** Charge (Direct approach skill)
* **W:** Ambush (Ambushing approach skill)
* **E:** Fortify (Hardened resilience skill)
* **R:** Regenerate (Vital resilience skill)

**Attribute Spread:**
* High Vitality + Moderate Instinct = Durable with good reaction windows

**Build Identity:** Mobile melee assassin with physical resilience and self-healing

---

### Design Benefits

* Gear determines skills -> Clear build identity and counter-building opportunities
* Fixed skill pools -> No overwhelming choice paralysis
* 8 weapon + 8 armor combinations -> Complete coverage without gaps
* Horizontal progression -> Collect situational loadouts, not power tiers
* Manageable MVP scope -> 4 main weapons + 2 off-hands + 6 armor types

---

## MVP Scope (Phase 1)

**Goal:** Playable combat loop with one enemy type and minimal abilities. Demonstrate the gear-based skill system.

### Player Starting Loadout

**Design Note:** MVP focuses on one complete build to validate the gear-skill system. All abilities cost stamina only (no mana). This simplified starting loadout demonstrates how weapons and armor determine skill access.

**Starting Gear:**
* **Main Hand:** Sword (Direct + Evasive approaches)
* **Off Hand:** Shield (Patient approach)
* **Helm:** Mental (Focus, Dispel skills)
* **Armor:** Hardened (Fortify, Deflect skills)
* **Accessory:** Vital (Regenerate, Endure skills)

**Available Skills from Gear:**
* **Approach Skills:** Direct (x2), Evasive (x2), Patient (x2) = 6 available
* **Resilience Skills:** Focus, Dispel, Fortify, Deflect, Regenerate, Endure = 6 available

**MVP Slotted Abilities (4 total):**

**Auto-Attack (Passive):**
* **Auto-Attack**
  - Automatic, no cost, no cooldown
  - Range: Adjacent hex (1 hex away)
  - Targeting: Nearest hostile in facing direction (60 degree cone) within range
  - Attack speed: Driven by Intensity commitment tier (Presence)
  - Damage: Fraction of Force (scales with Might + level)
  - Pauses when not adjacent to target
  - Visual: Character automatically swings weapon at adjacent target
  - Audio: Weapon swoosh + impact sound
  - Player interaction: Passive - triggers automatically when adjacent to hostile target

**Slotted Approach Skills:**
* **Lunge (Q key)** - Direct approach skill (from Sword main hand)
  - Instant gap closer with damage
  - Range: Mid tier
  - Cost: Stamina
  - Cooldown: None (universal lockout only)
  - Targeting: Nearest hostile in facing direction (60 degree cone) within range
  - Effect: Instantly teleport adjacent to target, deal Force-scaled physical damage
  - Damage scales with Force (Might → absolute)
  - Visual: Quick dash to target, attack animation on arrival
  - Audio: Dash sound + impact
  - Player interaction: Face enemy, press Q to close distance and attack

* **Counter (W key)** - Patient approach skill (from Shield off-hand)
  - Defensive reaction skill
  - Range: Self-target
  - Cost: Stamina
  - Cooldown: Universal lockout
  - Targeting: Self (clears all visible window threats)
  - Effect: Reflect all visible threats back to attackers
  - Reflected damage scales with Technique (Grace → absolute), with bonus from countered threat damage, capped
  - Visual: Parry animation, attack bounces back
  - Audio: Clashing metal
  - Player interaction: Press W when threats in queue to reflect damage back

**Slotted Resilience Skills:**
* **Fortify (E key)** - Hardened armor skill
  - Full queue clear with damage mitigation
  - Range: Self-target
  - Cost: Stamina
  - Cooldown: Universal lockout
  - Effect: Reduce all queued physical damage, then apply
  - Visual: Character braces, metallic sheen
  - Audio: Metal clang
  - Player interaction: Press E when multiple threats in queue to tank damage

* **Deflect (R key)** - Hardened armor skill
  - Full queue clear (MVP simplified version)
  - Range: Self-target
  - Cost: Stamina
  - Cooldown: Universal lockout
  - Effect: Clear all queued threats completely
  - Visual: Shield block animation
  - Audio: Impact thud
  - Player interaction: Press R when multiple threats queued to clear entire queue
  - Note: Post-MVP will be selective (lower cost, 1 threat, physical only)

### Enemy Type

**Wild Dog:**
* Basic melee NPC with low HP
* Physical damage, attack speed from Intensity tier
* Behavior: Aggro within range, melee pursuit with TargetLock (ADR-012), basic attack

### Systems Required

1. **Movement and Heading:**
   - Arrow key movement between hexes
   - Heading tracking (persists after movement stops)
   - Character rotation to match heading
   - Position on hex indicates facing direction
   - Visual facing indicator (optional arrow/cone)

2. **Directional Targeting:** Target indicator system with geometric selection, tier lock (1/2/3 keys), and TAB cycling (see Targeting System section)

3. **Combat State Management:**
   - Enter/exit combat triggers
   - Combat UI activation

4. **Reaction Queue:**
   - Queue component (stores threats + timers)
   - UI rendering (circular icons with timers)
   - Queue insertion on incoming damage
   - Queue resolution (timer expiry, overflow, reaction ability)

5. **Attributes Integration:**
   - Cunning (Instinct) vs Finesse (Grace) → reaction window duration (contest)
   - Focus → Concentration → queue visibility window (commitment tier)
   - Vitality → Constitution → HP (absolute)
   - Might → Force → offensive damage (Lunge, Overpower, AutoAttack)
   - Grace → Technique → defensive damage (Counter reflected damage)
   - Presence → Intensity → auto-attack cadence (commitment tier)

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
   - Attack cycle (cadence from Intensity tier)

### Success Criteria

**Core Mechanics:**
* Movement with arrow keys updates heading correctly
* Target indicator shows nearest hostile in facing direction
* Auto-Attack activates automatically when adjacent to target
* Dog's attacks enter player's reaction queue with visible timer
* Damage applies with armor reduction if player doesn't react

**Abilities Work:**
* Lunge (Q) - Gap closer with damage (Direct skill from Sword)
* Counter (W) - Reflect queued damage (Patient skill from Shield)
* Fortify (E) - Reduce queued damage (Hardened armor skill)
* Deflect (R) - Clear all queued threats (Hardened armor skill)

**Skill Expression:**
* Gear determines available skills (Sword+Shield warrior with physical defense)
* Resource management matters (stamina balances offense and defense)
* Positioning matters (melee range provides DPS but exposes to danger)
* Combat feels responsive and clear (no confusion about state)

---

## Post-MVP Extensions

### Phase 2: Complete Gear System
* **Add remaining weapons:**
  - Main Hand: Mace, Whip, Revolver (3 more weapons)
  - Off Hand: Dagger (1 more weapon)
* **Add remaining armor:**
  - Helm: Primal option (Enrage, Attune skills)
  - Armor: Shielded option (Ward, Repel skills)
  - Accessory: Blessed option (Heal, Cleanse skills)
* **Gear acquisition:**
  - Loot drops from enemies
  - Crafting system (basic)
  - Vendor purchases
* **Gear swapping UI:**
  - Inventory screen showing equipped gear
  - Ability bar updates when gear changes
  - Visual feedback showing which skills come from which gear

### Phase 3: Build Depth and Variety
* **Full approach skill sets:**
  - Define 2 skills per approach (Direct, Evasive, Binding, Distant, Patient, Ambushing)
  - Total: 12 approach skills across 6 approaches
* **Full resilience skill sets:**
  - Define 2 skills per armor archetype (Mental, Primal, Shielded, Hardened, Blessed, Vital)
  - Total: 12 resilience skills across 6 archetypes
* **Ability slotting system:**
  - Choose which 4 abilities to slot from available pool (based on equipped gear)
  - Save/load ability bar configurations per gear set
* **Magic damage and mana:**
  - Add magic-based weapons (Revolver uses mana, Whip uses stamina)
  - Magic-based armor skills (Ward, Dispel, Heal use mana)
  - Hybrid builds (stamina + mana management)
* **Multiple enemy types:**
  - Ranged enemies (test Distant approach effectiveness)
  - Tank enemies (test armor-piercing mechanics)
  - Fast enemies (test reaction timing)
  - Magic enemies (test Ward/Dispel armor skills)

### Phase 4: Tactical Depth
* **Positional modifiers:**
  - Flanking bonus (attacking from behind)
  - Terrain advantage (high ground, cover)
* **Status effects:**
  - Stun, root, DoT (damage over time)
  - Buffs/debuffs from abilities
* **Enemy telegraphs:**
  - Major attacks with ground indicators
  - Pattern recognition (boss mechanics)
* **Boss encounters:**
  - Multi-phase fights
  - Gear-check mechanics (requires specific defensive skills)
  - Pattern-based challenges

### Phase 5: Player Progression
* **Horizontal gear progression:**
  - Collect multiple gear sets for different situations
  - Sidegrade weapons/armor (same tier, different approaches)
  - Situational loadouts (anti-magic set, anti-physical set, AoE set, single-target set)
* **Attribute respeccing:**
  - Allow players to respec attributes freely or with low cost
  - Experiment with different attribute spreads on same gear
* **Gear enhancement:**
  - Upgrade gear quality (better base stats, not new skills)
  - Enchantments (modify skill effectiveness)
  - Visual customization (skins, dyes)
* **PvP combat:**
  - Flagging system for opt-in PvP
  - Arena/duel systems
  - Gear-based counter-building (swap to anti-player loadout)

---

## Controls and Keybindings

**Two-Handed Keyboard Layout (No Mouse Required):**

### Movement (Right Hand)
* **Arrow Keys**: Move between adjacent hexes, updates heading
* **Left/Right**: East/West movement (absolute directions)
* **Up/Down**: Context-sensitive diagonal movement (flat-top hex grid)
  - Full 6-directional movement with 4 keys
* Your character faces the direction you last moved

### Combat Abilities (Left Hand)
* **Q**: Lunge - Gap closer + damage (Direct skill)
* **W**: Counter - Reflect visible window threats (Patient skill, self-target)
* **E**: Fortify - Reduce all queued damage (Hardened skill, self-target)
* **R**: Deflect - Clear all queued threats (Hardened skill, self-target)
* **Auto-Attack**: Passive - Automatically attacks adjacent hostile (cadence from Intensity tier)
* Abilities target current hostile/ally indicator or self (depending on ability type)

### Targeting (Left Hand)
* **Automatic:** Target indicator shows nearest hostile in facing direction (60 degree cone)
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
* Detailed movement mechanics for flat-top hex grid can be documented separately if needed

---

## Combat UI

**Goal:** "Always know who you're fighting, what's coming at you, and what you can do about it." Every element communicates its purpose through visual design alone.

**Three Visual Layers:**
* **World-Space UI** — Health bars above entities, target indicators on ground. Parented to entity Transform, follows entity movement.
* **Screen-Space Overlay** — Resource bars, action bar, reaction queue, target frame. Fixed position, never occluded.
* **Floating UI** — Damage numbers, ephemeral status text. World-space but short-lived.

**Client Prediction Strategy:** Local player UI uses predicted values (`step`) for instant feedback. Remote entity UI uses server-confirmed values (`state`). Reaction queue: client maintains state locally (synced via threat insertion events), runs timer countdown independently; server authoritative for insertion/removal.

### Visual Language

**Shape = Identity.** Circles and rectangles are never confused:
* **Circles = Threats** (reaction needed, time pressure, incoming danger)
* **Rectangles = Abilities** (player actions, deliberate choices)

### HUD Layout (In Combat)

```
+---------------------------------------------------------------------------+
|  IN COMBAT    [Time/Date]              +------------------+ [FPS/Debug]   | Top Bar
|                                        | Wild Dog      3h |               |
|              REACTION QUEUE            | ----------80/100 | Target        | Top Center
|          (sword) (fire) ( )            |                  | Frame         |
|          0.8s   0.4s    -             | stam ---- 45/80  | (Top-Right)   |
|                                        | mana -- 20/60    |               |
|         +---+ Enemy (orange bar)       |                  |               | Combat Area
|         |   |  <- Target               | (sword) ( ) ( ) |               |
|         +---+  Indicator               |  0.6s           |               |
|                                        +------------------+               |
|                     Player                                                |
|                    (arrow)   <- Facing indicator                          |
|                  [buffs]                                                   |
|                                                                           |
|         Enemy (faint bar)   Enemy (faint bar)                            |
|                                                                           |
|         [Stamina ------] [Health ------] [Mana ------]                   | Bottom
|                                                                           |
|           [Q]     [W]     [E]     [R]                                    | Action Bar
|          sword   fire    spin    EMPTY                                    |
+---------------------------------------------------------------------------+
```

### Key Elements

**Reaction Queue (Top-Center):**
* Circular threat indicators with dark red/black background
* Each threat shows: attack type icon (center), timer ring (clockwise depletion), optional countdown text
* Glowing orange/red borders when active; pulsing animation in final 20% of timer
* Empty slots shown as faint gray outlines
* Threats beyond visibility window are hidden (player cannot react to them)

**Action Bar (Bottom-Center, below resource bars):**
* Rectangular ability slots for Q/W/E/R
* Each slot shows: keybind label, ability icon, resource cost badge, cooldown overlay
* State indicators: ready (full color), on cooldown (gray sweep), insufficient resources (red outline + dim), out of range (orange outline + dim)

**Enemy Health Bars (World Space):**
* Thin horizontal bars above each enemy entity
* Orange/yellow fill, black background, entity name above
* Current target gets thicker/brighter bar; other enemies at reduced opacity

**Target Detail Frame (Top-Right):**
* Appears automatically when any target is selected (no lock required)
* Shows: entity name + hex distance, HP bar with exact numbers, resource pools (if applicable), their threat queue (if applicable)
* Scales by enemy type: basic enemies show name/HP only; smart enemies and players show full details

**Facing Indicator:**
* Directional arrow on character sprite (always visible)
* Character offset toward facing direction on hex
* Optional 60-degree cone overlay (toggleable)

**Range Feedback:**
* Ability icons dim/gray when target is out of range
* Distance counter near target indicator (hex count)

**Combat State Indicator:**
* Crossed swords icon (top-left) when in combat
* Red vignette around screen edges
* "Enemy disengaged" text when leashing

### Implementation Priority

**Phase 1 (Blocks MVP):** Reaction queue visual redesign, action bar, enemy health bars, target detail frame
**Phase 2 (Reduces Frustration):** Facing indicator, range feedback, tier lock indicators, combat state indicator
**Phase 3 (Polish):** Ally target indicator, damage numbers, manual lock feedback, status effects
**Phase 4 (Quality of Life):** Ability tooltips, colorblind modes, UI scale options

---

## Open Questions

**Gear System:**
* **How does gear acquisition work in MVP?** (start with fixed loadout? loot? vendor?)
* **Can players swap gear mid-combat?** (should there be combat lockout or cooldown?)
* **How is equipped gear displayed?** (paper doll UI? character model shows visual changes?)
* **How many skills per approach/archetype?** (2 per approach = 12 total approach skills?)
* **Do weapons have stat differences beyond skills?** (damage ranges? attack speed? or just skill access?)
* **How does ability slotting work?** (drag-drop UI? numbered slots? saved loadouts?)
* **Should gear have level/tier requirements?** (or purely horizontal sidegrades?)
* **How do players know which skills come from which gear?** (tooltips? color-coding? gear icons on ability bar?)

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
* **Gear UI:** How to show equipped gear clearly? (always-visible icons? character sheet?)
* **Skill source clarity:** Should ability tooltips show "From: Sword (Direct)" to indicate source?

**Balance:**
* Are range tiers correct? (1-2 close, 3-6 mid, 7+ far)
* Should enemies get facing bonus/penalty? (backstab damage, frontal armor)
* All numeric tuning (resource pools, ability costs, recovery timers, synergy strength) is playtesting-driven

**Tactical Synergies:**
* **Which abilities should synergize?** (need to define synergy pairs/chains per weapon combo)
* **Synergy tagging system:** How are synergies defined? (ability tags like "gap_closer", "aoe"? explicit pairs?)
* **Synergy discovery pacing:** Should early game have fewer synergies to avoid overwhelming new players?
* **Multiple synergy sources:** If two abilities both trigger synergy on same follow-up, do both glow?
* **Synergy feedback intensity:** How bright/obvious should glow be? (particle effects? border only? animation?)
* **Synergy audio:** What sound plays when synergy triggers? (ding? whoosh? ability-specific?)
* **Synergy chains depth:** How many abilities can chain before resources run out? (intended burst length?)
* **Weapon-specific synergies:** Does each weapon combo have unique synergy patterns?
* **Build diversity:** Do different Triumvirate approaches create different synergy opportunities?
* **Enemy AI synergies:** Should enemies also have ability synergies, or player-only mechanic?

**Directional Combat:**
* **Facing cone: 60 degrees** (decided - one hex-face direction)
* Should moving backwards be slower than forward? (incentivize facing enemies) - Needs playtesting
* Should abilities have facing requirements? (some abilities only work if target is in front)
* How much does heading/position on hex matter geometrically? (for tiebreakers)
* Movement speed/responsiveness? - Needs playtesting to balance feel vs tactical play
* **Auto-attack pause:** Should auto-attack pause while moving? Or only when not adjacent?
* **Auto-attack windup:** Should there be animation lock/windup time to prevent kiting abuse?

**Scope:**
* Should MVP include health bars for enemies? (assumed yes)
* Should MVP include death animations/loot drops? (or just despawn?)
* Should stamina regen be visible (floating numbers) or just bar fill?
* Should MVP include target indicators? (yes - critical for directional combat)
* Should MVP include tier lock system? (or just auto-target for simplicity)
* **Gear swapping in MVP?** (should players be able to change gear, or fixed loadout only?)

---

## Design Goals Achieved

* **Conscious but decisive** - Reaction windows give time to think, recovery timers create natural commitment without artificial delays
* **Tactical synergies reward adaptation** - Ability sequences that make tactical sense flow smoothly with glowing visual feedback, no forced rotations
* **No twitch mechanics** - Directional targeting and timed reactions, not pixel-perfect aiming
* **Positioning matters** - Facing, heading, and geometric targeting reward tactical positioning
* **Build identity matters** - Gear determines skills, attributes shape effectiveness
* **Resource management is tactical** - Stamina/mana costs create meaningful decisions
* **Mutual destruction possible** - Emergent drama from simultaneous lethal damage
* **Clear feedback** - Queue UI, target indicators, and synergy glows show exactly what's happening
* **Skill expression** - Mastery comes from reading fights, managing resources, positioning, and chaining tactical sequences
* **No cursor required** - Fully playable with keyboard, controller-friendly design
* **Emergent tactics** - Tier lock and geometric targeting create depth without complexity
* **Gear-driven builds** - Equipment choices fundamentally change playstyle and available tactics
* **Horizontal progression** - Collect situational gear sets rather than vertical power tiers
* **Counter-building** - Swap gear to adapt to different threats (magic vs physical, ranged vs melee)
* **Manageable complexity** - Fixed skill pools per gear piece prevent overwhelming choice paralysis
* **Thematic coherence** - Weapon/armor archetypes create clear fantasy (sword+shield warrior, whip+dagger assassin)

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Reaction queue position | Top-center | Center-screen with vertical offset | Better visibility, closer to player focus |
| 2 | Deflect scope | Clears first queued threat | Clears all queued threats | Simplified MVP defensive option |
| 3 | World health bars | Per-entity spawn/despawn | 2 persistent bars repositioned to targets | More performant, less clutter |
| 4 | Range feedback | Error on cast attempt | Proactive red border on action bar | Prevents failed ability attempts |
| 5 | Knockback | Positioning tool (push only) | Reactive: targets threat source, removes threat, pushes | Deeper skill expression + queue interaction |
| 6 | Overpower cooldown | Per-ability cooldown | Universal lockout (standard) | GCD prevents spam; separate cooldown adds complexity |
| 7 | Tier badge UI | Visual badge on target indicator | Deferred (core tier lock works) | Bevy 3D text complexity, not MVP-blocking |
| 8 | Visual ring indicator | Not in spec | Ring around player shows targeting area | Critical UX for spatial tier lock |
| 9 | Empty tier visualization | Facing cone range highlighting | Deferred (filtering works) | Visual overlay not critical for functionality |
| 10 | MVP ability set | Counter (W) + Fortify (E) | Overpower (W) + Knockback (E) | ADR-009 predates updated spec; functional but doesn't demonstrate gear-skill system |
| 11 | Hex orientation | Pointy-top hex grid | FlatTop hex grid | Changed during development; flat edge north, vertices east/west |
| 12 | Passive mitigation | Separate armor (physical) and resistance (magic) percentages | Unified Toughness vs Dominance contest pattern | ADR-031; contest-driven, no type split |
| 13 | Auto-attack speed | Fixed interval | Cadence from Intensity commitment tier (Presence) | Legacy cadence still active; Intensity intended for AoE projection |
| 14 | Counter scope | Reflect first queued threat (1) | Reflects all visible window threats | More impactful defensive option |

## Implementation Gaps

**Critical:** Unified tier lock tutorial (MANDATORY before support abilities launch)

**High:** TAB cycling, Counter ability, Fortify ability, gear-based skill gating

**Medium:** Tier badge UI, empty tier visualization, visual telegraphs, combat music

**Post-MVP:** Full weapon/armor system (8 weapons + 8 armor pieces), ability slotting (choose 4 from 12), full reaction ability set (7 additional), ground effects, stagger system

---

## Related Design Documents

* [attributes.md](attributes.md) - Attribute system (bipolar pairs, scaling modes, derived stats)
* [combat-balance.md](combat-balance.md) - Combat balance tuning and number crunching
* [triumvirate.md](triumvirate.md) - Origin/Approach/Resilience classification system
