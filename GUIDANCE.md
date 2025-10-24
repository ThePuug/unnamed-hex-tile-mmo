# Codebase Architecture Guidance

This document provides critical architectural information for understanding the unnamed-hex-tile-mmo codebase. Read this before making changes to avoid common misunderstandings.

## Development Workflow Rules

### Rule 1: Test-Driven Development (TDD)

**ALWAYS write a failing unit test before making code changes.**

When implementing fixes or new features:
1. **First**: Write a unit test that captures the expected behavior
2. **Second**: Verify the test fails (demonstrating the bug or missing feature)
3. **Third**: Implement the fix/feature
4. **Fourth**: Verify the test passes
5. **Fifth**: Run the full test suite to ensure no regressions

This approach:
- Documents the expected behavior in code
- Prevents regressions
- Makes the intent of changes clear
- Catches misunderstandings early

**Example**:
- Write test that captures expected behavior
- Verify test fails (proves bug exists)
- Implement fix
- Verify test passes and no regressions

### Rule 2: Update GUIDANCE.md After Confirmed Solutions

**When the user confirms a solution should be accepted, update this GUIDANCE.md file immediately.**

Add only the **minimum necessary** to prevent future misunderstandings:
- Critical architectural concepts that were misunderstood
- Key pitfalls that led to bugs
- Essential debugging context for similar issues

**Do NOT add:**
- Exhaustive implementation details (code is self-documenting)
- Redundant explanations already clear from code
- Historical minutiae or verbose examples

Keep guidance **concise and essential** - this is a reference for avoiding mistakes, not comprehensive documentation.

**IMPORTANT**: You have permission to update GUIDANCE.md immediately when the user confirms the code change is accepted. Do NOT attempt to commit the changes yourself - only update the file.

## Table of Contents
- [Development Workflow Rules](#development-workflow-rules)
- [Core Architecture](#core-architecture)
- [Position & Movement System](#position--movement-system)
- [Client-Side Prediction](#client-side-prediction)
- [Network Events](#network-events)
- [Key Components & Resources](#key-components--resources)
- [System Execution Order](#system-execution-order)

---

## Core Architecture

This is a client-server MMO built with Bevy ECS:
- **Authoritative Server**: Server has final say on all game state
- **Client-Side Prediction**: Clients predict their own movement locally for responsiveness
- **Hexagonal Grid**: World is based on Qrz (hexagonal) coordinates
- **Shared Systems**: Many systems (physics, behaviour) run on both client and server

### Key Directories
- `src/common/`: Shared code between client and server
- `src/client/`: Client-only code (rendering, input, networking)
- `src/server/`: Server-only code (AI, terrain generation, connection management)

---

## Position & Movement System

### The Offset Component - CRITICAL UNDERSTANDING

The `Offset` component has **three fields** with distinct purposes:

```rust
pub struct Offset {
    pub state: Vec3,      // Server-authoritative position
    pub step: Vec3,       // Client-predicted position (local) OR current visual position (remote)
    pub prev_step: Vec3,  // Previous frame's step (for rendering interpolation)
}
```

#### **DO NOT CONFUSE THESE ROLES:**

**`state`** (Server Authority):
- Updated **only** when server sends corrections or confirmations
- For **local player**: Represents server's confirmed position; adjusted when crossing tile boundaries
- For **remote players**: Always `Vec3::ZERO` (center of their Loc tile)
- Physics system updates this for local players based on input prediction

**`step`** (Visual Position):
- For **local player**: Client-side predicted position updated by physics system
- For **remote players**: Current interpolated position, smoothly moving toward `state`
- This is what gets **rendered** (interpolated with `prev_step`)

**`prev_step`** (Rendering):
- Previous frame's value of `step`
- Used for smooth inter-frame interpolation in rendering
- Interpolation: `prev_step.lerp(step, overstep_fraction)`

### Position Calculation Flow

```
WORLD POSITION = map.convert(Loc) + Offset.step
```

Where:
- `Loc`: The hex tile entity is in (Qrz coordinates)
- `Offset.step`: Sub-tile offset from center of that hex
- `map.convert()`: Converts hex coordinates to world-space Vec3

### Local Player Movement Flow

1. **Input Capture** (`client/systems/input.rs`):
   - Keyboard → `KeyBits` → Sent to server
   - Input added to local prediction queue

2. **Client-Side Prediction** (`common/systems/physics.rs`):
   - Runs in `FixedUpdate` (every 125ms)
   - Processes input queue, simulating physics
   - Updates `Offset.step` with predicted position

3. **Server Confirmation** (`common/systems/behaviour/controlled.rs`):
   - Server processes same input, sends confirmation
   - Client removes confirmed inputs from prediction queue

4. **Server Correction** (when crossing tiles):
   - Server sends new `Loc` when player crosses tile boundary
   - `world.rs::do_incremental()` handles Loc updates
   - **IMPORTANT**: Must preserve world-space position to avoid visual stutter

5. **Rendering Interpolation** (`client/systems/actor.rs`):
   - Runs every frame (not fixed timestep)
   - Interpolates between `prev_step` and `step` using `overstep_fraction`
   - **Heading-based positioning**: When player is stationary (horizontal offset < 0.01) with a heading set, positions them in the triangle of their hex corresponding to heading direction (at `HERE` distance from center)

### Remote Player Movement Flow

1. **Server Update**: Server sends `Loc` updates when remote players move
2. **Interpolation** (`common/systems/behaviour/controlled.rs::interpolate_remote()`):
   - Runs every frame
   - Smoothly moves `step` toward `state` (Vec3::ZERO)
3. **Rendering**: Same interpolation as local player

---

## Client-Side Prediction

### Input Queue System

```rust
pub struct InputQueue {
    pub queue: VecDeque<Event>,  // FIFO: back=oldest, front=newest
}
```

**Critical Details:**
- Local players have an input buffer in `InputQueues` resource
- Remote players do NOT have input buffers (this is how we distinguish local vs remote)
- Check if entity is local: `buffers.get(&entity).is_some()`

### Input Queue Architecture

**Queue Structure:**
- `VecDeque` with inputs ordered oldest (back) to newest (front)
- Each `Event::Input` contains: `ent`, `key_bits`, `dt` (accumulated time), `seq` (sequence number)
- Sequence numbers increment with `wrapping_add` (u8, wraps at 256)

**Queue Flow:**
1. **Client sends** `Event::Incremental{KeyBits}` when keys change OR every 1 second (periodic)
2. **Both client & server** receive and create `Event::Input` with `seq = prev_seq + 1`
3. **New inputs pushed to front** with `dt=0`, displacing older inputs toward back
4. **Every frame**: `controlled::tick` pops front, adds frame dt, pushes back to front
5. **Server** pops from back (oldest) and sends as confirmation via `send_input`
6. **Client** receives confirmations via `do_input`, searches queue by seq, removes

**Why Periodic Updates:**
- Prevent `dt` overflow (u16 max = 65 seconds)
- Ensure regular confirmations even when keys held constant
- Both client & server create inputs for same `KeyBits`, keeping queues synchronized

**Confirmation Matching:**
- Client searches entire queue for matching `seq` (not just back)
- Network latency causes temporary queue length differences (client ahead by ~1-3 inputs)
- Key_bits must match or serious desync occurred
- Queue length > 5 indicates confirmation lag

### Prediction Process

1. Client sends `Event::Incremental{KeyBits}` to server
2. Client's `controlled::tick` creates `Event::Input`, adds to queue front
3. Client immediately simulates input locally (optimistic)
4. Server receives same `Event::Incremental`, creates matching `Event::Input`
5. Server's `send_input` pops from back, sends confirmation with `seq`
6. Client's `do_input` finds matching `seq`, removes from queue
7. If server disagrees (rare), client gradually corrects toward server state

---

## Network Events

### Event Types (`common/message.rs`)

**`Try`**: Requests from client to server (or local requests)
- Client wants to perform action
- Example: `Try { event: Event::Input { ... } }`

**`Do`**: Commands from server to client (or confirmed actions)
- Server confirms action or broadcasts state
- Example: `Do { event: Event::Incremental { component: Component::Loc(...) } }`

### Common Event Patterns

**`Event::Input`**: Player input with sequence number
- Contains: `ent`, `key_bits`, `dt`, `seq`
- Used for client-side prediction and server confirmation

**`Event::Incremental`**: Component updates
- Contains: `ent`, `component`
- Used for: `Component::Loc`, `Component::KeyBits`, `Component::Heading`, etc.

**`Event::Spawn`**: Entity spawning
- Contains: `ent`, `typ`, `qrz`

---

## Key Components & Resources

### Components

**`Loc`**: Hexagonal grid position (Qrz)
- Wrapper around `Qrz` type from qrz library
- Represents which hex tile entity is in

**`Offset`**: Sub-tile position (see detailed explanation above)

**`Behaviour`**: Controls entity AI/control mode
- `Behaviour::Controlled`: Player-controlled (local or remote)
- `Behaviour::PathTo`: AI pathfinding to target

**`KeyBits`**: Bitfield of pressed keys
- Efficient representation of input state
- Constants: `KB_UP`, `KB_DOWN`, `KB_LEFT`, `KB_RIGHT`, `KB_JUMP`, etc.

**`Heading`**: Direction entity is facing
- Used for FOV calculation and animation
- **Also used for positioning**: Stationary players are positioned in the triangle of their hex tile corresponding to their heading direction
- Position calculation: `tile_center + (direction_to_heading_neighbor * HERE)` where `HERE = 0.33`

**`AirTime`**: Jump/fall state
- `None` = grounded
- `Some(positive)` = ascending (jump)
- `Some(negative)` = falling

**`Physics`**: Marker component for entities that undergo physics simulation

**`ActorAttributes`**: Configurable entity attributes
- `movement_speed`: Custom movement speed (default: 0.005)

### Resources

**`InputQueues`**: Manages prediction queues
- Maps `Entity` → `InputQueue`
- Tracks which entities have non-empty queues for performance

**`EntityMap`** (client only): Maps remote entity IDs to local entity IDs
- Server uses different entity IDs than client
- Bidirectional map for translation

**`Map`**: Hexagonal world map
- Wraps `qrz::Map<EntityType>`
- Provides `convert()` for hex → world space

**`NNTree`**: Spatial index (R*-tree)
- Fast nearest-neighbor queries
- Used for collision detection

---

## System Execution Order

### FixedUpdate (125ms intervals)
1. `controlled::apply` - Apply physics to controlled entities
2. `controlled::tick` - Accumulate time on inputs
3. `controlled::interpolate_remote` - Interpolate remote players toward state
4. `physics::update` - Run physics simulation on local predictions

### Update (Every Frame)
1. `renet::do_manage_connections` - Handle network events
2. `world::do_incremental` - Process component updates from server
3. `input::do_input` - Process input confirmations
4. `actor::update` - Interpolate actor positions for rendering, apply heading-based positioning
5. `camera::update` - Update camera position

### Important: Loc Update Handling

When a `Component::Loc` update is received (tile boundary crossing):

**MUST preserve world-space positions** for both local and remote players:
```rust
// Convert to world space
let prev_world = map.convert(**loc0) + offset0.prev_step;
let step_world = map.convert(**loc0) + offset0.step;

// Re-express in new tile's coordinate system
offset0.prev_step = prev_world - map.convert(*loc);
offset0.step = step_world - map.convert(*loc);
```

This ensures smooth visual transitions without stuttering.

---

## Common Pitfalls & Important Notes

### ⚠️ DO NOT:
1. **Modify `step` when you mean to modify `state`**
   - `state` = server authority
   - `step` = client prediction or interpolated position

2. **Assume local and remote players work the same way**
   - Check: `buffers.get(&entity).is_some()` to distinguish

3. **Forget to preserve world-space positions during Loc updates**
   - Always convert to world space, then back to new tile's local space

4. **Run interpolation in FixedUpdate**
   - Rendering interpolation must run every frame (Update schedule)

5. **Modify rendering positions in FixedUpdate**
   - Physics modifies `state` and `step` in FixedUpdate
   - Rendering reads them in Update

6. **Remove periodic `Event::Incremental` updates**
   - Required to prevent dt overflow after 65 seconds
   - Creates "duplicate" inputs (same key_bits) but necessary for system stability

7. **Expect perfect queue synchronization between client/server**
   - Network latency causes 1-3 input difference (normal)
   - Client confirmation handler searches entire queue, not just back

8. **Apply heading positioning in physics/input systems**
   - Heading-based positioning is a **rendering-only** adjustment
   - Applied in `client/systems/actor.rs::update()` during final position calculation
   - Only affects stationary players (not pressing movement keys)
   - Physics and input systems should continue to work with `offset.step` as normal

9. **Check offset magnitude to detect stationary players**
   - Stationary = check `KeyBits`, NOT `offset.step` magnitude
   - Offset is physics *result*, KeyBits is player *intent*
   - Checking offset causes stuttering (small offset while keys pressed)

### ✅ DO:
1. **Use shared code paths when possible**
   - Both local and remote players should share common logic where appropriate

2. **Understand the prediction/confirmation loop**
   - Input → Predict → Confirm → Remove from queue

3. **Respect the server as source of truth**
   - Client prediction is optimistic, server always wins

4. **Use proper schedules**
   - FixedUpdate: Physics, gameplay logic
   - Update: Rendering, interpolation, network I/O

5. **Write tests before implementing fixes**
   - See [Development Workflow Rules](#development-workflow-rules)
   - Tests document expected behavior and prevent regressions
   - Place tests in appropriate module with `#[cfg(test)]` annotation

---

## Physics Constants

Located in `common/systems/physics.rs`:

```rust
const GRAVITY: f32 = 0.005;                    // units/ms²
const JUMP_ASCENT_MULTIPLIER: f32 = 5.0;      // Jump is 5x faster than fall
const JUMP_DURATION_MS: i16 = 125;             // Jump ascent time
const PHYSICS_TIMESTEP_MS: i16 = 125;          // Fixed update interval
const MOVEMENT_SPEED: f32 = 0.005;             // Default units/ms
const SLOPE_FOLLOW_SPEED: f32 = 0.95;          // Terrain following speed
const LEDGE_GRAB_THRESHOLD: f32 = 0.0;         // Disabled
const MAX_ENTITIES_PER_TILE: usize = 7;        // Collision limit
```

---

## Debugging Tips

### Visual Stuttering Issues
- Check if `step` and `prev_step` are being preserved during Loc updates
- Verify world-space calculations are correct
- Check that interpolation is running every frame, not in FixedUpdate

### Position Desync Issues
- Check server confirmation is arriving
- Verify input sequence numbers match
- Look for `state` vs `step` confusion

### Movement Not Working
- Check if entity has `Physics` component
- Verify input is being captured and sent to server
- Check if entity has input buffer in `InputQueues`

### Remote Players Not Moving Smoothly
- Verify `interpolate_remote()` is running
- Check that remote players don't have input buffers
- Ensure `state` is being set to `Vec3::ZERO`

### Players Standing at Center Instead of Heading Triangle
- Verify `actor::update()` runs in Update schedule
- Check heading is set (not `Qrz::default()`)
- Stationary detection must use KeyBits, not offset magnitude

### Movement Stuttering
- Cause: Checking `offset.step` magnitude instead of `KeyBits` for stationary detection
- Fix: Use `keybits.key_bits & (KB_HEADING_Q | KB_HEADING_R) == 0`
- Always use physics position when movement keys pressed

---

## Testing

Run tests with:
```bash
cargo test               # All tests
cargo test physics       # Physics system tests
cargo test behaviour     # Behaviour system tests
```

---

## Additional Resources

- See inline documentation in `common/systems/physics.rs` for detailed physics implementation
- See `common/message.rs` for complete event/component definitions
- See `qrz` library for hexagonal grid math
