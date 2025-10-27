# ControlledPlugin

**Location**: `common/plugins/controlled.rs`

**Purpose**: Manages systems for entities with `Behaviour::Controlled` (player-controlled entities, both local and remote).

## When to Read This

- Modifying player movement or input handling
- Working on client-side prediction
- Changing remote player interpolation
- Debugging synchronization between client and server

## Systems (all run in FixedUpdate)

### `controlled::apply`
- Applies physics to controlled entities
- Prepares `offset.prev_step` for rendering interpolation
- Updates `offset.step` to current physics state

### `controlled::tick`
- Accumulates time (`dt`) on the front input in each entity's input queue
- Maintains queue invariant: always at least 1 input per controlled entity
- **Critical**: Uses `front_mut()` to modify in-place (never pop/push)

### `controlled::interpolate_remote`
- Interpolates remote players' `step` toward authoritative `state`
- Runs at constant speed (`movement_speed`)
- Local players skip this system (distinguished by presence in `InputQueues`)

## System Ordering

The plugin does not specify explicit ordering. External systems must order themselves relative to these:

**Client**: `input::do_input` must run `.after(controlled::tick)` to ensure confirmations process after time accumulation

**Both**: `physics::update` typically runs after these systems in FixedUpdate

## Used By

- Client: `run-client.rs` includes this plugin
- Server: `run-server.rs` includes this plugin

Both client and server need these systems for controlled entity behavior.
