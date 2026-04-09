# Extract Client WorldStreamingPlugin

Adopt the DEVELOPER role. Extract the terrain streaming and LoD mesh systems from `main.rs` into a self-contained `WorldStreamingPlugin`.

## Goal

All chunk loading, eviction, and summary mesh generation systems live in one plugin with explicit ordering. `main.rs` registers the plugin instead of individual systems.

## New file: `crates/client/src/plugins/world_streaming.rs`

Create a plugin following the same pattern as `DiagnosticsPlugin` (`crates/client/src/plugins/diagnostics.rs`). The plugin struct is `WorldStreamingPlugin`, implements `Plugin`.

### Systems to register in the plugin's `build()`:

**Update schedule:**
```
world::do_spawn
world::dispatch_summary_tasks.after(world::do_spawn)
world::poll_summary_meshes.after(world::dispatch_summary_tasks)
```

**Update schedule (admin-conditional eviction):**
```rust
#[cfg(feature = "admin")]
app.add_systems(Update, world::evict_data.run_if(admin::not_in_flyover));
#[cfg(not(feature = "admin"))]
app.add_systems(Update, world::evict_data);
```

The system functions stay in `crates/client/src/systems/world.rs` — the plugin only handles registration and resource init, same as the other plugins.

### Resources to init in the plugin's `build()`:

Move these four `init_resource` / `insert_resource` calls from `main.rs` into the plugin:

```rust
app.init_resource::<LoadedChunks>();
app.init_resource::<SummaryMeshes>();
app.init_resource::<ForcedSummaryRadius>();
app.init_resource::<LodTriangleStats>();
app.init_resource::<SummaryCache>();
```

These resources are primarily owned by the streaming systems. Other systems (diagnostics, admin) read them — that's fine, Bevy resources are globally accessible. Ownership means "who creates and primarily mutates."

**Do NOT move into the plugin:**
- `ClientTimers` — cross-cutting timing resource used by renet, admin, diagnostics. Stays in `main.rs`.
- `Map` — shared read/write across all game systems. Stays in `main.rs`.
- `EntityMap` — entity lifecycle, not streaming. Stays in `main.rs`.
- `SkipNeighborRegen` — admin-only. Stays in `main.rs`.

## Changes to `main.rs`

### Remove from `main.rs`:

1. The `add_systems(Update, ...)` block containing `world::do_spawn`, `dispatch_summary_tasks`, `poll_summary_meshes` (currently lines 159–167, same block as `do_init`, `handle_pong`, `periodic_ping`, `world::update`). Keep the non-streaming systems — see below.

2. The standalone `evict_data` registration (lines 211–214, admin-gated).

3. The five `init_resource` calls listed above (lines 180, 182, 184–186).

### Keep in `main.rs` (from the block that currently mixes streaming with non-streaming):

These stay registered directly in `main.rs` — they are not streaming systems:

```rust
app.add_systems(Update, (
    world::do_init,
    renet::handle_pong,
    renet::periodic_ping,
    world::update,
));
```

### Add plugin registration:

```rust
use crate::plugins::world_streaming::WorldStreamingPlugin;
// ...
app.add_plugins((
    // ... existing plugins ...
    WorldStreamingPlugin,
));
```

### Update `crates/client/src/plugins/mod.rs`:

Add `pub mod world_streaming;`

## Imports in the plugin file

The plugin needs to reference:
- `crate::systems::world` (for the system functions)
- `crate::resources::{LoadedChunks, SummaryMeshes, ForcedSummaryRadius, LodTriangleStats, SummaryCache}`
- `#[cfg(feature = "admin")] crate::systems::admin` (for `not_in_flyover` run condition)

## DiagnosticsPlugin collision note

`DiagnosticsPlugin` currently has these lines (around line 86–87, in a test or conditional block):
```rust
app.insert_resource(SummaryMeshes::default());
app.world_mut().resource_mut::<SummaryMeshes>().set_changed();
```
and later:
```rust
app.init_resource::<SummaryMeshes>();
```

Check whether this is in a test or conditional path. If `DiagnosticsPlugin` is also initializing `SummaryMeshes`, resolve the duplicate — WorldStreamingPlugin should be the single owner. `init_resource` is idempotent (no-op if already exists), but `insert_resource` will overwrite. Ensure plugin ordering puts WorldStreamingPlugin before DiagnosticsPlugin, or remove the diagnostics init.

## Verification

```bash
cargo build -p client
cargo build -p client --features admin
cargo build -p client --no-default-features
```

Functional check: run client+server, confirm terrain loads, LoD summary meshes appear at distance, chunk eviction works when walking.

## What this enables

After this extraction, the streaming pipeline is self-contained:
- Adding a new LoD tier means adding a system to `WorldStreamingPlugin::build()` with explicit ordering relative to the existing chain
- The resource set is visible in one place
- No risk of accidentally interleaving with combat/input/targeting systems
