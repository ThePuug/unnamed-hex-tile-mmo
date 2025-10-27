# BehaviourPlugin (Server)

**Location**: `server/plugins/behaviour.rs`

**Purpose**: Manages server-only AI behaviour systems for NPCs and pathfinding.

## When to Read This

- Adding new AI behaviors for NPCs
- Modifying pathfinding logic
- Working on NPC target selection
- Debugging NPC movement or decision-making

## Systems (all run in FixedUpdate)

### `find_something_interesting_within`
- NPCs with `FindSomethingInterestingWithin` component select random nearby entities
- Queries spatial index (`NNTree`) within specified distance
- Sets `Target` component on success
- Triggers bevy_behave success event

### `nearby`
- Processes `Nearby` component to pick random hex location near an origin
- Supports three origin types:
  - `NearbyOrigin::Target` - relative to target entity's location
  - `NearbyOrigin::Dest` - relative to current destination
  - `NearbyOrigin::Loc(loc)` - relative to specific location
- Picks random distance in `[min, max]` range
- Sets `Dest` component with chosen location
- Triggers bevy_behave success event

### `pathto::tick`
- Manages pathfinding for entities with `PathTo` component
- Generates paths using A* algorithm
- Respects `PathLimit` variants:
  - `PathLimit::By(n)` - move N tiles then succeed
  - `PathLimit::Until(n)` - move until N tiles away then succeed
  - `PathLimit::Complete` - move all the way to destination
- Updates path array as entity progresses

### `pathto::apply`
- Applies movement along computed path
- Pops next tile from path and sets as current `Loc`
- Triggers bevy_behave success when path complete or limit reached

## Integration with bevy_behave

These systems integrate with the `bevy_behave` behavior tree plugin. They:
- Query for `BehaveCtx` to get target entity and trigger success/failure
- Operate as behavior tree leaf nodes
- Can be composed into complex AI behaviors

## Used By

- Server only: `run-server.rs` includes this plugin
- Client does NOT include this plugin (AI runs server-side only)
