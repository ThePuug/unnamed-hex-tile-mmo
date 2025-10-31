# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play

**Early Prototype** - Experience the core tactical combat system in development.

## What You Can Do Now
- **Explore** - Traverse procedurally generated hexagonal terrain with organic slopes and dynamic day/night cycles
- **Combat** - Face hostile creatures using directional keyboard combat (no mouse required)
- **React** - Experience the unique threat queue system - incoming attacks give you time to respond before damage lands
- **Experiment** - Test abilities, observe critical hits, see mutual destruction in action

## Controls
- Arrow keys - Move through the hex grid
- Number keys - Activate abilities (combat system in progress)
- G - Toggle hex grid visualization
- Q - Spawn test entities

## What to Expect
This is a combat-focused prototype. You'll fight enemies, take damage, die, and respawn. The core tactical reaction system is playable - everything else is coming soon.

**Focus:** The combat should feel deliberate and strategic, not twitchy. You're testing whether having a window to react to incoming damage feels engaging.

# Game features
## things you care about
- client-side prediction (movement feels instant, zero perceived lag)
- tactical reaction-based combat system (designed for conscious decisions over twitch reflexes)
  - reaction windows between being hit and taking damage (queue threats, react or absorb)
  - mutual destruction possible (both combatants can kill each other simultaneously)
  - fair critical hit system (determined at attack time based on attacker's Instinct attribute)
- directional keyboard combat (designed for no mouse required)
- sun/moon/season cycles with dynamic lighting
- hexagonal movement with A* pathfinding
- procedural perlin noise terrain generation
- organic terrain slopes (tiles slope toward neighbors at different elevations)
- chunk-based world streaming with smart caching
- architected for massive scale (targeting 1000+ concurrent players, 100 km² world - unproven but designed to handle it)

## Architectural Foundations

- Authoritative server with client-side prediction
- R*-tree spatial indexing for O(log n) entity queries
- Custom hexagonal coordinate system (`qrz` library)
- Chunk-based terrain discovery with LRU world cache
- Boundary-triggered fog-of-war (not per-movement)
- Input stream isolation (streaming vs GCD)
- Procedural mesh generation with organic slope transitions
- A* pathfinding on hex grid
- Do/Try event pattern for client-server authority
- Four-stage damage pipeline: Deal → Insert → Resolve → Apply (each stage testable independently)
- Hybrid damage timing: outgoing at attack time (attacker state), mitigation at resolution (defender state)
- ECS architecture (Bevy engine)
- Network protocol with client prediction and rollback
- Contextual developer console with hierarchical menus
- Shared game logic in `common/` (client and server use identical physics/behavior)