# ADR-013: Developer Console Architecture

## Status

**Accepted** - 2025-10-30

## Context

**Related RFC:** [RFC-007: Contextual Developer Console](../01-rfc/007-developer-console.md)

Debug features scattered across keyboard (J, H, G, V, F3, Digit1-3) with no discoverability. Need organized, visual menu system.

### Requirements

- Discoverable (show all available features)
- Organized (group by category)
- One-handed navigation (numpad only)
- No gameplay interruption (overlay, not pause)
- Extensible (easy to add features)

### Options Considered

**Option 1: Hierarchical Menu with Event System** ✅ **SELECTED**
- Root menu → 5 sub-menus (Terrain, Combat, Performance, Visualization, Tools)
- Event-based actions (decoupled)
- Bevy UI rendering
- Numpad navigation

**Option 2: Flat Command List**
- Single menu with all commands
- ❌ Poor organization, doesn't scale

**Option 3: Text Commands Only**
- Type `/toggle grid`
- ❌ Harder to discover, needs text input
- Deferred to Phase 2

**Option 4: egui Rendering**
- Rich UI library
- ❌ Separate rendering pipeline, overkill for MVP

## Decision

**Use hierarchical menu with event-based actions and Bevy UI rendering (Option 1).**

### Core Mechanism

**Console State (Resource):**
```rust
pub struct DevConsole {
    pub visible: bool,
    pub current_menu: MenuPath,
    pub history: Vec<MenuPath>,  // Breadcrumb navigation
}

pub enum MenuPath {
    Root,
    Terrain,
    Combat,
    Performance,
    Visualization,
    Tools,
}
```

**Action Events:**
```rust
pub enum DevConsoleAction {
    // Terrain
    ToggleGrid,
    ToggleSlopeRendering,
    ToggleFixedLighting,

    // Combat
    QueueDamageThreat,
    DrainStamina,
    DrainMana,

    // Performance
    TogglePerfUI,

    // Visualization
    ToggleSpawnerMarkers,
}
```

**Navigation System:**
- NumpadDivide: Toggle console visibility
- Numpad 1-9: Select menu option
- Numpad 0: Back (or close if at root)
- Breadcrumb shows current path (Root → Terrain → Grid)

**UI Rendering:**
- Bevy UI overlay (semi-transparent panel)
- Top-left position (doesn't obscure gameplay)
- Dynamic menu rebuilding (despawn/respawn on menu change)
- State indicators: Green [ON], Red [OFF], Yellow [Action]

**Integration with Existing Systems:**
- Console reads `DiagnosticsState`, `SpawnerVizState`, etc.
- Actions modify same resources as direct keybindings
- No state duplication
- Phase 1: Dual access (console + direct keys both work)
- Phase 2: Deprecate direct keys (force console)

---

## Rationale

### 1. Event-Based Actions

**Decouples navigation from execution:**
- Input handling emits `DevConsoleAction` events
- Separate system processes events
- Enables future text commands (same events, different input)

### 2. Resource-Based State

**Why Resource (not component):**
- Single global console (not per-entity)
- Accessed by many systems
- Simpler than singleton entity query

### 3. Hierarchical Menus

**Organization by category:**
- Terrain: Grid, Slope, Lighting
- Combat: Resource drains, Queue tests
- Performance: FPS, Profiling
- Max 2 levels deep (Root → Sub-menu)

**Benefits:** Easy to find features, scales to 100+ commands

### 4. Numpad Navigation

**One-handed, isolated from gameplay:**
- Numpad separate from arrow keys (gameplay)
- Calculator-style input (matches dev tool aesthetic)
- No conflicts with gameplay controls

### 5. Bevy UI Rendering

**Native, consistent with codebase:**
- No external dependencies (vs egui)
- Sufficient for text menus
- Future: Reconsider for text input (egui might help)

---

## Consequences

### Positive

- **Discoverability:** New developers find features without docs
- **Organization:** Logical grouping by category
- **Extensibility:** Add action event + menu item
- **Reduced conflicts:** Frees J/H/G/V keys for gameplay (Phase 2)
- **No interruption:** Overlay doesn't pause game

### Negative

- **Numpad requirement:** Laptops without numpad can't use
- **Navigation slower:** 3 keypresses (/, 1, 1) vs 1 (J)
- **Two ways initially:** Console + direct keys confusing
- **Additional complexity:** New system to maintain

### Mitigations

- Phase 2: Rebindable keys, text commands
- Keep frequent toggles as direct keys (Phase 1)
- Document console as preferred method
- Isolate complexity in `client/plugins/console.rs`

---

## Implementation Notes

**File Structure:**
```
src/client/plugins/console/
  ├── mod.rs            # Plugin definition
  ├── state.rs          # DevConsole resource, MenuPath enum
  ├── actions.rs        # DevConsoleAction event, execution
  ├── ui.rs             # Bevy UI rendering, menu builders
  └── navigation.rs     # Input handling, menu transitions
```

**Menu Example:**
```
Developer Console
─────────────────
1. Terrain      [Slope: ON, Grid: OFF, Light: Fixed]
2. Combat       [Test tools and cheats]
3. Performance  [FPS: ON, Profiling: OFF]
4. Visualization[Spawners: ON]
5. Tools        [Camera, Spawn, Teleport]

0. Close Console
```

**Action Execution:**
```rust
pub fn execute_console_actions(
    mut reader: EventReader<DevConsoleAction>,
    mut state: ResMut<DiagnosticsState>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::ToggleGrid => {
                state.grid_visible = !state.grid_visible;
                // Trigger mesh regeneration...
            }
            // ... other actions
        }
    }
}
```

---

## Validation Criteria

**Functional:**
- NumpadDivide toggles console visibility
- Numpad 1-9 navigates menus, executes actions
- Numpad 0 goes back (or closes)
- State indicators accurate (ON/OFF)
- All existing features accessible

**UX:**
- Find any feature within 30 seconds
- Menu labels clear (no documentation needed)
- Console opens/closes within 1 frame
- Breadcrumbs show current location

**Performance:**
- Console visible: < 1ms frame time increase
- Menu rebuilding: < 0.5ms
- Memory: < 10KB

---

## References

- **RFC-007:** Contextual Developer Console
- **Existing:** DiagnosticsPlugin, debug_resources.rs, spawner_viz.rs

## Date

2025-10-30
