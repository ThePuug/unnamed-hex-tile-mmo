# qrz Library

Hexagonal coordinate system library — 3D hex grid math and world space conversion. This is the coordinate reference, not game design or architecture.

## When to Read This

- Working with hex coordinates or grid navigation
- Converting between hex tiles and world positions
- Adding grid-based features (rings, lines, walks round a tile)
- Debugging position or distance calculations

## Core Concepts

### Axial Coordinates (q, r, z)

The library uses **axial coordinates** for hexagonal grids:
- **q**, **r**: The two grid axes
- **s**: Derived axis (s = -q - r)
- **z**: Vertical elevation

**Invariant**: `q + r + s = 0` (automatically maintained)

### Orientation

The grid is flat-top: a flat edge faces north, vertices point east and west. `convert()` uses `x = 3/2*q, z = √3/2*q + √3*r`, so north is −z and east is +x. Up maps directly to N (flat edge), so arrow keys need no heading context.

`DIRECTIONS` runs round the tile NW, SW, S, SE, NE, N — `(-1,0), (-1,1), (0,1), (1,0), (1,-1), (0,-1)` — and `ring` and `circling` turn the same way.

**Heading angles** (flat-top compass): N=0°, NE=60°, SE=120°, S=180°, SW=240°, NW=300°. `From<Heading> for Quat` converts to Y-rotation via `quat_angle = 2π - compass` (Y-rotation is CCW from above, compass is CW). Targeting's `to_angle()` uses the same table, and `angle_between_locs()` adds a +90° offset because `atan2` on flat-top Cartesian puts SE at 30°, not 120°.

> These heading/targeting facts live in `common-bevy` (`components/heading.rs`, `systems/targeting.rs`), not in `qrz` — they are the game layer's convention on top of the library. They are recorded here because they are the part people get wrong.

### Distance Metrics

**`hex_distance((q1, r1), (q2, r2))`**: The hex distance between two axial coordinates — the maximum of the absolute cube deltas, `max(|dq|, |dr|, |dq + dr|)`. The one formula: `common-bevy`'s chunks and summaries and the server call it rather than restating it.

**`flat_distance(other)`**: `hex_distance` of two tiles, elevation ignored. Adjacent hexes are distance 1.

**`distance(other)`**: `flat_distance` plus the absolute z difference. Use for queries that care about height.

## Key Types

### `Qrz` Struct
```rust
pub struct Qrz {
    pub q: i32,
    pub r: i32,
    pub z: i32,
}
```

**Constants**:
- `Qrz::Z` - Unit vector in z direction (0,0,1)
- `DIRECTIONS` - The 6 neighbour offsets, in order round the tile

**Key Methods**:
- `flat_distance(&Qrz) -> i32` / `distance(&Qrz) -> i32`
- `neighbors() -> Vec<Qrz>` - All 6 adjacent hexes (same z), in `DIRECTIONS`' order
- `ring(radius) -> Vec<Qrz>` - The `6 × radius` tiles at that flat distance, in order round the ring; entry `radius × i` is the corner along `DIRECTIONS[i]`
- `circling(from, to) -> Vec<Qrz>` - The walk round this tile along `to`'s ring, the shorter way, from the ring tile nearest `from` through `to`
- `beyond(from, steps) -> Vec<Qrz>` - The tiles straight on along the line from `from` through this one, a neighbour at a time

**Arithmetic**: Supports `+`, `-`, `*` (scalar multiply, z included). `Qrz` is `Eq + Hash`, not `Ord`: a set or map of tiles is a `HashSet`/`HashMap`.

### `Map` Struct

The grid's geometry, holding no tiles. Terrain is `common_bevy::resources::map::Map`, which keeps the tiles and delegates conversion and vertices to a `qrz::Map`.

```rust
pub struct Map {
    radius: f32,       // Hex size in world units, centre to vertex
    rise: f32,         // Vertical scale (z → y)
}
```

**Construction**: `Map::new(radius, rise)` — the game's values are `common::grid::HEX_RADIUS` and `common::grid::RISE`.

**Key Methods**:
- `convert(Qrz) -> Vec3` - Hex to world space
- `convert(Vec3) -> Qrz` - World to hex space (with cube rounding)
- `radius()` / `rise()` - Accessors for the construction parameters
- `vertices(qrz) -> Vec<Vec3>` - **7** positions, `rise` above the centre: 6 corners clockwise `[NE, E, SE, SW, W, NW]`, then the centre
- `face(here, next) -> (Vec2, Vec2)` - The face between neighbours in the ground plane (x, z): unit normal from `here` into `next`, and midpoint
- `exit(from, dir, here) -> (f32, Qrz)` - Where a ground-plane ray leaves `here`: distance along `dir` to the first face, and the neighbour across it. Zero for a point already a hair past a face

### `Convert<T, U>` Trait

Bidirectional conversion between coordinate systems:
```rust
pub trait Convert<T, U> {
    fn convert(&self, it: T) -> U;
}
```

Implemented by `Map` for `Qrz ↔ Vec3` conversions.

## Coordinate Conversion Details

**Affine Transformation**: One forward matrix (Qrz → Vec3) and its inverse (Vec3 → Qrz, then cube rounding), both scaled by `radius`; `z` scales by `rise`.

**Cube Rounding**: Converting Vec3 → Qrz requires rounding to nearest hex:
1. Convert to fractional cube coordinates
2. Round to nearest integer satisfying q+r+s=0
3. Handles edge cases where multiple coordinates need rounding

**Vertex topology**: Corners `i` and `i+1` bound the edge facing `DIRECTIONS[(4 - i) mod 6]` — the corners run clockwise and `DIRECTIONS` counter-clockwise. `common-bevy`'s `surface::CORNER_NEIGHBOURS` and the mesh builder's corner tables are written to this order; `map.rs`'s test `an_edge_faces_its_direction` pins it.

## Common Patterns

### Finding Nearby Hexes
```rust
let origin = Qrz { q: 0, r: 0, z: 0 };
let neighbors = origin.neighbors();  // 6 adjacent hexes
let ring = origin.ring(3);           // the 18 tiles three steps out, in order round
```

### World ↔ Hex Conversion
```rust
let map = Map::new(1.0, 0.8);
let hex = Qrz { q: 1, r: 2, z: 3 };
let world_pos: Vec3 = map.convert(hex);
let recovered: Qrz = map.convert(world_pos);  // Rounds to nearest hex
```

### Distance Queries
```rust
let a = Qrz { q: 0, r: 0, z: 0 };
let b = Qrz { q: 2, r: -1, z: 0 };
let dist = a.flat_distance(&b);  // 2D distance on hex grid
let dist_3d = a.distance(&b);    // Includes elevation
```

## Module Structure

- `qrz.rs` - Core `Qrz` type, `DIRECTIONS`, `hex_distance`, rings, walks and lines
- `map.rs` - `Map` geometry: world space conversion, vertices, faces, exits
- `lib.rs` - Public exports (`Qrz`, `DIRECTIONS`, `hex_distance`, `Map`, `Convert`)

## Usage in Main Codebase

- `Loc` component wraps `Qrz` for entity positions
- `common_bevy::resources::map::Map` holds the terrain and wraps a `qrz::Map` for its geometry
- Physics, movement, pathfinding, chunking and summaries use hex distance
- NNTree uses qrz for spatial queries

## Testing

```bash
cargo test -p qrz
```
