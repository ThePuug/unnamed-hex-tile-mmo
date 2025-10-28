# UiPlugin (Client)

**Location**: `client/plugins/ui.rs`

**Purpose**: Consolidates all game UI elements for the client.

## When to Read This

- Adding new UI features (panels, HUD elements, overlays)
- Modifying character panel or attribute system
- Working on player feedback systems
- Adding interactive UI elements

## Features Provided

### Character Panel
- **Toggle**: Press `C` to show/hide
- **Display**: Shows all three attribute pairs (MIGHT↔GRACE, VITALITY↔FOCUS, INSTINCT↔PRESENCE)
- **Interactive**: Click and drag yellow bars to adjust shift values within spectrum range
- **Real-time**: Updates shift values immediately on drag (client-side only, no server sync yet)

### HUD Elements
- **Time Display**: Shows game time in format `HH:MM D.Week.Season`
  - Seasons: Thaw, Blaze, Ash, Freeze
  - Days: Mon, Tus, Wed, Tur, Fid, Sat, Sun
  - Synced with server via `Server::elapsed_offset`

### Target Cursor
- **Visual Feedback**: Red hex overlay showing which tile the player is looking at
- **Terrain Following**: Matches terrain slopes (respects slope rendering toggle)
- **Dynamic**: Updates based on player's `Heading` component

## Resources

### `CharacterPanelState`
Tracks character panel visibility and drag interaction:
- `visible: bool` - Whether panel is shown
- `dragging: Option<DragState>` - Current drag state if user is dragging an attribute bar

### `DragState`
Captures drag interaction state:
- `attribute: AttributeType` - Which attribute is being dragged
- `bar_entity: Entity` - Entity of the bar being dragged
- `initial_mouse_x: f32` - Mouse X position when drag started
- `initial_shift: i8` - Shift value when drag started

## Components

### Character Panel Components
- `CharacterPanel` - Root marker for panel entity
- `AttributeTitle` - Marker for title row (shows reach values)
- `AttributeCurrent` - Marker for current value text
- `AttributeBar` - Marker for bar container (clickable for dragging)
- `SpectrumRange` - Visual spectrum indicator (green bars)
- `AxisMarker` - Visual axis indicator (yellow bars, draggable)

### HUD Components
- `UiTargetCamera` - Links UI to camera
- `Info::Time` - Marker for time display text

### Target Cursor Components
- `TargetCursor` - Marker for cursor entity

## Systems

### Startup (run once)
- `ui::setup` - Creates HUD root with time display (runs after camera setup)
- `character_panel::setup` - Creates character panel UI (initially hidden)
- `target_cursor::setup` - Creates red hex cursor mesh

### Update (every frame)
- `ui::update` - Updates time display based on server time
- `character_panel::toggle_panel` - Handles C key to show/hide panel
- `character_panel::handle_shift_drag` - Handles mouse drag on attribute bars
- `character_panel::update_attributes` - Updates panel visuals based on `ActorAttributes`
- `target_cursor::update` - Updates cursor position and mesh to follow player's heading

## Attribute System Details

### Drag Interaction
The attribute bars use a **delta-based dragging system**:
1. **On click**: Capture initial mouse X position and current shift value
2. **While dragging**: Calculate mouse movement delta in pixels
3. **Convert delta**: Map pixel delta to attribute units (250px = 240 units)
4. **Update shift**: `new_shift = initial_shift + delta`
5. **Clamp**: Keep shift within `[-spectrum, +spectrum]` range

This approach prevents the bar from "jumping" when you click on it - it only moves as you drag.

### Visual Layout
Each attribute pair shows:
- **Top row**: `[LEFT_NAME] reach: XXX | reach: XXX [RIGHT_NAME]`
- **Middle row**: `current: XX`
- **Bottom row**: Visual bar with:
  - Gray background (full -120 to +120 range)
  - Green bars (spectrum range based on attributes)
  - Yellow bar (current shift position, draggable)

## Used By

- Client only: `run-client.rs` includes this plugin
- Server does NOT include this plugin (no UI rendering)

## Adding New UI Features

When adding new UI elements:
1. Create systems in `client/systems/` (e.g., `new_panel.rs`)
2. Add setup system to `Startup` in the plugin
3. Add update systems to `Update` in the plugin
4. Add any resources with `init_resource` in the plugin
5. Update this documentation

## Related Systems

- **`ActorAttributes`** (in `common/components/mod.rs`) - The data model for attributes
- **Attribute formulas** - Documented in attribute component tests
- **`DiagnosticsState`** - Target cursor respects `slope_rendering_enabled` flag
