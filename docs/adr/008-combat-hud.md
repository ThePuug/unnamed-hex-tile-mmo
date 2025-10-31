# ADR-008: Combat HUD Implementation

## Status

Proposed (Revised)

**Revision History:**
- 2025-10-31: Initial draft (12-16 days, 5 features)
- 2025-10-31: Revised based on PLAYER feedback (17-23 days, 8 features)
  - Added: Action Bar (Phase 2)
  - Added: Target Detail Frame (Phase 5)
  - Added: Facing Indicator (Phase 7)
  - Justification: combat-hud.md Phase 1 Critical requirements

## Context

### Combat System UI Requirements

Previous ADRs defined the combat backend systems but left UI implementation details unspecified:

- **ADR-002:** Health/Stamina/Mana components exist, but no visual bars
- **ADR-003:** ReactionQueue component exists, but no UI display
- **ADR-004:** Directional targeting system exists, but no target indicators
- **ADR-005:** Damage pipeline exists, damage numbers covered

From `docs/spec/combat-system.md` MVP requirements (lines 560-621) and `docs/spec/combat-hud.md` Phase 1 Critical features:

**Required UI Systems (Phase 1 Critical):**
1. **Resource Bars:** Display Health/Stamina/Mana (player only)
2. **Reaction Queue Display:** Show queued threats with timers (player only)
3. **Action Bar:** Display Q/W/E/R abilities with costs, cooldowns, ready states
4. **Target Indicators:** Red circle on hostile target, green circle on ally target
5. **Target Detail Frame:** Show enemy HP, resources, queue state (top-right)
6. **Health Bars:** Display Health on all entities in combat (world-space)
7. **Combat State Feedback:** Visual indication of in-combat state

**Additional Requirements (Phase 2 High Priority):**
- **Facing Indicator:** Arrow + position offset to show heading direction

**Existing UI:**
- Damage numbers (ADR-005 Phase 3)
- Floating text system (ADR-005)

### Current Codebase State

**Components Exist (Backend):**
- `Health { state: f32, step: f32 }` (ADR-002)
- `Stamina { state: f32, step: f32 }` (ADR-002)
- `Mana { state: f32, step: f32 }` (ADR-002)
- `ReactionQueue { threats: Vec<QueuedThreat>, capacity: usize }` (ADR-003)
- `CombatState { in_combat: bool, last_combat_time: Duration }` (implied by spec)

**UI Missing:**
- Resource bar rendering (Health/Stamina/Mana)
- Reaction queue visual display
- Target indicator sprites/circles
- World-space health bars

### Architectural Challenges

#### Challenge 1: UI Layout and Positioning

**Problem:** Where to position UI elements without cluttering screen?

**Options:**
- **Bottom-center cluster:** Resources + queue together (compact, eye-line)
- **Distributed:** Resources bottom, queue top (clear separation)
- **Contextual:** Only show in combat (reduces noise)

**Considerations:**
- Player scans center of screen during combat (character position)
- Top/bottom edges for non-urgent info (resources)
- Center-top for urgent info (incoming threats)

#### Challenge 2: Reaction Queue Timer Visualization

**Problem:** How to show depleting timers clearly?

**Options:**
- **Circular progress ring:** Depletes clockwise (MOBA-style)
- **Linear bar:** Fill decreases left-to-right (simple)
- **Numeric countdown:** Text shows "0.8s" remaining (precise)

**Considerations:**
- Timers are critical (must be instantly readable)
- Multiple timers running simultaneously (3 threats)
- Urgency indication (final 20% of timer needs visual alarm)

#### Challenge 3: Target Indicator Implementation

**Problem:** How to render indicators on entities?

**Options:**
- **Ground decal:** Circle sprite on ground beneath entity
- **Sprite attachment:** Circle sprite parented to entity
- **Outline shader:** Shader effect on entity sprite

**Considerations:**
- Must be visible on any terrain/background
- Must follow entity movement (interpolation)
- Performance (multiple entities may have indicators)

#### Challenge 4: Health Bar Visibility

**Problem:** When to show health bars?

**Options:**
- **Always visible:** All entities show health bars always
- **In-combat only:** Only show when CombatState active
- **When damaged:** Only show when Health < max

**Considerations:**
- Visual noise (20 NPCs = 20 health bars)
- Combat clarity (need to see enemy HP)
- Performance (text rendering overhead)

#### Challenge 5: Resource Bar Scale and Layout

**Problem:** How to layout 3 resource bars compactly?

**Options:**
- **Stacked vertical:** 3 bars stacked (Health top, Mana bottom)
- **Horizontal row:** 3 bars side-by-side (wide, uses horizontal space)
- **Nested/Overlay:** Health large, Stamina/Mana smaller overlays

**Considerations:**
- Screen real estate (bottom-center is prime space)
- Readability (bar size vs visual clutter)
- Color coding (Red=Health, Yellow=Stamina, Blue=Mana)

### Design Principles from Combat System Spec

From spec (lines 5-13):
- **Conscious but Decisive** - UI must inform without overwhelming
- **No twitch mechanics** - UI updates smooth, not jittery
- **Clarity** - Always know combat state at a glance

From spec (lines 199-207) - Reaction Queue Display:
```
[⚔️ ●●○○○] [🔥 ●●●○○] [⚔️ ●●●●○]
  0.4s left   0.8s left   1.2s left
```

**Visual Requirements:**
- Circular icons with timer rings
- Attack type icons (physical ⚔️, magic 🔥)
- Left-to-right = order of resolution (soonest first)

---

## Decision

We will implement a **modular combat HUD system with distinct visual languages** for different information types.

### Core Architectural Principles

#### 1. UI Layering and Separation

**Three Visual Layers:**

1. **World-Space UI:** Health bars, target indicators (attached to entities)
2. **Screen-Space Overlay:** Resource bars, reaction queue (fixed screen position)
3. **Floating UI:** Damage numbers (world-space but ephemeral)

**Rationale:**
- World-space: Contextual to entities (health, targeting)
- Screen-space: Player state (resources, queue)
- Floating: Event feedback (damage dealt/taken)

#### 2. Visual Language Consistency

**Shapes Communicate Purpose:**
- **Circular:** Threats, timers, urgent depleting resources
- **Rectangular:** Abilities, stable resources, inventory (future)
- **Icons:** Attack types, ability types, status effects

**Colors Communicate State:**
- **Red:** Health, damage, danger, hostile targets
- **Yellow/Orange:** Stamina, physical resources, warnings
- **Blue:** Mana, magic resources, information
- **Green:** Healing, buffs, ally targets
- **Gray:** Inactive, disabled, unavailable

**Rationale:**
- Instant recognition without reading text
- Colorblind-safe when combined with shapes/icons
- Consistent with genre conventions (red=health universal)

#### 3. Minimal Client Prediction for UI

**Resource Bars:**
- Use `step` for local player (predicted)
- Use `state` for remote entities (server-confirmed)

**Reaction Queue:**
- Client maintains queue state (synced via Do::InsertThreat)
- Client runs timer countdown (local interpolation)
- Server authoritative for insertion/removal

**Health Bars:**
- Remote entities: Use `state` only (no prediction)
- Local player: Use `step` (predicted damage from ADR-005)

**Rationale:**
- Local player: instant feedback critical
- Remote: accuracy > responsiveness
- Matches existing prediction patterns (Offset component)

---

### Detailed Design Decisions

#### Decision 1: Resource Bar Layout (Bottom-Center)

**Location:** Bottom-center of screen, fixed position

**Layout:** Horizontal row, left-to-right: Stamina, Health, Mana

```
     ┌──────────────────────────────────────────┐
     │      ╔════════════════════════╗          │
     │      ║  STAMINA  HEALTH  MANA ║          │
     │      ║  [████  ] [█████] [███]║          │
     │      ╚════════════════════════╝          │
     └──────────────────────────────────────────┘
            Bottom-center of screen
```

**Bar Specifications:**

**Stamina Bar (Yellow):**
```rust
// src/client/systems/ui/resource_bars.rs
const STAMINA_COLOR: Color = Color::rgb(0.9, 0.8, 0.0);  // Yellow
const STAMINA_BG: Color = Color::rgb(0.3, 0.3, 0.0);     // Dark yellow
width: 150px
height: 20px
position: bottom-center + offset(-155, 50)  // Left of Health
```

**Health Bar (Red):**
```rust
const HEALTH_COLOR: Color = Color::rgb(0.9, 0.1, 0.1);   // Red
const HEALTH_BG: Color = Color::rgb(0.3, 0.0, 0.0);      // Dark red
width: 150px
height: 20px
position: bottom-center + offset(0, 50)  // Center
```

**Mana Bar (Blue):**
```rust
const MANA_COLOR: Color = Color::rgb(0.1, 0.4, 0.9);     // Blue
const MANA_BG: Color = Color::rgb(0.0, 0.1, 0.3);        // Dark blue
width: 150px
height: 20px
position: bottom-center + offset(155, 50)  // Right of Health
```

**Bar Rendering:**
```rust
// Each bar has 2 rectangles:
// 1. Background (full width, dark color)
// 2. Foreground (fill width = current/max * width, bright color)

let fill_width = (current / max) * bar_width;
// Background: full bar_width
// Foreground: fill_width (aligned left)
```

**Text Labels:**
- Show "current / max" centered on each bar
- Font size: 12pt
- Color: White
- Example: "120 / 150"

**Visibility:**
- Always visible when player exists
- Hide when player dead or despawned
- No fading/hiding (combat or not)

**Why this layout:**
- Center-bottom is prime real estate (near character)
- Horizontal layout uses screen width efficiently
- Health in center (most important resource)
- Left-to-right scanning natural

---

#### Decision 2: Reaction Queue Display (Top-Center)

**Location:** Top-center of screen, fixed position (below top edge by 50px)

**Layout:** Horizontal row of circular threat indicators

```
              ┌───────────────────────────┐
              │   INCOMING THREATS        │
              │  ╭───╮  ╭───╮  ╭───╮     │
              │  │⚔️ │  │🔥 │  │   │     │
              │  │●●○│  │●●●│  │   │     │
              │  ╰───╯  ╰───╯  ╰───╯     │
              │  0.4s    0.8s   (empty)   │
              └───────────────────────────┘
                    Top-center of screen
```

**Threat Indicator Specifications:**

**Circular Container:**
```rust
// src/client/systems/ui/reaction_queue.rs
radius: 40px
background: Color::rgba(0.1, 0.0, 0.0, 0.8)  // Dark red, semi-transparent
border: 3px solid Color::rgb(0.9, 0.3, 0.0)  // Orange border
spacing: 10px between circles
```

**Timer Ring:**
```rust
// Circular progress indicator (depletes clockwise)
// Uses bevy_egui or custom circle rendering
start_angle: 0° (top)
end_angle: progress * 360°  // progress = time_remaining / timer_duration
color: Color::rgb(0.9, 0.3, 0.0)  // Orange
width: 4px (ring thickness)
```

**Attack Type Icon:**
```rust
// Center of circle
icon_size: 32px × 32px
icons: {
    Physical: "⚔️" (or sword sprite),
    Magic: "🔥" (or flame sprite),
}
color: White
```

**Countdown Text (Optional):**
```rust
// Below circle
font_size: 10pt
color: White
format: "{:.1}s" (e.g., "0.8s")
```

**Empty Slots:**
```rust
// Shows queue capacity
background: Color::rgba(0.1, 0.1, 0.1, 0.3)  // Very faint gray
border: 1px solid Color::rgba(0.3, 0.3, 0.3, 0.5)  // Faint gray border
no icon, no timer, no text
```

**Urgency Indicators:**

When timer < 20% remaining:
```rust
if time_remaining < timer_duration * 0.2 {
    // Pulsing animation
    border_color = lerp(orange, red, sin(time * 8.0))  // Fast pulse
    border_width = lerp(3px, 5px, sin(time * 8.0))
    // Optional: audio tick
}
```

**Queue Overflow Warning:**

When queue is full (3/3 active):
```rust
// Leftmost threat gets distinct visual
if queue.threats.len() == queue.capacity {
    threats[0].border_color = Color::RED;
    threats[0].pulsing = true;  // Red pulsing = "this resolves if overflow"
}
```

**Component Structure:**
```rust
// src/client/components/reaction_queue_ui.rs
#[derive(Component)]
pub struct ReactionQueueDisplay {
    pub threat_circles: Vec<Entity>,  // UI entities for each slot
}

#[derive(Component)]
pub struct ThreatCircleUI {
    pub slot_index: usize,
    pub threat_data: Option<QueuedThreat>,  // None = empty slot
}
```

**Update System:**
```rust
// src/client/systems/ui/reaction_queue.rs
pub fn update_reaction_queue_ui(
    time: Res<Time>,
    player_query: Query<&ReactionQueue, With<Controlled>>,
    mut ui_query: Query<(&ThreatCircleUI, &mut Style, &mut BackgroundColor)>,
) {
    // For each threat circle:
    // - Update timer ring progress
    // - Update countdown text
    // - Apply urgency animations
    // - Sync with ReactionQueue component state
}
```

**Why this design:**
- Top-center is natural "incoming info" location
- Circular = urgent, depleting resource (matches mental model)
- Timer ring instantly readable (no need to interpret numbers)
- Left-to-right = temporal order (soonest expires first)

---

#### Decision 3: Target Indicators (World-Space Sprites)

**Hostile Target Indicator (Red Circle):**

**Visual:**
```rust
// src/client/systems/ui/target_indicators.rs
sprite: Circle (or ring sprite)
radius: 0.5 world units (slightly larger than entity)
color: Color::rgba(1.0, 0.0, 0.0, 0.7)  // Red, semi-transparent
position: entity.translation + Vec3::new(0.0, -0.2, -0.1)  // Below entity, render behind
```

**Ally Target Indicator (Green Circle):**
```rust
sprite: Circle (or ring sprite)
radius: 0.5 world units
color: Color::rgba(0.0, 1.0, 0.0, 0.7)  // Green, semi-transparent
position: entity.translation + Vec3::new(0.0, -0.2, -0.1)
```

**Tier Lock Badge:**

When tier lock active (1/2/3 pressed):
```rust
// Small badge attached to target indicator
sprite: Small circle with number
radius: 0.15 world units
color: Color::rgb(1.0, 1.0, 0.0)  // Yellow
text: "1", "2", or "3"
position: target_indicator.position + Vec3::new(0.3, 0.3, 0.1)  // Top-right
```

**TAB Lock Marker:**

When TAB lock active (manual selection):
```rust
// Additional border around indicator
outer_ring: radius = 0.6 world units
color: Color::rgba(1.0, 1.0, 1.0, 0.5)  // White outline
width: 0.05 world units
```

**Component Structure:**
```rust
// src/client/components/target_indicator.rs
#[derive(Component)]
pub struct TargetIndicator {
    pub target_type: TargetType,  // Hostile or Ally
}

pub enum TargetType {
    Hostile,
    Ally,
}
```

**System Flow:**
```rust
// src/client/systems/ui/target_indicators.rs
pub fn update_target_indicators(
    mut commands: Commands,
    targeting_query: Query<(Entity, &Target, &TierLock, &TabLock), With<Controlled>>,
    indicator_query: Query<(Entity, &TargetIndicator)>,
) {
    // 1. Despawn old indicators
    // 2. Spawn indicator on current hostile target
    // 3. Spawn indicator on current ally target (if exists)
    // 4. Add tier badge if tier lock active
    // 5. Add TAB marker if TAB lock active
}
```

**Rendering:**
- Use Bevy 2D sprites (Transform + Sprite components)
- Parent to target entity (Transform hierarchy)
- Z-order: Behind entity sprite (render first)

**Why this design:**
- Ground-based circles are genre-standard (RTS, MOBA)
- World-space attachment follows entity movement
- Semi-transparent doesn't obscure terrain
- Badges/markers add context without clutter

---

#### Decision 4: World-Space Health Bars (Above Entities)

**Visual:**
```rust
// src/client/systems/ui/health_bars.rs
width: 1.0 world units
height: 0.1 world units
position: entity.translation + Vec3::new(0.0, 1.5, 0.5)  // Above entity

// Two rectangles:
// 1. Background (gray, full width)
background_color: Color::rgb(0.2, 0.2, 0.2)
// 2. Foreground (red, fill width = current/max)
foreground_color: Color::rgb(0.9, 0.1, 0.1)

let fill_width = (health.current / health.max) * bar_width;
```

**Visibility Rules:**
```rust
// Show health bar if:
// 1. Entity in combat (CombatState component), OR
// 2. Health < max (damaged), OR
// 3. Entity is player (always show)

pub fn should_show_health_bar(
    entity_type: &EntityType,
    health: &Health,
    combat_state: Option<&CombatState>,
) -> bool {
    match entity_type {
        EntityType::Player => true,  // Always show for players
        _ => {
            health.state < health.max() ||  // Damaged
            combat_state.map(|c| c.in_combat).unwrap_or(false)  // In combat
        }
    }
}
```

**Component Structure:**
```rust
// src/client/components/health_bar.rs
#[derive(Component)]
pub struct HealthBar {
    pub background: Entity,  // Gray background bar
    pub foreground: Entity,  // Red fill bar
}
```

**Update System:**
```rust
// src/client/systems/ui/health_bars.rs
pub fn update_health_bars(
    mut commands: Commands,
    query: Query<(Entity, &Health, &EntityType, Option<&CombatState>)>,
    mut bar_query: Query<(&HealthBar, &mut Transform)>,
) {
    for (entity, health, entity_type, combat_state) in &query {
        if should_show_health_bar(entity_type, health, combat_state) {
            // Ensure health bar exists
            if !bar_query.contains(entity) {
                spawn_health_bar(&mut commands, entity);
            }
            // Update fill width
            let fill_width = (health.step / health.max()) * BAR_WIDTH;
            // Update foreground Transform.scale.x
        } else {
            // Despawn health bar if exists
            if let Ok((bar, _)) = bar_query.get(entity) {
                commands.entity(bar.background).despawn();
                commands.entity(bar.foreground).despawn();
            }
        }
    }
}
```

**Rendering:**
- Use Bevy 2D sprites (colored rectangles)
- Parent to entity (Transform hierarchy)
- Scale foreground sprite X-axis for fill effect
- Z-order: Above entity sprite (render last)

**Why this design:**
- Above entity = natural "status" location
- Show when damaged/in-combat reduces visual noise
- Red universal for health
- Simple scaling for fill animation

---

#### Decision 5: Combat State Feedback

**Problem:** Player needs to know when they're in combat (affects UI visibility, movement restrictions).

**Visual Indicators:**

**1. Screen Vignette (Subtle):**
```rust
// src/client/systems/ui/combat_state.rs
// When in combat, darken screen edges slightly
vignette_strength: 0.15  // Subtle, not intrusive
color: Color::rgba(0.1, 0.0, 0.0, vignette_strength)  // Dark red tint
```

**2. Resource Bar Glow:**
```rust
// Add subtle glow/outline to resource bars when in combat
if in_combat {
    bar_outline: 2px solid Color::rgba(1.0, 0.5, 0.0, 0.6)  // Orange glow
}
```

**3. Combat Music/SFX:**
- Handled by audio system (out of scope for this ADR)

**Update System:**
```rust
pub fn update_combat_state_ui(
    combat_query: Query<&CombatState, With<Controlled>>,
    mut vignette_query: Query<&mut BackgroundColor, With<CombatVignette>>,
    mut bar_query: Query<&mut Style, With<ResourceBarContainer>>,
) {
    if let Ok(combat_state) = combat_query.get_single() {
        if combat_state.in_combat {
            // Apply vignette
            // Apply bar glow
        } else {
            // Remove vignette
            // Remove bar glow
        }
    }
}
```

**Why this design:**
- Vignette = subtle genre convention (used in many action games)
- Doesn't obscure gameplay (low opacity)
- Bar glow = contextual (already looking at resources)

---

#### Decision 6: Action Bar Layout (Bottom-Center, Below Resources)

**Problem:** Players need to see equipped abilities, costs, cooldowns, and ready states. Without this, combat is unplayable (pressing Q/W/E/R blindly).

**Location:** Bottom-center, directly below resource bars

**Layout:** Horizontal row of 4 rectangular ability slots (Q, W, E, R)

```
     ┌──────────────────────────────────────────┐
     │  [Stamina] [Health] [Mana]              │
     │                                          │
     │  [Q]     [W]     [E]     [R]           │
     │ Ability Ability Ability  Empty          │
     └──────────────────────────────────────────┘
            Bottom-center of screen
```

**Ability Slot Specifications:**

**Shape:** Rectangular boxes (distinct from circular threat indicators)

**Size:**
```rust
width: 80px
height: 80px
spacing: 10px between slots
position: bottom-center + offset(0, 100)  // Below resource bars
```

**Each Slot Contains:**

**1. Keybind Label (Always Visible):**
```rust
text: "Q", "W", "E", "R"
position: top-left corner or center-top
font_size: 16pt
color: White
```

**2. Ability Icon (Center):**
```rust
icon_size: 48px × 48px
icons: {
    BasicAttack: "⚔️" (or sword sprite),
    Dodge: "🌀" (or dash sprite),
    FireBolt: "🔥" (or fireball sprite),
    // ... other abilities
}
position: center of slot
```

**3. Resource Cost Badge (Bottom-Right):**
```rust
// Small badge showing cost
badge_size: 24px × 16px
position: bottom-right corner
format: "{cost} 💧" for stamina, "{cost} 💠" for mana
font_size: 10pt
color: Yellow (stamina) or Blue (mana)
// If no cost: show nothing or "FREE" in green
```

**4. Cooldown Overlay:**
```rust
// Gray fill over icon when on cooldown
overlay_color: Color::rgba(0.2, 0.2, 0.2, 0.7)  // Semi-transparent gray
// Radial sweep clockwise (0° to progress * 360°)
// Shows remaining cooldown visually

// Optional: Countdown text
countdown_text: "{:.1}s" (e.g., "0.5s")
position: center of overlay
font_size: 12pt
color: White
```

**5. State Indicators (Border Colors):**
```rust
// Border indicates ability state
match state {
    Ready => Color::rgb(0.3, 0.8, 0.3),          // Green - ready to use
    OnCooldown => Color::rgb(0.5, 0.5, 0.5),     // Gray - cooldown
    InsufficientResources => Color::rgb(0.9, 0.1, 0.1),  // Red - can't afford
    OutOfRange => Color::rgb(0.9, 0.5, 0.0),     // Orange - target too far
    NoTarget => Color::rgb(0.9, 0.9, 0.0),       // Yellow - need target
}
border_width: 3px
```

**Empty Ability Slots:**
```rust
// If ability not unlocked/equipped
background: Color::rgba(0.1, 0.1, 0.1, 0.5)  // Very dark, faint
border: 1px solid Color::rgba(0.3, 0.3, 0.3, 0.5)
icon: Lock symbol 🔒 or "EMPTY" text
no keybind label (key does nothing)
```

**Component Structure:**
```rust
// src/client/components/action_bar.rs
#[derive(Component)]
pub struct ActionBarDisplay {
    pub ability_slots: Vec<Entity>,  // UI entities for each slot (Q, W, E, R)
}

#[derive(Component)]
pub struct AbilitySlotUI {
    pub slot_index: usize,             // 0-3 for Q/W/E/R
    pub ability_type: Option<AbilityType>,  // None = empty slot
    pub keybind: KeyCode,              // Q, W, E, R
}
```

**Update System:**
```rust
// src/client/systems/ui/action_bar.rs
pub fn update_action_bar_ui(
    time: Res<Time>,
    player_query: Query<(&Stamina, &Mana, &Gcd, &EquippedAbilities), With<Controlled>>,
    mut ui_query: Query<(&AbilitySlotUI, &mut BackgroundColor, &mut BorderColor, &Children)>,
    mut icon_query: Query<&mut Visibility>,
    mut text_query: Query<&mut Text>,
) {
    // For each ability slot:
    // - Check if ability equipped
    // - Check if GCD active
    // - Check if resources sufficient
    // - Check if target in range (if needed)
    // - Update border color based on state
    // - Update cooldown overlay (radial sweep + text)
    // - Update resource cost badge
}
```

**Why this design:**
- Bottom-center = near resources (one eye-line for all player state)
- Rectangular = distinct from circular threats (clear visual language)
- Large slots = easy to read at a glance
- Color-coded borders = instant state recognition
- Keybind labels always visible = no guessing which key does what

---

#### Decision 7: Target Detail Frame (Top-Right)

**Problem:** Players need to see enemy HP, resources, and queue state to make tactical decisions. World-space health bars are approximate - need exact values.

**Location:** Top-right corner of screen, fixed position

**Layout:** Compact frame showing current target details

```
     ┌──────────────────────────┐
     │ Wild Dog            3h   │ ← Name + Distance
     │ ==================  80/100 │ ← HP bar + exact numbers
     │                           │
     │ 💧 ============  45/80   │ ← Stamina (if entity has it)
     │ 💠 =======  20/60        │ ← Mana (if entity has it)
     │                           │
     │ QUEUE: (⚔️) ( ) ( )      │ ← Threat queue
     │        0.6s               │   (if entity has one)
     └──────────────────────────┘
```

**Frame Specifications:**

**Container:**
```rust
width: 280px
height: variable (120-200px depending on content)
position: top-right + offset(-10, 10)  // 10px margin from edges
background: Color::rgba(0.1, 0.1, 0.1, 0.85)  // Dark, semi-transparent
border: 2px solid Color::rgba(0.5, 0.5, 0.5, 0.8)  // Gray border
padding: 10px
```

**1. Entity Name + Distance (Header):**
```rust
// Top line
text: "{entity_name}    {distance}h"
font_size: 14pt (name), 12pt (distance)
color: White (name), Gray (distance)
layout: name left-aligned, distance right-aligned
```

**2. Health Bar + Exact Numbers:**
```rust
// Full-width bar
bar_width: 260px (container width - padding)
bar_height: 16px
foreground_color: Color::rgb(0.9, 0.5, 0.0)  // Orange (distinct from player red)
background_color: Color::rgb(0.2, 0.1, 0.0)  // Dark orange/brown
text: "{current:.0}/{max:.0}" (e.g., "80/100")
text_position: right-aligned, on bar
text_color: White
font_size: 12pt
```

**3. Resource Pools (If Entity Has Them):**
```rust
// Stamina bar
icon: "💧" (yellow droplet)
bar_width: 180px (narrower than HP)
bar_height: 12px
color: Yellow fill, dark yellow background
text: "{current:.0}/{max:.0}"

// Mana bar
icon: "💠" (blue diamond)
bar_width: 180px
bar_height: 12px
color: Blue fill, dark blue background
text: "{current:.0}/{max:.0}"

// If entity doesn't have resources: skip section
```

**4. Threat Queue (If Entity Has One):**
```rust
// Label + circular threat indicators
label: "QUEUE:" (left-aligned, gray text)
threat_indicators: {
    size: 32px radius (60% of player queue size)
    spacing: 5px between circles
    style: same as player reaction queue (circular, timer rings)
    timer_text: countdown below each threat
    icons: attack type icons (⚔️, 🔥, etc.)
}
// Only show active threats (no empty slots)
// If entity doesn't have queue: skip section
```

**Visibility Rules:**
```rust
// Auto-show when target exists
pub fn should_show_target_frame(
    target: Option<&Target>,
) -> bool {
    target.is_some()  // Show when any target selected
}
```

**Scaling by Entity Type:**
```rust
match entity_type {
    EntityType::BasicEnemy => {
        // Show: Name, distance, HP only
        // Skip: Resources, queue
    },
    EntityType::EliteEnemy | EntityType::Player => {
        // Show: Everything (name, distance, HP, resources, queue)
    },
    EntityType::Boss => {
        // Show: Everything + future phase indicators
    },
}
```

**Component Structure:**
```rust
// src/client/components/target_frame.rs
#[derive(Component)]
pub struct TargetDetailFrame {
    pub target_entity: Entity,
    pub hp_bar: Entity,
    pub stamina_bar: Option<Entity>,
    pub mana_bar: Option<Entity>,
    pub queue_display: Option<Entity>,
}
```

**Update System:**
```rust
// src/client/systems/ui/target_frame.rs
pub fn update_target_frame(
    mut commands: Commands,
    player_query: Query<&Target, With<Controlled>>,
    target_query: Query<(
        &EntityType,
        &Health,
        Option<&Stamina>,
        Option<&Mana>,
        Option<&ReactionQueue>,
        &Loc,
    )>,
    mut frame_query: Query<&mut TargetDetailFrame>,
    player_loc_query: Query<&Loc, With<Controlled>>,
) {
    // 1. Check if player has target
    // 2. If target exists and frame doesn't: spawn frame
    // 3. If target exists: update frame contents
    //    - Calculate distance (hex count between player and target)
    //    - Update HP bar fill
    //    - Update resource bars (if present)
    //    - Update queue display (if present)
    // 4. If no target: despawn frame
}
```

**Why this design:**
- Top-right = conventional target frame location (genre standard)
- Compact = doesn't block center action
- Exact numbers = tactical precision ("58/100 = almost dead")
- Enemy queue visible = enables queue manipulation tactics
- Enemy resources visible = "can they afford dodge?" decision-making
- Auto-show/hide = no extra input required (frictionless)

---

#### Decision 8: Facing Indicator (Character + Arrow)

**Problem:** After movement stops, players may not know which direction they're facing. Directional targeting depends on 60° facing cone, so heading clarity is critical.

**Solution:** Combine character position offset + directional arrow

**Visual Components:**

**1. Character Position Offset (Already in Spec):**
```rust
// Character positioned on hex to indicate facing
// (not centered if facing a direction)
let offset_distance = 0.15;  // 15% of hex radius toward facing direction
let facing_offset = Vec3::new(
    heading.direction.x * offset_distance,
    heading.direction.y * offset_distance,
    0.0,
);
character_transform.translation = hex_center + facing_offset;
```

**2. Directional Arrow (New):**
```rust
// Small arrow sprite attached to character
sprite: Arrow sprite (or simple triangle)
size: 0.3 world units (small, subtle)
color: Color::rgba(1.0, 1.0, 1.0, 0.7)  // White, semi-transparent
position: character.translation + Vec3::new(0.0, 0.8, 0.1)  // Above character
rotation: heading.angle  // Points in facing direction
```

**Optional: Facing Cone Overlay (Toggleable):**
```rust
// Faint 60° cone on ground (optional feature)
cone_color: Color::rgba(0.5, 0.5, 1.0, 0.2)  // Blue, very faint
cone_angle: 60° (from heading)
cone_length: 3.0 world units (shows targeting area)
toggle_key: TAB (hold to show, release to hide)
// OR: Always on during combat, off outside combat
```

**Component Structure:**
```rust
// src/client/components/facing_indicator.rs
#[derive(Component)]
pub struct FacingArrow {
    pub parent_entity: Entity,  // Character entity
}

#[derive(Component)]
pub struct FacingConeOverlay {
    pub parent_entity: Entity,
    pub visible: bool,  // Toggleable
}
```

**Update System:**
```rust
// src/client/systems/ui/facing_indicator.rs
pub fn update_facing_indicator(
    query: Query<(Entity, &Heading, &Transform), With<Controlled>>,
    mut arrow_query: Query<(&FacingArrow, &mut Transform)>,
    mut cone_query: Query<(&FacingConeOverlay, &mut Visibility)>,
    keyboard: Res<Input<KeyCode>>,
) {
    for (entity, heading, char_transform) in &query {
        // Update arrow rotation + position
        if let Ok((arrow, mut arrow_transform)) = arrow_query.get_mut(entity) {
            arrow_transform.rotation = Quat::from_rotation_z(heading.angle);
            arrow_transform.translation = char_transform.translation + Vec3::new(0.0, 0.8, 0.1);
        }

        // Update cone visibility (optional)
        if let Ok((cone, mut visibility)) = cone_query.get_mut(entity) {
            *visibility = if keyboard.pressed(KeyCode::Tab) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}
```

**Why this design:**
- Position offset + arrow = dual reinforcement (subtle + clear)
- Arrow always visible = no guessing after movement stops
- Optional cone overlay = tactical players can see exact targeting area
- Toggleable = doesn't clutter screen for players who don't want it

---

## Consequences

### Positive

#### 1. Complete Combat UI Coverage

- Resource bars → always know HP/Stamina/Mana
- **Action bar → see abilities, costs, cooldowns, ready states**
- Reaction queue → see incoming threats
- Target indicators → know what you're attacking
- **Target detail frame → see enemy HP, resources, queue state**
- Health bars → see enemy HP (approximate)
- **Facing indicator → know heading direction**
- Combat state → know when in combat

Fulfills **all Phase 1 Critical features** from combat-hud.md spec.

#### 2. Clear Visual Hierarchy

- Circular = urgent (threats, timers)
- Rectangular = stable (resources, abilities future)
- World-space = contextual (health, targets)
- Screen-space = player state (resources, queue)

Instant visual differentiation prevents confusion.

#### 3. Minimal Performance Overhead

- Resource bars: 3 sprites + 3 text labels (low cost)
- Reaction queue: Max 3 circles × 4 elements = 12 UI elements
- Target indicators: 1-2 sprites (only active targets)
- Health bars: N entities × 2 sprites (only visible in combat/damaged)

Total: ~30-50 UI elements in typical combat (negligible for Bevy).

#### 4. Tactical Decision-Making Enabled

**Action bar answers:**
- "What abilities do I have?" → See Q/W/E/R slots
- "Can I afford this ability?" → See resource costs
- "Is this ability ready?" → See cooldown state

**Target frame answers:**
- "Can they dodge?" → See their stamina
- "Is their queue full?" → See threat indicators
- "Are they almost dead?" → See exact HP numbers

**Enables core combat tactics:**
- Queue manipulation (add threat when enemy queue full)
- Resource pressure (attack when enemy low on stamina)
- Tactical timing (attack when enemy distracted by threats)

#### 5. Extensible for Future Features

**Status effects** (future):
- Small icons above health bars
- Same circular style as threat indicators

**Minimap** (future):
- Top-left corner (doesn't conflict with target frame)

**Party frames** (future):
- Left side of screen
- Same resource bar style as player

#### 6. Consistent with Genre Conventions

- Red circle on target (RTS/MOBA standard)
- Health bars above entities (MMO standard)
- Bottom-center resources (action game standard)
- **Action bar below resources (MMO standard)**
- Circular timers (MOBA standard)
- **Target frame top-right (MMO standard)**

Reduces learning curve for players.

### Negative

#### 1. UI Entity Overhead

- Health bars on 20 NPCs = 40 sprites (background + foreground)
- Spawning/despawning entities every frame (visibility toggle)
- Hierarchical transforms (parented to entities)

**Mitigation:**
- Only show when in combat/damaged (reduces count)
- Object pooling (reuse despawned entities)
- Batch spawning (spawn all bars in single system run)

#### 2. Text Rendering Performance

- Resource bar labels: "120 / 150" × 3 = dynamic text
- Countdown timers: "0.8s" × 3 = updating every frame
- Text rendering is expensive in Bevy

**Mitigation:**
- Use bitmap fonts (faster than TTF)
- Update text only when value changes (dirty checking)
- Optional: Hide countdown text (show timer ring only)

#### 3. World-Space UI Scaling Issues

- Health bars/target indicators scale with camera zoom
- May be too small when zoomed out
- May be too large when zoomed in

**Mitigation:**
- Use orthographic camera (consistent scaling)
- Clamp scale based on camera zoom
- Test at min/max zoom levels

#### 4. Color Accessibility

- Red/green target indicators (colorblind issue)
- Red health bars (deuteranopia)
- Orange reaction queue borders (protanopia)

**Mitigation:**
- Combine color + shape (circles for targets)
- Combine color + icons (⚔️ for threats)
- Future: colorblind mode (change colors, keep shapes/icons)

#### 5. UI Clutter with Many Entities

- 20 NPCs = 20 health bars = visual noise
- 10 players = 10 target indicators = confusing
- Hard to distinguish entities

**Mitigation:**
- Only show health bars in combat/damaged (reduces count)
- Only show 1 hostile + 1 ally indicator (clear)
- Future: camera culling (don't render off-screen UI)

### Neutral

#### 1. UI Framework Choice

- MVP uses Bevy UI (built-in, simple)
- May migrate to bevy_egui for complex UI later (ability tooltips, menus)
- Current choice sufficient for MVP

**Consideration:** Bevy UI has limitations (no rich text, limited layout options).

#### 2. Animation Complexity

- Timer ring depletion (smooth circular progress)
- Pulsing borders (sin wave interpolation)
- Health bar interpolation (smooth fill changes)

May require custom rendering or shader effects.

**Consideration:** Start simple (no animations), add polish in Phase 2.

---

## Implementation Phases

### Phase 1: Resource Bars (Foundation)

**Goal:** Display Health/Stamina/Mana bars at bottom-center

**Tasks:**
1. Create `client/systems/ui/resource_bars.rs`:
   - System: `setup_resource_bars` (spawns UI entities)
   - System: `update_resource_bars` (updates fill width based on Health/Stamina/Mana)
   - Component: `ResourceBarContainer`, `HealthBarUI`, `StaminaBarUI`, `ManaBarUI`

2. UI entity structure:
   - Container node (bottom-center, flex row)
   - Each bar: Background sprite + Foreground sprite + Text label
   - Use Bevy UI `Style` for positioning

3. Update logic:
   - Query local player's Health/Stamina/Mana components
   - Calculate fill percentage: `current / max`
   - Update foreground sprite width (scale or clip)
   - Update text label: `"{current:.0} / {max:.0}"`

**Success Criteria:**
- Resource bars visible at bottom-center
- Bars update when Health/Stamina/Mana change
- Text shows current/max values
- Colors correct (Red/Yellow/Blue)

**Duration:** 2 days

---

### Phase 2: Action Bar Display (Critical Addition)

**Goal:** Display Q/W/E/R abilities with costs, cooldowns, ready states

**Tasks:**
1. Create `client/systems/ui/action_bar.rs`:
   - System: `setup_action_bar` (spawns 4 ability slot UI entities)
   - System: `update_action_bar` (syncs with player abilities, GCD, resources)
   - Component: `ActionBarDisplay`, `AbilitySlotUI`

2. UI entity structure:
   - Container node (bottom-center, below resource bars, flex row)
   - 4 rectangular slots (Q, W, E, R):
     - Keybind label (always visible)
     - Ability icon (center)
     - Resource cost badge (bottom-right)
     - Cooldown overlay (radial sweep)
     - State-indicating border (green/gray/red/orange/yellow)

3. Ability synchronization:
   - Query player's equipped abilities (future: `EquippedAbilities` component)
   - MVP: Hardcoded abilities (Q=BasicAttack, E=Dodge, W/R=Empty)
   - Check GCD state (Gcd component)
   - Check resource sufficiency (Stamina/Mana vs ability cost)
   - Check target requirement (Target component)
   - Update border color based on state

4. Cooldown visualization:
   - Radial sweep overlay (clockwise, 0° to progress * 360°)
   - Optional countdown text (0.5s remaining)
   - Gray out icon when on cooldown

**Success Criteria:**
- 4 ability slots visible at bottom-center (below resources)
- Keybinds labeled (Q, W, E, R)
- MVP: Q shows BasicAttack (free), E shows Dodge (30 stamina), W/R empty
- Border changes color based on state (green=ready, gray=cooldown, red=can't afford)
- GCD overlay appears when GCD active (0.5s)

**Duration:** 2-3 days

---

### Phase 3: Reaction Queue Display

**Goal:** Visualize incoming threats at top-center

**Tasks:**
1. Create `client/systems/ui/reaction_queue.rs`:
   - System: `setup_reaction_queue_ui` (spawns 3 circle slots)
   - System: `update_reaction_queue_ui` (syncs with ReactionQueue component)
   - Component: `ReactionQueueDisplay`, `ThreatCircleUI`

2. UI entity structure:
   - Container node (top-center, flex row)
   - 3 circular slots (background + border + icon + timer ring + text)
   - Use bevy_prototype_lyon or custom circle mesh

3. Timer ring rendering:
   - Custom mesh (circle arc) or sprite sheet
   - Update arc angle based on `time_remaining / timer_duration`
   - Clockwise depletion (start at top, sweep right)

4. Threat synchronization:
   - Client receives `Do::InsertThreat` → add to UI queue
   - Client runs timer countdown (local interpolation)
   - Client receives `Do::ResolveThreat` (or clears via Dodge) → remove from UI

**Success Criteria:**
- 3 circular slots visible at top-center
- Threats appear when inserted (icon + timer ring)
- Timer rings deplete smoothly
- Empty slots shown (faint gray)

**Duration:** 3-4 days

---

### Phase 4: Target Indicators (World-Space)

**Goal:** Red circle on hostile target, green circle on ally target

**Tasks:**
1. Create `client/systems/ui/target_indicators.rs`:
   - System: `update_target_indicators` (spawns/despawns indicators)
   - Component: `TargetIndicator { target_type: TargetType }`

2. Indicator rendering:
   - Spawn 2D circle sprite (or ring sprite asset)
   - Parent to target entity (Transform hierarchy)
   - Position: entity.translation + offset (below entity)
   - Color: Red (hostile) or Green (ally)

3. Targeting integration:
   - Query local player's Target component (from ADR-004)
   - Spawn indicator on Target.hostile entity
   - Spawn indicator on Target.ally entity (if exists)
   - Despawn indicators when targets change

**Success Criteria:**
- Red circle appears on hostile target
- Green circle appears on ally target (if targeted)
- Indicators follow entity movement (interpolated)
- Indicators update when player changes facing/target

**Duration:** 2 days

---

### Phase 5: Target Detail Frame (Critical Addition)

**Goal:** Show detailed enemy state (HP, resources, queue) in top-right frame

**Tasks:**
1. Create `client/systems/ui/target_frame.rs`:
   - System: `update_target_frame` (spawns/updates/despawns frame)
   - Component: `TargetDetailFrame`

2. Frame structure:
   - Container (top-right, 280px × variable height)
   - Entity name + distance header
   - HP bar with exact numbers (orange fill)
   - Resource bars (stamina/mana) if entity has them
   - Threat queue display (mini version) if entity has queue

3. Target synchronization:
   - Query player's Target component (from ADR-004)
   - If target exists: spawn/update frame
   - If no target: despawn frame
   - Calculate distance (hex count between player and target)

4. Content scaling by entity type:
   - Basic enemies (Wild Dog): Show name, distance, HP only
   - Elite enemies/players: Show everything (HP, resources, queue)
   - Future: Boss enemies with phase indicators

5. Enemy queue visualization:
   - Smaller circular threat indicators (60% of player queue size)
   - Same timer rings, attack icons
   - Label: "QUEUE:" for clarity
   - Only show active threats (no empty slots)

**Success Criteria:**
- Frame appears at top-right when target selected
- Frame disappears when no target
- Shows exact HP numbers (80/100)
- Shows distance in hexes (3h)
- MVP: Wild Dog shows name + HP only (no resources/queue yet)
- Future: Elite enemies show resources + queue

**Duration:** 2-3 days

---

### Phase 6: World-Space Health Bars

**Goal:** Health bars above all entities in combat

**Tasks:**
1. Create `client/systems/ui/health_bars.rs`:
   - System: `update_health_bars` (spawns/despawns/updates bars)
   - Component: `HealthBar { background, foreground }`

2. Bar rendering:
   - Spawn 2 sprites (background gray, foreground red)
   - Parent to entity (Transform hierarchy)
   - Position: entity.translation + Vec3::Y * 1.5 (above entity)
   - Scale foreground.x based on health percentage

3. Visibility logic:
   - Always show for players
   - Show for NPCs if in combat OR damaged
   - Hide for NPCs if out of combat AND full HP

4. Update logic:
   - Query all entities with Health component
   - Update foreground scale: `(health.step / health.max()) * bar_width`
   - Interpolate scale changes (smooth animation)

**Success Criteria:**
- Health bars appear above entities in combat
- Bars update smoothly when damage taken
- Bars hide when out of combat (NPCs only)
- Player health bar always visible

**Duration:** 2-3 days

---

### Phase 7: Combat State Feedback + Facing Indicator

**Goal:** Visual indication of in-combat state + heading direction

**Tasks:**
1. Create `client/systems/ui/combat_state.rs`:
   - System: `update_combat_state_ui` (applies vignette/glow)
   - Component: `CombatVignette`, `CombatGlow`

2. Vignette effect:
   - Spawn full-screen overlay (NodeBundle with BackgroundColor)
   - Radial gradient (dark red edges, transparent center)
   - Opacity based on combat state (0.0 out of combat, 0.15 in combat)
   - Interpolate opacity (smooth fade in/out)

3. Resource bar glow:
   - Add outline/border to resource bar container when in combat
   - Orange glow (2px border, semi-transparent)
   - Toggle on combat state change

4. **Facing indicator** (High Priority addition):
   - Create `client/systems/ui/facing_indicator.rs`
   - Spawn arrow sprite above character (0.3 world units size)
   - Update arrow rotation to match Heading component
   - Optional: Facing cone overlay (toggle with TAB)
   - Character position offset toward heading (existing spec feature)

**Success Criteria:**
- Screen edges darken subtly when in combat
- Resource bars glow orange when in combat
- Effects fade smoothly when entering/exiting combat
- **Arrow appears above character pointing in facing direction**
- **Character positioned on hex to indicate facing (offset from center)**
- Optional: TAB shows 60° facing cone overlay

**Duration:** 2-3 days

---

### Phase 8: UI Polish and Optimization

**Goal:** Smooth animations, performance optimization

**Tasks:**
1. Timer ring animation:
   - Smooth circular progress (60fps, no jitter)
   - Pulsing border when < 20% time remaining
   - Audio tick when urgent (optional, toggleable)

2. Health bar interpolation:
   - Smooth fill changes (lerp over 0.2s)
   - No snapping (jarring visual)
   - Use `step` interpolation pattern (like Offset)

3. Object pooling:
   - Reuse despawned health bar entities
   - Pool size: 50 bars (covers 25 NPCs)
   - Reduces entity spawning overhead

4. Text update optimization:
   - Only update text when value changes (dirty checking)
   - Cache formatted strings ("120 / 150")
   - Reduce string allocation

5. Camera culling:
   - Don't render health bars for off-screen entities
   - Check entity position vs camera frustum
   - Significant savings with many NPCs

**Success Criteria:**
- Timer rings deplete smoothly (no stuttering)
- Health bars interpolate (no snapping)
- 100 NPCs in combat < 10% CPU (UI systems)

**Duration:** 2-3 days

---

## Validation Criteria

### Functional Tests

- **Resource Bars:** Player takes damage → Health bar decreases
- **Reaction Queue:** Threat inserted → circle appears with timer
- **Target Indicators:** Player faces enemy → red circle appears
- **Health Bars:** NPC enters combat → health bar appears
- **Combat State:** Player attacks → vignette appears

### Visual Tests

- **Clarity:** Can identify combat state at a glance (< 1 second)
- **Readability:** Resource values readable from normal playing distance
- **Urgency:** Timer urgency (< 20%) visually distinct from normal
- **Consistency:** Colors match conventions (Red=Health, Yellow=Stamina, Blue=Mana)

### Performance Tests

- **UI Overhead:** 20 NPCs in combat → 60fps maintained
- **Text Updates:** Resource bar text updates < 1ms per frame
- **Health Bar Spawning:** 50 NPCs enter combat → no frame drops

### UX Tests

- **Resource Awareness:** Player knows current HP/Stamina/Mana without looking away from character
- **Threat Awareness:** Player sees incoming threats before timers expire
- **Target Clarity:** Player knows which entity they're attacking

---

## Open Questions

### Design Questions

1. **Timer Countdown Text?**
   - Show numeric "0.8s" below timer ring?
   - MVP: Show text (precision useful), make toggleable later

2. **Health Bar Size?**
   - 1.0 world units too large? Too small?
   - MVP: 1.0 units, adjust based on playtest feedback

3. **Vignette Intensity?**
   - 0.15 opacity too subtle? Too intrusive?
   - MVP: 0.15, make configurable in settings

### Technical Questions

1. **Timer Ring Rendering?**
   - Use bevy_prototype_lyon (vector shapes)?
   - Use sprite sheet (pre-rendered frames)?
   - Use custom mesh (arc primitive)?
   - MVP: Sprite sheet (simplest), migrate to lyon if needed

2. **UI Framework Migration?**
   - When to migrate to bevy_egui?
   - MVP uses Bevy UI (sufficient for simple bars/circles)
   - Migrate when need rich text, tooltips, complex layouts

3. **Health Bar Pooling Strategy?**
   - Pre-spawn pool entities on startup?
   - Lazy spawn (create on demand, cache)?
   - MVP: Lazy spawn, optimize if profiling shows issue

---

## Future Enhancements (Out of Scope)

### Phase 2+ Extensions

- **Ability Bar:** Display Q/W/E/R abilities with cooldowns
- **Status Effects:** Icons above health bars (buffs/debuffs)
- **Minimap:** Top-right corner, shows nearby entities
- **Combat Log:** Scrolling text feed of events
- **Damage Meters:** DPS tracking, threat meters

### Accessibility

- **Colorblind Mode:** Alternative color schemes
- **UI Scaling:** Increase/decrease UI size
- **Audio Cues:** Sounds for new threats, low HP

### Optimization

- **GPU Instancing:** Batch health bar rendering
- **LOD System:** Simplify distant UI elements
- **Sprite Atlases:** Reduce texture binds

---

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (MVP UI requirements, reaction queue display)
- **Combat HUD:** `docs/spec/combat-hud.md` (Phase 1 Critical features - AUTHORITATIVE for MVP scope)

### Codebase

- **Health Component:** ADR-002 (`Health.state/step`)
- **ReactionQueue Component:** ADR-003 (`QueuedThreat`, timers)
- **Target Component:** ADR-004 (directional targeting)
- **Damage Numbers:** ADR-005 (floating text system)

### Related ADRs

- **ADR-002:** Combat Foundation (Health/Stamina/Mana components)
- **ADR-003:** Reaction Queue System (threat queueing, timer management)
- **ADR-004:** Ability System and Directional Targeting (Target component)
- **ADR-005:** Damage Pipeline (damage numbers, floating text)

---

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md`
- Visual design reference: `docs/spec/combat-hud.md`

## Date

2025-10-31

---

## Summary for Developers

**What this ADR adds:**

1. **Resource Bars** (bottom-center): Health/Stamina/Mana visualization
2. **Action Bar** (bottom-center, below resources): Q/W/E/R abilities with costs, cooldowns, states
3. **Reaction Queue Display** (top-center): Circular threat indicators with timer rings
4. **Target Indicators** (world-space): Red circle on hostile, green circle on ally
5. **Target Detail Frame** (top-right): Enemy HP, resources, queue state (exact values)
6. **Health Bars** (world-space): Above all entities in combat (approximate HP)
7. **Facing Indicator** (character): Arrow + position offset showing heading
8. **Combat State Feedback** (screen-space): Vignette and resource bar glow

**Integration Points:**

- Queries Health/Stamina/Mana components (ADR-002)
- Queries Gcd component for cooldown overlay (ADR-002)
- Syncs with ReactionQueue component (ADR-003)
- Uses Target component (ADR-004)
- Uses Heading component for facing indicator (ADR-004)
- Complements damage numbers (ADR-005)

**Testing Priority:**

1. Resource bars update correctly (Health/Stamina/Mana)
2. **Action bar shows abilities with correct states** (ready/cooldown/can't afford)
3. Reaction queue shows threats with timers
4. Target indicators appear on correct entities
5. **Target frame shows enemy state** (HP/resources/queue when applicable)
6. Health bars visible in combat, hidden out of combat
7. **Facing indicator tracks heading** (arrow + character offset)
8. Combat state feedback smooth (vignette/glow)

**Revised Timeline:**

- Phase 1: Resource Bars (2 days)
- Phase 2: Action Bar (2-3 days) ← **NEW**
- Phase 3: Reaction Queue (3-4 days)
- Phase 4: Target Indicators (2 days)
- Phase 5: Target Detail Frame (2-3 days) ← **NEW**
- Phase 6: Health Bars (2-3 days)
- Phase 7: Combat State + Facing (2-3 days) ← **Modified**
- Phase 8: Polish (2-3 days)

**Total: 17-23 days** (was 12-16 days, +5-7 days for critical features)
