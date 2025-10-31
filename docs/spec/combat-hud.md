# Combat HUD Specification

## Player Experience Goal

**"Always know who you're fighting, what's coming at you, and what you can do about it"**

Players must understand combat state at a glance with zero ambiguity. Every UI element should communicate its purpose through visual design alone - no manual reading required.

---

## Design Principles

1. **Clarity Over Aesthetics** - If a choice makes information clearer but less pretty, choose clarity
2. **Visual Hierarchy** - Most urgent information (incoming threats) gets most prominent placement
3. **Distinct Visual Languages** - Different systems (threats vs abilities vs resources) should never be confused
4. **Accessible** - Colorblind-safe, readable at all resolutions, no reliance on color alone
5. **Combat-Focused** - Show only combat-critical info during fights, hide noise

---

## Current Implementation (Baseline)

### Working Elements

**1. Target Indicator System ✅**
- Red circle on current hostile target
- Clearly visible, unambiguous
- Ground-based or sprite-attached (needs documentation)
- **Player Feedback:** Works well, keep this approach

**2. Resource Pools (Bottom Center) ✅**
- Three horizontal bars: Yellow (Stamina), Red (Health), Blue (Mana)
- Shows current/max values with color fill
- Damage state visible (black/empty portion = lost HP)
- High contrast, easy to read
- **Player Feedback:** Size and prominence feel right

**3. Reaction Queue (Top Center) ✅ - Needs Enhancement**
- Three boxes representing threat queue slots
- Orange border = active threat
- Empty/gray = available queue capacity
- **Player Feedback:** Exists but non-obvious. Needs visual redesign to communicate "incoming danger"

---

## Critical Enhancements

### 1. Reaction Queue Visual Redesign ⚠️ HIGH PRIORITY

**Current Problem:** Boxes look generic, could be mistaken for ability slots or inventory.

**Goal:** Instantly recognizable as "incoming threats I need to react to"

#### Visual Identity Changes

**Shape & Style:**
- **Circular threat indicators** instead of squares (distinguishes from action bar)
- **Darker background** - Black or very dark red (danger theme)
- **Glowing borders** when active (orange/red glow = urgent)
- **Pulsing animation** as timer approaches expiration (final 20% of timer)

**Threat Information Display:**

Each active threat shows:
1. **Attack Type Icon** - Center of circle (⚔️ physical, 🔥 fire, ⚡ magic, 🟢 poison, etc.)
2. **Timer Ring** - Circular progress indicator depleting clockwise around border
3. **Countdown Text** - Optional numeric timer (0.8s, 0.4s) below/inside circle for precision

**Empty Slots:**
- Faint gray outline (barely visible)
- No glow, no animation
- Shows queue capacity at a glance

#### Visual Example (ASCII Approximation)

```
        INCOMING THREATS
    ╔═══════╗  ╔═══════╗  ┌───────┐
    ║   ⚔️   ║  ║   🔥   ║  │       │  ← Circular design
    ║ ●●●○○ ║  ║ ●●○○○ ║  │  ---  │  ← Timer rings
    ║  0.8s ║  ║  0.4s ║  │       │  ← Countdown text
    ╚═══════╝  ╚═══════╝  └───────┘
    (glowing)  (pulsing!)  (empty)
```

#### Positioning & Layout

**Current:** Top-center of screen
**Recommendation:** Keep top-center, but adjust:
- Slightly lower (more above player, less at screen edge)
- Optional: Add subtle connecting lines from threats to player character (shows "these are targeting YOU")
- Label text: "REACTION QUEUE" or "INCOMING THREATS" above circles (fade out after first few combats?)

#### Urgency Indicators

**Queue Overflow Warning:**
- When queue is full (3/3 active threats), leftmost threat gets distinct visual:
  - **Red pulsing border** (this one resolves immediately if new threat arrives)
  - **Screen shake** or **audio sting** when threat forced to resolve due to overflow

**Final Moments:**
- When timer < 20% remaining:
  - **Faster pulsing animation**
  - **Audio tick** (optional, toggleable)
  - **Brightening glow** (draws eye urgently)

#### Accessibility

- **Colorblind Mode:** Icons + shapes (don't rely on red/orange alone)
- **Visual Noise Toggle:** Option to hide countdown text (just keep ring/icon)
- **Audio Cues:** Toggleable sound for new threats, expiring threats

---

### 2. Action Bar (Player Abilities) ⚠️ CURRENTLY MISSING

**Current Problem:** Player abilities (Q/W/E/R) have no visual representation. Players don't know what they can press or when.

**Goal:** Clear display of available abilities, costs, and readiness state.

#### Layout & Positioning

**Location:** Bottom-center, directly below resource pools
- Keeps all "player state" information in one eye-line
- Left-to-right scanning: abilities → resources → check → act

**Visual Structure:**
```
                [Yellow Bar ─────] [Red Bar ─────] [Blue Bar ─────]
                   (Stamina)         (Health)         (Mana)

                [Q]      [W]      [E]      [R]
              Ability  Ability  Ability  Ability
               Slot 1   Slot 2   Slot 3   Slot 4
```

#### Ability Slot Design

**Shape:** Rectangular boxes (distinct from circular threat indicators)
**Size:** Larger than reaction queue indicators (these are player actions, more important)

**Each Slot Shows:**

1. **Keybind Label** - Large, prominent (Q/W/E/R)
   - Top-left corner or center-top
   - Always visible

2. **Ability Icon** - Visual representation
   - Sword icon for Basic Attack
   - Blur/dash for Dodge
   - Custom icons for each ability

3. **Resource Cost** - Small badge showing cost
   - Bottom-right corner
   - Yellow droplet + "30" for 30 stamina
   - Blue droplet + "40" for 40 mana
   - If no cost: show nothing or "FREE"

4. **Cooldown/GCD Overlay** - Gray fill over icon when on cooldown
   - Radial sweep (clockwise) showing cooldown remaining
   - Numeric countdown text optional (0.5s, 1.2s)

5. **State Indicators:**
   - **Ready:** Full color, no overlay
   - **On Cooldown:** Gray overlay with sweep/timer
   - **Insufficient Resources:** Red outline + dim icon
   - **Out of Range:** Orange outline + dim icon (if target too far)
   - **No Target:** Yellow outline (if ability needs target but none selected)

#### Empty Ability Slots

- If player hasn't unlocked 4 abilities yet, show empty slot:
  - Faint gray outline
  - Lock icon or "EMPTY" text
  - No keybind label (key does nothing)

#### Visual Example

```
     [Stamina ───────] [Health ──────] [Mana ────────]

      ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
      │    Q    │  │    W    │  │    E    │  │    R    │
      │         │  │         │  │         │  │         │
      │   ⚔️    │  │   🔥    │  │   🌀   │  │  EMPTY  │
      │         │  │ ●●●○○  │  │         │  │   🔒    │
      │  FREE   │  │ 30 💧  │  │ 30 💧  │  │         │
      └─────────┘  └─────────┘  └─────────┘  └─────────┘
      (Ready)      (Cooldown)    (Ready)      (Locked)
```

---

### 3. Target Indicator Enhancements

**Current:** Red circle on hostile target (working well)

**Needed Additions:**

#### Ally Target Indicator (Green)

- **Same visual style** as hostile indicator but green
- Shows nearest ally in facing direction
- Only visible when:
  - Player has ally-target ability equipped? (always show vs contextual)
  - Ally exists within range

#### Tier Lock Feedback

When player presses 1/2/3 to lock range tier:

**Tier Badge on Target:**
- Small numbered badge on target indicator
- "1", "2", or "3" in corner of red/green circle
- Different color? (white text on dark background)

**Tier Lock with No Valid Targets:**
- Highlight facing cone in that range tier
- Faint red overlay on ground hexes within locked range
- Shows empty search area (communicates "you're locked, but no targets here")
- Visual prompt: "No targets in CLOSE range" text near queue?

#### Manual Lock (TAB Cycle) Feedback

When player manually cycles with TAB:
- **Thicker border** on target indicator (double-ring)
- **Pulsing effect** (subtle)
- **Badge:** Small "TAB" icon or manual lock symbol
- Communicates: "You've overridden auto-targeting"

#### Target Information Overlay (Optional)

**Minimal Info on Target:**
- Entity name/type (small text above health bar)
- Distance in hexes (small text: "3 hexes")
- Only show when locked or in combat with that target

---

### 4. Enemy Health Bars (World Space)

**Current:** Not visible (blocking clarity on "am I winning?")

**Goal:** Show basic enemy health state at a glance

#### Design

**Style:** Horizontal bar above each enemy entity (world space)
- **Width:** Narrower than player health bar (less prominent)
- **Height:** Thin (2-3 pixels at 1080p)
- **Color:** Orange or yellow fill (distinct from player red)
- **Background:** Black or very dark (shows lost HP)
- **Includes:** Entity name/type (small text above bar: "Wild Dog")

#### Visibility Rules

**Always Show For:**
- Current target (always visible)
- Enemies in combat with player
- Enemies within 10-15 hexes (even if not in combat)

**Visual Distinction:**
- **Current Target:** Slightly thicker bar, brighter color
- **Other Enemies:** Thinner bar, lower opacity (50-70%)

**Purpose:** Quick reference only - detailed info in Target Frame (see below)

---

### 5. Target Detail Frame (Top-Right)

**Current:** Not implemented

**Goal:** Show detailed information about current target for informed decision-making

#### When to Show

**Simple Rule:** Appears automatically when you have a target selected
- Current target changes → frame updates instantly
- No target selected → frame disappears
- No special input required (no tab-lock, tier-lock, modifier key)

#### Layout & Positioning

**Location:** Top-right corner of screen

```
┌─────────────────────┐
│ Wild Dog        3h  │ ← Name + Distance
│ ─────────── 80/100  │ ← HP Bar + Numbers
│                     │
│ 💧 ────── 45/80     │ ← Stamina (if entity has it)
│ 💠 ──── 20/60       │ ← Mana (if entity has it)
│                     │
│ QUEUE: (⚔️) ( ) ( ) │ ← Their threat queue
│        0.6s         │    (if entity has one)
└─────────────────────┘
```

#### Frame Contents (MVP)

**1. Entity Name + Distance**
- Top line: Entity type/name ("Wild Dog", "Forest Sprite", "Player Name")
- Distance: Hex count from player (right-aligned: "3h" = 3 hexes)

**2. Health Bar + Numbers**
- Full-width bar (same style as world-space bar)
- Exact HP numbers: "80/100" (current/max)
- Color: Orange/yellow fill, black empty

**3. Resource Pools** (if entity has them)
- **Stamina bar:** Yellow droplet icon 💧 + bar + numbers
- **Mana bar:** Blue droplet icon 💠 + bar + numbers
- **If entity doesn't have resources:** Skip this section (simple enemies)

**4. Threat Queue** (if entity has one)
- **Label:** "QUEUE:" to clarify what these icons mean
- **Same visual style** as player's reaction queue (circular threats)
- **Smaller size:** 50-60% of player's queue indicators
- **Shows:** Active threats only (no empty slots displayed)
- **Timer rings:** Depleting clockwise around each threat
- **Timer text:** Countdown seconds below (0.6s, 0.3s, etc.)
- **If entity doesn't have queue:** Skip this section (simple AI enemies)

#### What This Tells Players

**Resource Pools Answer:**
- "Can they afford to dodge?" (stamina check)
- "Can they cast abilities?" (mana check)
- "Are they low on resources?" (press advantage)

**Threat Queue Answers:**
- "Are they busy reacting to threats?" (distracted)
- "Is their queue almost full?" (add threat to force overflow)
- "How much time before their threats resolve?" (timing window)

#### Scaling by Enemy Type

**Basic Enemies (Wild Dog, Simple AI):**
- Show: Name, distance, HP only
- Skip: Resources (they don't use them strategically)
- Skip: Threat queue (no reaction system)

**Smart Enemies / Elites:**
- Show: Name, distance, HP, resources
- Show: Threat queue (if they have reaction system)

**Player Targets (PvP):**
- Show: Everything (name, distance, HP, stamina, mana, queue)
- Critical for competitive counterplay

**Boss Enemies:**
- Show: Everything
- Potential future: Phase indicators, special mechanics

#### Frame Behavior

**Auto-Update:**
- All values update in real-time
- HP bar drains as enemy takes damage
- Resources deplete as enemy uses abilities
- Queue updates as threats added/resolved

**Target Switch:**
- Frame instantly switches to new target
- Smooth transition (no jarring pop)
- Previous target world-space HP bar returns to minimal style

**No Target:**
- Frame fades out gracefully
- Empty state: no frame visible

#### Why Top-Right?

**Reasoning:**
1. **Conventional placement** - Many games use top-right for target info (familiar)
2. **Out of combat area** - Doesn't block center action
3. **Visual flow:** Eyes scan center (combat) → top-right (target details) → bottom (your resources/abilities)
4. **Leaves top-left open** for future party frames / ally info

---

## High Priority Additions

### 6. Facing/Heading Indicator

**Current:** Character sprite rotates to face heading
**Problem:** Rotation alone may be too subtle, especially after movement stops

**Enhancement Options:**

#### Option A: Directional Arrow (Minimal)
- Small arrow attached to character sprite
- Points in facing direction (60° cone center)
- Subtle but always visible
- **Recommendation:** Start here, test if sufficient

#### Option B: Facing Cone Overlay (Tactical)
- Faint 60° cone overlay on ground
- Shows targeting area (what's "in front" of you)
- Toggle visibility:
  - Always on during combat?
  - Press-and-hold a key to show? (SHIFT for tactical view)
  - Only when tier-locked or TAB-locked?

#### Option C: Position on Hex (Subtle)
- Character offset toward facing direction (not hex-centered)
- Already in combat-system.md spec as intended
- May need exaggeration to be noticeable

**Recommendation:** Implement A + C (arrow + position offset), add B as optional toggle for players who want it.

---

### 7. Range Feedback System

**Problem:** Players won't know if target is in range for ability until they try to cast (frustrating!)

**Solutions:**

#### Option A: Distance Counter
- Small text near target indicator
- "3 hexes away" or just "3h"
- Updates in real-time as player/target moves

#### Option B: Range Ring Preview
- When player hovers/preps ability (before pressing key):
  - Faint circle around player showing ability range
  - Hexes within range highlighted subtly
- Shows "can I hit from here?" before committing

#### Option C: Ability Icon State
- Out-of-range abilities dim/gray out
- Orange border + dim icon = "target too far"
- Instant feedback without requiring range calculation

**Recommendation:** Implement Option C (always-on feedback) + Option A (explicit distance). Option B as enhancement later if needed.

---

### 8. Combat State Indicator

**Problem:** Players may not know if they're "in combat" (affects regen, travel, interactions)

**Visual Indicators:**

**In Combat:**
- Small icon or badge (crossed swords ⚔️) top-left corner
- Optional: Faint red border/vignette around screen edges
- Combat music starts (audio cue)
- Reaction queue becomes visible (if hidden out of combat)

**Out of Combat:**
- Icon disappears
- Border fades out
- Music transitions to exploration
- Optional: "Leaving combat..." fade text when transitioning

**Edge Case:**
- When leashing from enemy (ran too far):
  - "Enemy disengaged" text briefly
  - Prevents confusion on why combat ended

---

## Medium Priority (Polish)

### 9. Damage Numbers

**Purpose:** Visual feedback on damage dealt/taken

**Design:**
- Floating numbers above entity when damage occurs
- Brief lifespan (0.5-1.0s), fade up and out
- **Color coding:**
  - **White:** Normal damage dealt
  - **Yellow:** Critical hit (larger, bold)
  - **Red:** Damage taken
  - **Green:** Healing received
  - **Blue:** Shield/absorption

**Toggleable:**
- Some players find damage numbers cluttered
- Option in settings: "Show Damage Numbers" (on/off/self only/enemies only)

---

### 10. Status Effects Display

**Purpose:** Show buffs/debuffs on player

**Layout:**
```
              [Debuffs: 🔥 Burning, 💀 Poisoned]  ← Above character

                        🧍 Player

              [Buffs: ⚡ Haste, 🛡️ Ward]  ← Below character
```

**Design:**
- Small circular icons
- Timer ring around icon (shows duration remaining)
- Tooltip on hover (what does this do?)
- Max 5-6 visible (stack count if more)

**Priority:**
- Lower priority for MVP (no status effects implemented yet)
- Document now for future implementation

---

### 11. Ability Tooltips

**Hover/Hold Keybind for Details:**
- Hovering over ability slot (or holding Q/W/E key for 0.5s):
  - Tooltip appears with:
    - Ability name
    - Description (1-2 sentences)
    - Resource cost
    - Cooldown (if any)
    - Range/pattern (single target, line, radius, etc.)

**Example:**
```
┌─────────────────────────┐
│ DODGE (E)               │
│ Evade all queued threats│
│                         │
│ Cost: 30 Stamina        │
│ Cooldown: 0.5s (GCD)    │
│ Range: Self             │
└─────────────────────────┘
```

**Accessibility:** Helps new players learn abilities without reading external docs

---

## Screen Layout Summary

### Full HUD Layout (In Combat)

```
┌────────────────────────────────────────────────────────────────────────┐
│  ⚔️ IN COMBAT    [Time/Date]           ┌────────────────┐ [FPS/Debug] │ Top Bar
│                                         │ Wild Dog    3h │              │
│              REACTION QUEUE             │ ────── 80/100  │ Target      │ Top Center
│          (⚔️) (🔥) ( )  ← Your threats  │                │ Frame       │
│          0.8s 0.4s  -                   │ 💧 ──── 45/80  │ (Top-Right) │
│                                         │ 💠 ── 20/60    │              │
│         ┌───┐ Enemy (orange bar)        │                │              │ Combat Area
│         │ 🎯│  ← Target                 │ (⚔️) ( ) ( )  │              │
│         └───┘  Indicator                │  0.6s         │              │
│                                         └────────────────┘              │
│                     🧍 Player                                           │
│                    (arrow)   ← Facing indicator                         │
│                  [🛡️ ⚡]    ← Buffs                                     │
│                                                                          │
│         Enemy (faint bar)   Enemy (faint bar)                           │
│                                                                          │
│                                                                          │
│         [Stamina ─────] [Health ─────] [Mana ─────]                    │ Bottom
│                                                                          │
│           [Q]     [W]     [E]     [R]                                   │ Action Bar
│          ⚔️ Free 🔥 30💧 🌀 30💧  🔒 Empty                              │
└────────────────────────────────────────────────────────────────────────┘
```

### Out of Combat (Minimal HUD)

```
┌──────────────────────────────────────────────────────────────┐
│  [Time/Date]                                    [FPS/Debug]  │
│                                                                │
│                                                                │
│                [Reaction Queue: Hidden]                       │
│                                                                │
│                                                                │
│                                                                │
│                     🧍 Player                                 │
│                                                                │
│                                                                │
│                                                                │
│                                                                │
│         [Stamina ─────] [Health ─────] [Mana ─────]          │
│                                                                │
│           [Q]     [W]     [E]     [R]                         │
└──────────────────────────────────────────────────────────────┘
```

**Note:** Reaction queue could remain visible out of combat (just empty) or hidden entirely. Playtesting will determine which feels less jarring.

---

## Accessibility & Options

### Colorblind Modes

- **Protanopia/Deuteranopia (Red-Green):**
  - Replace red hostile indicator with orange + symbol (triangle)
  - Replace green ally indicator with blue + symbol (square)

- **Tritanopia (Blue-Yellow):**
  - Adjust resource bar colors (use distinct brightness levels)

### Visual Noise Toggles

- **Combat Text:** On / Off / Self Only / Enemies Only
- **Facing Cone Overlay:** Always / Combat Only / Hold Key / Off
- **Damage Numbers:** On / Off / Crits Only
- **Status Effect Tooltips:** Hover / Always / Off
- **Threat Countdown Text:** On / Off (keep visual ring only)

### UI Scale

- **Small / Medium / Large** scale options
- All text remains readable at 720p, 1080p, 1440p, 4K

---

## Implementation Priority

### Phase 1: Critical (Blocks MVP)
1. ✅ Target indicator (hostile) - Already exists
2. **Reaction queue visual redesign** - Circular threats, icons, timers
3. **Action bar addition** - Q/W/E/R with keybinds, costs, cooldowns
4. **Enemy health bars (world space)** - Basic HP bars above entities
5. **Target detail frame** - Top-right frame showing target HP, resources, queue

### Phase 2: High Priority (Reduces Frustration)
6. **Facing indicator** - Arrow + position offset
7. **Range feedback** - Ability dimming when out of range
8. **Tier lock indicators** - Badge on target, cone highlight when empty
9. **Combat state indicator** - "In Combat" icon

### Phase 3: Polish (Improves Feel)
10. **Ally target indicator** (green) - When ally targeting needed
11. **Damage numbers** - Floating text
12. **Manual lock feedback** (TAB) - Border changes
13. **Status effects** - When system implemented

### Phase 4: Quality of Life
14. **Ability tooltips** - Hover for details
15. **Colorblind modes** - Accessibility
16. **UI scale options** - Resolution support

---

## Open Questions (Needs Playtesting)

### Visual Design:
- **Reaction queue size:** How large should circular threats be? Too big = obstructs view, too small = hard to read
- **Action bar size:** Should abilities be larger than reaction threats? (yes, but by how much?)
- **Facing cone toggle:** Always on, combat-only, or opt-in? (needs player testing)
- **Enemy health bar fade distance:** 10 hexes? 15? Too many bars = clutter

### Information Density:
- **Countdown text on threats:** Helpful or cluttered? (option to toggle)
- **Distance counter on targets:** Always show or only when tier-locked?
- **Ability tooltips:** Hover vs hold-key to reveal?
- **Status effects:** Show all or only important ones?

### Behavior:
- **Reaction queue out of combat:** Hidden or visible-but-empty?
- **Target health bar:** Show when full HP or hide until damaged?
- **Combat state transitions:** Fade time when entering/exiting combat?
- **GCD visual:** Dim all abilities during 0.5s GCD or individual cooldown rings?

### Performance:
- **Animated elements:** How many pulsing/glowing effects before FPS drops?
- **UI rendering cost:** Separate render pass for UI or integrated?

---

## Success Criteria (Player Can Play Without Confusion)

✅ **I always know which enemy I'm targeting** (red indicator unambiguous)
✅ **I can see incoming threats clearly** (reaction queue visually distinct, shows type and time)
✅ **I know which direction I'm facing** (arrow + position offset)
✅ **I know what abilities I can use** (action bar shows keybinds, costs, cooldowns, range state)
✅ **I can see enemy health state** (health bars show damage dealt, progress toward kill)
✅ **I can see detailed target information** (target frame shows resources, queue state for decision-making)
✅ **I understand when I'm in combat** (visual/audio cues clear)
✅ **I can manage resources** (stamina/health/mana visible and readable)
✅ **I know if abilities will work before I press them** (range, cost, cooldown feedback)
✅ **I'm not confused by UI ambiguity** (every element has distinct visual language)
✅ **I can learn through play** (no manual reading required for basic understanding)

---

## Design Rationale

### Why Circular Threats vs Square Abilities?

**Shape = Identity.** In fast-paced combat, players need to distinguish systems instantly:
- **Circles = Threats (reaction needed, time pressure, incoming danger)**
- **Rectangles = Abilities (player actions, deliberate choices)**

This visual language carries throughout the HUD and prevents confusion.

### Why Bottom-Center for Action Bar?

**Scanning Pattern:** Eyes naturally move bottom → up when checking "can I act?"
1. Glance at action bar (what's ready?)
2. Glance at resources (can I afford it?)
3. Glance at target (in range?)
4. Execute

Keeping action bar + resources in one eye-line minimizes head movement and cognitive load.

### Why Top-Center for Reaction Queue?

**Urgency Demands Prominence.** Incoming threats require immediate response:
- Top-center = primary focus area (just above player character)
- Falling into your "danger zone" mentally
- Reaction abilities in bottom-center = hands move down to respond (tactile association)

### Why Enemy Health Bars Above Entities?

**Spatial Association.** Health belongs to the entity:
- Bar directly above enemy = instant visual connection
- No scanning required to match "which health bar is which enemy?"
- Works with multiple enemies on screen (each has their own bar)

### Why Target Detail Frame Always Shows (No Lock Required)?

**Simplicity Over Complexity.** Target information should be immediately available:
- **Current target = current interest** - If you're targeting them, you want their details
- **No hidden information** - All relevant data visible without extra keypresses
- **Consistent behavior** - Frame always appears when target exists, always disappears when no target
- **Reduces cognitive load** - No "did I lock?" or "why don't I see info?" confusion
- **Scales naturally** - Basic enemies show minimal info, smart enemies show full details

**Decision-Making Requires Information:**
- "Can they dodge?" requires seeing their stamina NOW, not after pressing lock key
- "Is their queue full?" needs to be glanceable for tactical timing
- Every extra input (tier lock, TAB lock, modifier key) is friction between player and information

**Future Enhancement Path:**
- If frame becomes too cluttered with status effects/buffs/debuffs
- Add optional "detailed view" mode (hold key to expand)
- But core info (HP, resources, queue) stays always-visible

---

## Future Enhancements (Post-MVP)

### Advanced Features (Not Blocking, Consider Later):
- **Party frames** (when multiplayer/grouping added)
- **Combat log** (scrolling text feed of actions)
- **Threat meter** (aggro tracking for tank/DPS roles)
- **Cast bars** (enemy casting indicators for interrupt gameplay)
- **Proc indicators** (special effect active, special ability ready)
- **Combo points** (if combo system added to combat)
- **Rage/Focus bars** (if additional resource types added)

### Customization Options (Nice to Have):
- **HUD editor** (drag/drop UI elements)
- **Element opacity** (fade HUD elements to preference)
- **Hide specific elements** (minimalist HUD players)
- **Size individual elements** (large threats, small abilities, etc.)
- **Color themes** (different color schemes for UI)

---

## Related Specifications

- [Combat System Specification](combat-system.md) - Core mechanics this HUD supports
- [Attribute System Specification](attribute-system.md) - How attributes affect UI (Instinct → timer durations, Focus → queue capacity)

---

## Changelog

- **2025-10-31:** Initial draft (PLAYER role)
  - Identified reaction queue exists but needs visual redesign
  - Proposed action bar addition (bottom-center)
  - Defined visual hierarchy and distinct shape languages
  - Established implementation priority phases
