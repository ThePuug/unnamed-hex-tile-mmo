# DiagnosticsPlugin (Client)

**Location**: `client/plugins/diagnostics.rs`

**Purpose**: Consolidates all debug and diagnostic features for the client.

## When to Read This

- Adding new debug visualizations or tools
- Modifying keyboard shortcuts for debug features
- Working on performance monitoring
- Debugging rendering or grid display issues

## Features Provided

### Performance Monitoring
- FPS tracking via `FrameTimeDiagnosticsPlugin`
- Entity count tracking via `EntityCountDiagnosticsPlugin`
- Render statistics via `RenderDiagnosticsPlugin`
- On-screen performance UI via `iyes_perf_ui::PerfUiPlugin`

### Hex Grid Overlay
- Visualizes tile boundaries as a wireframe mesh
- Toggleable via keyboard (see `DiagnosticsConfig`)
- Updates when map changes or slope rendering toggles
- Respects slope rendering setting for accurate visualization

### Toggleable Features
- **Slope rendering**: Enables/disables terrain slopes on mesh and grid
- **Fixed lighting**: Consistent lighting mode for debugging
- **Performance UI**: Shows/hides FPS and diagnostic info

## Resources

### `DiagnosticsState`
Tracks current state of all toggleable features:
- `grid_visible: bool` - Whether hex grid overlay is shown
- `slope_rendering_enabled: bool` - Whether terrain slopes are rendered
- Other debug states

### `DiagnosticsConfig`
Stores keyboard shortcuts for toggling features (see `config.rs` submodule)

### `HexGridOverlay`
Component attached to grid overlay entity:
- `needs_regeneration: bool` - Flags when mesh needs update

## Systems

### Startup (run once)
- `grid::setup_grid_overlay` - Creates hex grid overlay entity with mesh
- `perf_ui::setup_performance_ui` - Initializes performance display

### Update (every frame)
- `grid::toggle_grid_visibility` - Handles keyboard input for grid toggle
- `toggles::toggle_slope_rendering` - Handles keyboard input for slope toggle
- `toggles::toggle_fixed_lighting` - Handles keyboard input for lighting toggle
- `perf_ui::toggle_performance_ui` - Handles keyboard input for perf UI toggle
- `grid::update_grid_mesh` - Regenerates grid mesh when needed

## Module Structure

The plugin is organized into submodules:
- `config.rs` - Configuration and state resources
- `grid.rs` - Hex grid overlay systems
- `perf_ui.rs` - Performance UI systems
- `toggles.rs` - Debug toggle systems

## Used By

- Client only: `run-client.rs` includes this plugin
- Server does NOT include this plugin (no rendering)
