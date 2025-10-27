# NNTreePlugin

**Location**: `common/plugins/nntree.rs`

**Purpose**: Provides spatial indexing for fast nearest-neighbor queries on hexagonal grid.

## When to Read This

- Implementing features that need to find nearby entities
- Optimizing spatial queries or collision detection
- Adding AI behaviors that require proximity detection
- Debugging spatial query performance issues

## Overview

NNTreePlugin wraps an R*-tree (via `rstar` crate) to provide efficient spatial queries. It automatically maintains the tree as entities move around the world.

## Key Components

### `NearestNeighbor` Component
Marker component that adds an entity to the spatial index:
```rust
pub struct NearestNeighbor {
    pub ent: Entity,
    pub loc: Loc,
}
```

**How it works:**
- Component hooks (`on_add`/`on_remove`) automatically insert/remove from tree
- When `Loc` changes, `update` system re-inserts entity with new position

### `NNTree` Resource
Spatial index resource that wraps `RTree<NearestNeighbor>`:
- Derefs to RTree for direct access to query methods
- Provides methods like `locate_within_distance`, `nearest_neighbor`, etc.

## Distance Metric: "Hexhattan"

The plugin uses a custom distance metric for hexagonal grids:
- **2D**: Maximum of absolute differences in cube coordinates (q, r, s)
  - Where s = -q - r (derived from axial invariant)
- **3D**: Adds absolute z difference to 2D distance
- Returns **distance squared** for performance

This correctly handles hex grid topology where neighbors are equidistant.

## Systems

### `update` (runs in Update schedule)
- Queries for entities with `Changed<Loc>`
- Removes old position from tree
- Updates `NearestNeighbor.loc`
- Inserts new position into tree

## Usage Pattern

To make an entity queryable in the spatial index:
1. Add `NearestNeighbor::new(entity, loc)` component to entity
2. Tree automatically updates as entity moves
3. Query tree using `NNTree` resource methods

Example:
```rust
fn find_nearby(
    nntree: Res<NNTree>,
    query: Query<&Loc, With<Player>>,
) {
    for &loc in &query {
        let nearby = nntree.locate_within_distance(loc, 100);
        // Process nearby entities...
    }
}
```

## Used By

- Both client and server: `run-client.rs` and `run-server.rs` include this plugin
- Used by spawner system (NPC activation/despawn)
- Used by AI behaviors (`find_something_interesting_within`)
- Used anywhere proximity detection is needed

## Performance Notes

- R*-tree provides O(log n) queries for most operations
- Distance squared avoids expensive square root calculations
- Component hooks ensure tree stays synchronized automatically
- No manual tree management required
