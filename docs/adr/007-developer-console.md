# ADR-007: Contextual Developer Console

## Status

Accepted

## Context

### Current Debug Capabilities (Scattered)

The codebase has robust debug and diagnostic capabilities, but they are **scattered across multiple keybindings** with no discoverability or organization:

**Performance Monitoring:**
- **F3**: Toggle performance UI (FPS, frame time, entity count, render stats, terrain tiles)

**Visual Debug Tools:**
- **J**: Toggle hex grid overlay (tile boundaries wireframe)
- **V**: Toggle spawner visualization (cyan cylinder markers)

**Rendering Toggles:**
- **H**: Toggle slope rendering (flat vs sloped terrain meshes)
- **G**: Toggle fixed lighting (locked 9 AM vs dynamic day/night cycle)

**Test/Debug Systems:**
- **Digit1** (top row): Queue 20 damage threat (reaction queue test)
- **Digit2** (top row): Drain 30 stamina directly
- **Digit3** (top row): Drain 25 mana directly

### Problems with Current Architecture

#### 1. Discoverability

- **No way to see available debug commands** without reading source code
- New developers cannot discover features
- Easy to forget keybindings between sessions

#### 2. Key Conflict Risk

- Debug keys spread across keyboard (J, H, G, V, F3, Digit1-3)
- May conflict with gameplay keys as game expands
- Hard to remember which key does what

#### 3. No Context or Organization

- Features not grouped logically
- Cannot see current state of toggles (is slope rendering on?)
- No way to know if a feature is available in current mode

#### 4. Testing Workflows Inefficient

- Combat testing requires remembering Digit1/2/3 keys
- Terrain debugging requires toggling J+H keys separately
- Performance analysis requires F3+other features

### Design Goals for Developer Console

#### 1. Discoverability
**Goal:** Developer can find any debug feature without reading docs

**Requirements:**
- Visual menu showing all available commands
- Clear labels describing what each option does
- Current state visible for toggles (ON/OFF)

#### 2. Contextual Navigation
**Goal:** Organize features into logical sections

**Requirements:**
- Terrain section (grid, slope rendering, lighting)
- Combat section (resource drains, reaction queue test)
- Performance section (FPS, entity count, profiling)
- Visualization section (spawner markers, character panel)
- Tools section (camera zoom, teleport, spawn NPCs - future)

#### 3. Numpad-Based Navigation
**Goal:** Fully navigable with one hand (numpad only)

**Requirements:**
- **NumpadDivide (/)**: Open/close console
- **Numpad 0-9**: Select menu options
- No mouse required
- No conflicts with gameplay keys (gameplay uses arrows + Q/Space)

#### 4. Minimize Gameplay Interruption
**Goal:** Console is overlay, doesn't pause game

**Requirements:**
- Semi-transparent overlay panel
- Console visible while gameplay continues
- Quick toggle (single key press)
- Breadcrumb navigation (see current menu path)

---

## Decision

We will implement a **hierarchical, numpad-navigable developer console** that consolidates all debug capabilities into a contextual menu system.

### Architecture Decisions

#### Decision 1: Resource-Based State Machine

**Console State:**
```rust
#[derive(Resource)]
pub struct DevConsole {
    pub visible: bool,
    pub current_menu: MenuPath,
    pub history: Vec<MenuPath>, // Breadcrumb navigation
}

#[derive(Clone, PartialEq, Eq)]
pub enum MenuPath {
    Root,
    Terrain,
    Combat,
    Performance,
    Visualization,
    Tools,
}
```

**Why Resource (not component):**
- Single global console (not per-entity)
- Accessed by many systems (toggle handlers)
- Simpler than singleton entity query

---

#### Decision 2: Menu Hierarchy

**Root Menu** (`NumpadDivide` to open):
```
Developer Console
─────────────────
1. Terrain      [Slope: ON, Grid: OFF, Light: Fixed]
2. Combat       [Test tools and cheats]
3. Performance  [FPS: ON, Profiling: OFF]
4. Visualization[Spawners: ON, Character: OFF]
5. Tools        [Camera, Spawn, Teleport]

0. Close Console
```

**Terrain Menu** (press `1` from root):
```
Terrain Settings
─────────────────
1. Toggle Grid Overlay      [OFF]
2. Toggle Slope Rendering   [ON]
3. Toggle Fixed Lighting    [Fixed 9AM]
4. Regenerate Mesh          [Action]

0. Back to Main Menu
```

**Combat Menu** (press `2` from root):
```
Combat Testing
─────────────────
1. Queue 20 Damage Threat   [Action]
2. Drain 30 Stamina         [Action]
3. Drain 25 Mana            [Action]
4. Clear Reaction Queue     [Action]
5. Refill Resources         [Action]

0. Back to Main Menu
```

**Performance Menu** (press `3` from root):
```
Performance Monitoring
─────────────────
1. Toggle Performance UI    [ON]
2. Toggle FPS Counter       [ON]
3. Toggle Detailed Stats    [OFF]
4. Log Frame Report         [Action]

0. Back to Main Menu
```

**Visualization Menu** (press `4` from root):
```
Debug Visualizations
─────────────────
1. Toggle Spawner Markers   [ON]
2. Toggle Collision Boxes   [OFF - Future]
3. Toggle Pathfinding       [OFF - Future]
4. Toggle Target Indicators [ON - Future]

0. Back to Main Menu
```

**Tools Menu** (press `5` from root):
```
Developer Tools
─────────────────
1. Teleport to Cursor       [Action - Future]
2. Spawn NPC at Cursor      [Action - Future]
3. Clear All Entities       [Action - Future]
4. Place Test Spawner       [Action - Future]

0. Back to Main Menu
```

---

#### Decision 3: UI Rendering (Bevy UI)

**Panel Structure:**
```rust
pub fn setup_dev_console(
    mut commands: Commands,
) {
    commands.spawn((
        DevConsoleRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(20.0),
            top: Val::Px(100.0),
            width: Val::Px(400.0),
            padding: UiRect::all(Val::Px(15.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)), // Semi-transparent black
        BorderColor(Color::srgb(0.3, 0.6, 0.9)), // Blue border
        BorderRadius::all(Val::Px(8.0)),
        Visibility::Hidden, // Start hidden
        ZIndex(1000), // Render above gameplay UI
    ))
    .with_children(|parent| {
        // Title section
        parent.spawn((
            Text::new("Developer Console"),
            TextFont { font_size: 20.0, ..default() },
            TextColor(Color::srgb(0.3, 0.6, 0.9)), // Blue title
        ));

        // Breadcrumb section (shows current menu path)
        parent.spawn((
            BreadcrumbText,
            Text::new("Main Menu"),
            TextFont { font_size: 14.0, ..default() },
            TextColor(Color::srgb(0.7, 0.7, 0.7)), // Gray breadcrumb
        ));

        // Menu items container (rebuilt dynamically based on current menu)
        parent.spawn((
            MenuItemsContainer,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
        ));
    });
}
```

**Menu Item Format:**
```rust
// Toggle item (shows current state)
"1. Toggle Grid Overlay      [OFF]"
"2. Toggle Slope Rendering   [ON]"

// Action item (no state)
"1. Queue 20 Damage Threat   [Action]"

// Sub-menu (shows summary)
"1. Terrain      [Slope: ON, Grid: OFF, Light: Fixed]"
```

**Visual Design:**
- **Position**: Top-left corner (doesn't obscure center gameplay)
- **Size**: 400px wide, dynamic height based on menu items
- **Colors**:
  - Background: Dark gray (90% opacity)
  - Border: Blue accent
  - Title: Blue
  - Menu items: White text, gray background on hover
  - State indicators: Green [ON], Red [OFF], Yellow [Action]
- **Typography**: Monospace font (aligned columns)

---

#### Decision 4: Navigation System

**Input Handling:**
```rust
pub fn handle_console_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut console: ResMut<DevConsole>,
    state: Res<DiagnosticsState>, // Existing state
    mut writer: EventWriter<DevConsoleAction>,
) {
    // Toggle console visibility
    if keyboard.just_pressed(KeyCode::NumpadDivide) {
        console.visible = !console.visible;
        return;
    }

    // Only process menu navigation if console is open
    if !console.visible {
        return;
    }

    // Numpad 0: Back/Close
    if keyboard.just_pressed(KeyCode::Numpad0) {
        if console.current_menu == MenuPath::Root {
            console.visible = false; // Close console from root
        } else {
            console.current_menu = console.history.pop().unwrap_or(MenuPath::Root);
        }
    }

    // Numpad 1-9: Select menu item
    match console.current_menu {
        MenuPath::Root => {
            if keyboard.just_pressed(KeyCode::Numpad1) {
                console.history.push(MenuPath::Root);
                console.current_menu = MenuPath::Terrain;
            }
            // ... handle 2-9 for other root menu items
        }
        MenuPath::Terrain => {
            if keyboard.just_pressed(KeyCode::Numpad1) {
                writer.send(DevConsoleAction::ToggleGrid);
            }
            if keyboard.just_pressed(KeyCode::Numpad2) {
                writer.send(DevConsoleAction::ToggleSlopeRendering);
            }
            // ... handle other terrain menu items
        }
        // ... handle other menus
    }
}
```

**Action Event System:**
```rust
#[derive(Event)]
pub enum DevConsoleAction {
    // Terrain actions
    ToggleGrid,
    ToggleSlopeRendering,
    ToggleFixedLighting,
    RegenerateMesh,

    // Combat actions
    QueueDamageThreat,
    DrainStamina,
    DrainMana,
    ClearReactionQueue,
    RefillResources,

    // Performance actions
    TogglePerfUI,
    ToggleFPSCounter,
    ToggleDetailedStats,
    LogFrameReport,

    // Visualization actions
    ToggleSpawnerMarkers,
    ToggleCollisionBoxes,
    TogglePathfinding,
    ToggleTargetIndicators,

    // Tools actions
    TeleportToCursor,
    SpawnNPCAtCursor,
    ClearAllEntities,
    PlaceTestSpawner,
}
```

**Why Event-Based:**
- Decouples input handling from action execution
- Actions can be triggered from other sources (future: text commands, UI buttons)
- Easier to test (emit events, check results)

---

#### Decision 5: Integration with Existing Systems

**No Changes to Existing Keybindings (Initially):**

Existing systems remain functional:
- **J/H/G/V/F3/Digit1-3** keys still work as before
- Console provides **alternative access** to same features
- Allows gradual migration (test console without breaking existing workflows)

**Phase 2 (Optional): Deprecate Direct Keybindings:**

After console is proven stable:
- Remove direct keybindings (J/H/G/V/Digit1-3)
- Force all debug access through console
- Reduces key conflict risk

**Integration Points:**
```rust
// Existing: Direct toggle system
pub fn toggle_grid_visibility(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<DiagnosticsConfig>,
    mut state: ResMut<DiagnosticsState>,
    // ...
) {
    if keyboard.just_pressed(config.grid_toggle_key) { // Still works!
        state.grid_visible = !state.grid_visible;
        // ...
    }
}

// New: Console-triggered action
pub fn execute_console_actions(
    mut reader: EventReader<DevConsoleAction>,
    mut state: ResMut<DiagnosticsState>,
    // ... other params
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::ToggleGrid => {
                state.grid_visible = !state.grid_visible; // Same logic!
                // ... trigger regeneration, etc.
            }
            // ... other actions
        }
    }
}
```

**Shared State:**
- Console reads `DiagnosticsState` to show current toggle states
- Console actions modify same resources as existing systems
- No duplication of state

---

#### Decision 6: Dynamic Menu Rendering

**Menu Rebuilding:**
```rust
pub fn update_console_menu(
    console: Res<DevConsole>,
    state: Res<DiagnosticsState>,
    spawner_viz: Res<SpawnerVizState>,
    char_panel: Res<CharacterPanelState>,
    mut menu_query: Query<Entity, With<MenuItemsContainer>>,
    mut commands: Commands,
) {
    // Only update if console visible and menu changed
    if !console.visible || !console.is_changed() {
        return;
    }

    let Ok(container_entity) = menu_query.single_mut() else { return };

    // Despawn old menu items
    commands.entity(container_entity).despawn_descendants();

    // Rebuild menu based on current path
    commands.entity(container_entity).with_children(|parent| {
        match console.current_menu {
            MenuPath::Root => build_root_menu(parent, &state, &spawner_viz, &char_panel),
            MenuPath::Terrain => build_terrain_menu(parent, &state),
            MenuPath::Combat => build_combat_menu(parent),
            MenuPath::Performance => build_performance_menu(parent, &state),
            MenuPath::Visualization => build_visualization_menu(parent, &spawner_viz, &char_panel),
            MenuPath::Tools => build_tools_menu(parent),
        }
    });
}

fn build_terrain_menu(parent: &mut ChildBuilder, state: &DiagnosticsState) {
    parent.spawn((
        Text::new(format!("1. Toggle Grid Overlay      [{}]",
            if state.grid_visible { "ON" } else { "OFF" })),
        TextFont { font_size: 16.0, ..default() },
        TextColor(if state.grid_visible {
            Color::srgb(0.2, 0.8, 0.2) // Green if ON
        } else {
            Color::srgb(0.8, 0.2, 0.2) // Red if OFF
        }),
    ));
    // ... other menu items
}
```

**Menu Updates:**
- Rebuilt whenever `console.current_menu` changes (navigation)
- Rebuilt whenever state resources change (toggles update)
- Efficient: Only despawn/respawn text entities (minimal overhead)

---

#### Decision 7: MVP Scope vs Future Extensions

**MVP (Phase 1):**
- Console UI panel (Bevy UI overlay)
- Root menu + 5 sub-menus (Terrain, Combat, Performance, Visualization, Tools)
- All existing debug features accessible via console
- Numpad navigation (0-9, /)
- Breadcrumb navigation
- Current state indicators for toggles

**Phase 2 (Post-MVP):**
- Text command input (type commands instead of menu navigation)
  - Example: `/toggle grid`, `/drain stamina 50`, `/spawn dog 10`
- Command history (up/down arrows to recall)
- Command autocomplete (tab completion)
- Console log (display output from commands)
- Keybinding customization (rebind numpad keys)

**Phase 3 (Advanced):**
- Save/load console state (persist favorite commands)
- Macros (bind multiple commands to single key)
- Scripting (Lua/Rhai for complex test scenarios)
- Remote console (connect from external tool for automated testing)

**Explicitly Out of Scope (MVP):**
- Text input (Phase 2)
- Mouse interaction (numpad-only for MVP)
- Customizable keybindings (Phase 2)
- Gamepad support (not needed for dev tool)
- Console persistence (Phase 3)

---

## Consequences

### Positive

#### 1. Discoverability

- **New developers** can explore all debug features via menu
- **No documentation required** to find features (self-documenting)
- **Visual feedback** shows current state of all toggles

#### 2. Reduced Key Conflicts

- **Single key to access all debug** (NumpadDivide)
- **Frees up J/H/G/V/Digit1-3 keys** for gameplay (Phase 2)
- **Numpad isolated from gameplay** (arrows + Q/Space remain unchanged)

#### 3. Organized Testing Workflows

- **Combat testing**: Open console → Combat menu → run all tests from one place
- **Terrain debugging**: Open console → Terrain menu → toggle grid+slopes together
- **Performance profiling**: Open console → Performance menu → enable all monitors

#### 4. Extensibility

- **Easy to add new features** (add menu item, add action event, add handler)
- **No code changes to navigation** (menu system generic)
- **Future-proof for text commands** (event system supports both menus and text)

#### 5. Minimal Gameplay Interruption

- **Overlay doesn't pause game** (can test while playing)
- **Quick toggle** (NumpadDivide on/off)
- **Top-left position** doesn't obscure combat (center screen clear)

### Negative

#### 1. Additional Complexity

- **New system to maintain** (console UI, navigation, action events)
- **More code than scattered keybindings** (trade-off for organization)
- **Menu definitions require updating** when adding features

**Mitigation:**
- Console isolated in `client/plugins/console.rs` (contained complexity)
- Menu definitions declarative (easy to read/modify)
- Tests for navigation logic (prevent regressions)

#### 2. Numpad Requirement

- **Laptops without numpad** cannot use console
- **Numlock off** breaks navigation

**Mitigation:**
- Phase 2: Allow rebinding to alternative keys (QWERTY number row)
- Phase 2: Text commands don't require numpad (type `/toggle grid`)
- Document numpad requirement clearly

#### 3. Two Ways to Do Same Thing (Phase 1)

- **Direct keybindings (J/H/G/etc.) AND console** both work
- **Confusing for new developers** (which should I use?)

**Mitigation:**
- Phase 1: Document console as "preferred" method
- Phase 2: Deprecate direct keybindings (force console)
- Add in-game hint: "Press NumpadDivide for dev console"

#### 4. Menu Navigation vs Direct Keybindings (Speed)

- **Direct key (J) = 1 keypress** to toggle grid
- **Console menu = 3 keypresses** (NumpadDivide, Numpad1 Terrain, Numpad1 Grid)
- **Slower for rapid iteration**

**Mitigation:**
- Phase 1: Keep direct keybindings for frequently-used features (H/G for terrain)
- Phase 2: Text commands fast as direct keys (`/toggle grid` = one command)
- Organize menus to minimize depth (max 2 levels)

#### 5. UI Rendering Overhead

- **Menu rebuilding** on every state change (despawn/respawn text entities)
- **Performance impact** negligible (10-20 text entities max)

**Mitigation:**
- Only rebuild when console visible
- Only rebuild when menu path or state changes (not every frame)
- Profile if issues arise (unlikely with small menu)

### Neutral

#### 1. Bevy UI vs egui

- **Bevy UI chosen** (native, consistent with codebase)
- **egui alternative** (richer features, separate rendering)

**Consideration:**
- Bevy UI sufficient for MVP (text menus, simple layout)
- egui overkill for current needs (future: text input might benefit)

#### 2. Numpad Navigation vs QWERTY

- **Numpad chosen** (isolated, one-handed)
- **QWERTY alternative** (more laptops have it)

**Consideration:**
- Numpad matches "developer tool" mental model (calculator-style input)
- Phase 2 can support both (configurable)

#### 3. Event System vs Direct Calls

- **Event system chosen** (decoupled, extensible)
- **Direct calls alternative** (simpler, less overhead)

**Consideration:**
- Events enable text commands (Phase 2)
- Events enable automation (Phase 3)
- Overhead negligible (dev tool, not hot path)

---

## Implementation Plan

### Phase 1: Core Console (MVP)

**Goal:** Functional console with all existing debug features accessible

**Tasks:**

1. **Console Resources and State** (1 day)
   - Create `DevConsole` resource (visible, current_menu, history)
   - Create `MenuPath` enum (Root, Terrain, Combat, etc.)
   - Create `DevConsoleAction` event enum (all actions)
   - Unit tests for state transitions

2. **UI Rendering** (2 days)
   - Create `setup_dev_console` system (Bevy UI panel)
   - Create `update_console_menu` system (rebuild on menu change)
   - Create menu builders (`build_terrain_menu`, etc.)
   - Visual polish (colors, spacing, breadcrumbs)

3. **Navigation System** (1 day)
   - Create `handle_console_input` system (numpad key handling)
   - Implement breadcrumb history (push/pop on navigation)
   - Test menu navigation (root → sub-menu → back)

4. **Action Execution** (2 days)
   - Create `execute_console_actions` system (event handler)
   - Integrate with existing systems:
     - Terrain actions → `DiagnosticsState` modifications
     - Combat actions → `debug_resources.rs` logic
     - Performance actions → `perf_ui.rs` toggles
     - Visualization actions → `spawner_viz.rs`, `character_panel.rs`
   - Test each action works identically to direct keybindings

5. **Documentation and Polish** (1 day)
   - Create `GUIDANCE/DevConsole.md`
   - Update `CLAUDE.md` to mention console
   - Add in-game hint ("Press NumpadDivide for dev console")
   - Test full workflow (open → navigate → execute → close)

**Duration:** 7 days total

---

### Phase 2: Text Commands (Post-MVP)

**Goal:** Type commands instead of navigating menus

**Tasks:**

1. Text input field (Bevy UI TextInput or egui)
2. Command parser (split command string into action + args)
3. Command registry (map command strings to `DevConsoleAction` events)
4. Command history (store previous commands, up/down arrow recall)
5. Autocomplete (tab completion for command names)

**Duration:** 5 days

---

### Phase 3: Advanced Features (Future)

**Goal:** Power-user features and automation

**Tasks:**

1. Console log (display command output)
2. Macros (bind multiple commands to single key)
3. Scripting (Lua/Rhai for test scenarios)
4. Remote console (external tool connects via TCP)
5. Save/load state (persist favorite commands)

**Duration:** 10-15 days (phased)

---

## Validation Criteria

### Functional Tests

- **Toggle visibility:** NumpadDivide opens/closes console
- **Navigate menus:** Numpad1-5 opens sub-menus, Numpad0 goes back
- **Execute actions:** Each action in menu works identically to direct keybinding
- **State display:** Toggle states (ON/OFF) accurate in menu text
- **Breadcrumbs:** Current menu path shown correctly

### Integration Tests

- **Terrain actions:** Toggle grid/slope/lighting via console, verify mesh updates
- **Combat actions:** Drain resources via console, verify UI bars update
- **Performance actions:** Toggle perf UI via console, verify overlay shows/hides
- **Visualization actions:** Toggle spawner markers via console, verify cylinders spawn/despawn
- **Character panel and zoom:** Still accessible via C and Minus/Equal keys (gameplay features, not in console)

### UX Tests

- **Discoverability:** New developer can find any debug feature within 30 seconds
- **Speed:** Accessing common action (toggle grid) takes <5 seconds
- **Clarity:** Menu labels understandable without documentation
- **Responsiveness:** Console opens/closes within 1 frame (no lag)

### Performance Tests

- **Rendering overhead:** Console visible = <1ms frame time increase
- **Menu rebuilding:** State change triggers rebuild in <0.5ms
- **Memory usage:** Console entities = <10KB memory (negligible)

---

## Open Questions

### Design Questions

1. **Should console show tooltips on hover?**
   - Pro: Explains what each option does (better discoverability)
   - Con: Requires mouse interaction (numpad-only goal)
   - MVP: No tooltips (keep numpad-only), Phase 2: optional mouse support

2. **Should console remember last menu between sessions?**
   - Pro: Faster access to frequently-used menus
   - Con: Confusing if console opens to unexpected menu
   - MVP: Always open to root, Phase 3: save/load state

3. **Should console be accessible on server builds?**
   - Pro: Server debugging (spawn NPCs, teleport, etc.)
   - Con: Requires headless UI (text-only console)
   - MVP: Client-only, Future: server has text-only REPL console

### Technical Questions

1. **Should we use egui instead of Bevy UI?**
   - egui pros: Richer features (text input, scrolling, layouts)
   - egui cons: Separate rendering pipeline, larger dependency
   - MVP: Bevy UI (native, sufficient for menus), Phase 2: reconsider for text input

2. **Should menu items be data-driven (JSON/TOML)?**
   - Pro: Non-programmers can add menu items
   - Con: More complexity (file loading, parsing, validation)
   - MVP: Hardcoded in Rust, Future: data-driven if needed

3. **Should console actions support arguments?**
   - Example: "Drain Stamina" → "Drain Stamina (amount?)"
   - MVP: Fixed amounts (30 stamina, 25 mana), Phase 2: text commands support args

---

## References

### Existing Systems

- **DiagnosticsPlugin:** `client/plugins/diagnostics.rs` (current debug features)
- **DiagnosticsState:** `client/plugins/diagnostics/config.rs` (toggle states)
- **Debug Resource Drains:** `client/systems/debug_resources.rs` (combat tests)
- **Spawner Visualization:** `client/systems/spawner_viz.rs` (spawner markers)

### External References

- **Bevy UI Guide:** https://bevyengine.org/learn/book/ui/
- **egui (alternative):** https://github.com/emilk/egui
- **Quake Console (inspiration):** https://en.wikipedia.org/wiki/Console_(video_game_CLI)

---

## Decision Makers

- ARCHITECT role design (current session)
- User specification: Numpad-navigable contextual menu, consolidate debug features
- UX goals: Discoverability, organization, minimal interruption

## Date

2025-10-30

---

## Summary for Developers

**What this ADR adds:**

1. **DevConsole Resource** - State machine tracking visible/current menu/history
2. **Hierarchical Menu System** - Root menu → 5 sub-menus (Terrain, Combat, Performance, Visualization, Tools)
3. **Numpad Navigation** - NumpadDivide opens console, 0-9 navigates/selects
4. **Action Event System** - `DevConsoleAction` events trigger existing debug features
5. **Dynamic Menu Rendering** - Bevy UI overlay, rebuilds on state/menu changes

**Integration:**

- **Phase 1:** Console provides alternative access (existing J/H/G/V keys still work)
- **Phase 2:** Deprecate direct keybindings (force console for all debug)
- **Shared state:** Console reads/writes `DiagnosticsState`, `SpawnerVizState`, etc.
- **Gameplay features unchanged:** Character panel (C) and camera zoom (Minus/Equal) remain as gameplay controls, NOT in console

**Navigation Example:**
```
Press NumpadDivide → Root Menu shows
Press Numpad1 → Terrain Menu opens
Press Numpad1 → Toggle Grid Overlay [ON]
Press Numpad0 → Back to Root Menu
Press Numpad0 → Close Console
```

**File Structure:**
```
src/client/plugins/console/
  ├── mod.rs            # Plugin definition
  ├── state.rs          # DevConsole resource, MenuPath enum
  ├── actions.rs        # DevConsoleAction event, execution system
  ├── ui.rs             # Bevy UI rendering, menu builders
  ├── navigation.rs     # Input handling, menu transitions
  └── GUIDANCE.md       # How to add new features
```
