# qrz Library

Hexagonal coordinate system library — 3D hex grid math, world space conversion, and orientation-aware rendering. This is the coordinate reference, not game design or architecture.

## When to Read This

- Working with hex coordinates or grid navigation
- Implementing new movement or pathfinding features
- Converting between hex tiles and world positions
- Adding grid-based features (FOV, line of sight, etc.)
- Debugging position or distance calculations

## Core Concepts

### Axial Coordinates (q, r, z)

The library uses **axial coordinates** for hexagonal grids:
- **q**: East-west axis
- **r**: Northeast-southwest axis
- **s**: Derived axis (s = -q - r) - southeast-northwest
- **z**: Vertical elevation

**Invariant**: `q + r + s = 0` (automatically maintained)

### Hex Orientation

The library supports two orientations via `HexOrientation`:

- **FlatTop** (default): Flat edge north/south, vertices east/west. `convert()` uses `x = 3/2*q, z = √3/2*q + √3*r`.
- **PointyTop**: Vertex pointing north. Legacy orientation with context-sensitive diagonal input.

FlatTop eliminates directional ambiguity — Up maps directly to N (flat edge), so arrow keys need no heading context.

**Heading angles** (flat-top compass): N=0°, NE=60°, SE=120°, S=180°, SW=240°, NW=300°. `From<Heading> for Quat` converts to Y-rotation via `quat_angle = 2π - compass`. Targeting's `to_angle()` and `angle_between_locs()` use +90° offset for flat-top geometry.

### Distance Metrics

**`flat_distance(other)`**: 2D hex distance ignoring elevation
- Maximum of absolute differences in cube coordinates (q, r, s)
- Correct distance for hex grid topology
- Example: Adjacent hexes are distance 1

**`distance(other)`**: 3D distance including elevation
- Adds absolute z difference to flat_distance
- Use for queries that care about height

## Key Types

### `Qrz` Struct
```rust
pub struct Qrz {
    pub q: i16,
    pub r: i16,
    pub z: i16,
}
```

**Constants**:
- `Qrz::Q` - Unit vector in q direction (1,0,0)
- `Qrz::R` - Unit vector in r direction (0,1,0)
- `Qrz::Z` - Unit vector in z direction (0,0,1)
- `DIRECTIONS` - Array of 6 cardinal hex directions

**Key Methods**:
- `neighbors()` - Returns all 6 adjacent hexes (same z)
- `normalize()` - Returns unit direction vector
- `arc(dir, radius)` - Returns hexes in 120° arc at distance
- `fov(dir, dist)` - Returns all hexes in cone (field of view)

**Arithmetic**: Supports `+`, `-`, `*` (scalar multiply)

### `Map<T>` Struct

Generic container for storing data at hex coordinates with world space conversion.

```rust
pub struct Map<T> {
    radius: f32,       // Hex size in world units
    rise: f32,         // Vertical scale (z → y)
    orientation: HexOrientation,
    // ... internal storage
}
```

**Construction**: `Map::new(radius, rise, orientation)`

**Key Methods**:
- `convert(Qrz) -> Vec3` - Hex to world space (orientation-aware)
- `convert(Vec3) -> Qrz` - World to hex space (with rounding)
- `orientation()` - Returns the map's hex orientation
- `insert(qrz, value)` - Store tile data
- `get(qrz)` - Retrieve tile data
- `iter()` - Iterate tiles in sorted order
- `nearest_below(pos, range)` - Find closest tile below position
- `neighbors(qrz)` - Get data from adjacent hexes
- `line_between(start, end)` - Get hexes along line
- `vertices(center, radius)` - Generate 6 vertex positions (orientation-correct)

### `Convert<T, U>` Trait

Bidirectional conversion between coordinate systems:
```rust
pub trait Convert<T, U> {
    fn convert(&self, it: T) -> U;
}
```

Implemented by `Map` for `Qrz ↔ Vec3` conversions.

## Coordinate Conversion Details

**Affine Transformation**: Orientation-specific matrices.
- Forward: Qrz → Vec3 (hex to world)
- Inverse: Vec3 → Qrz (world to hex, with cube rounding)

**Cube Rounding**: Converting Vec3 → Qrz requires rounding to nearest hex:
1. Convert to fractional cube coordinates
2. Round to nearest integer satisfying q+r+s=0
3. Handles edge cases where multiple coordinates need rounding

**Vertex topology**: Vertex index ordering is identical for both orientations — direction_to_vertices, skirt mapping, and slope code all work unchanged.

## Common Patterns

### Finding Nearby Hexes
```rust
let origin = Qrz { q: 0, r: 0, z: 0 };
let neighbors = origin.neighbors();  // 6 adjacent hexes
let fov = origin.fov(&Qrz::Q, 5);   // Cone in Q direction, distance 5
```

### World ↔ Hex Conversion
```rust
let map: Map<EntityType> = Map::new(1.0, 0.8, HexOrientation::FlatTop);
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

- `qrz.rs` - Core `Qrz` type and hex grid math
- `map.rs` - `Map<T>` storage, world space conversion, orientation-aware vertex generation
- `lib.rs` - Public exports (`Qrz`, `Map`, `HexOrientation`, `Convert`)

## Usage in Main Codebase

The qrz library is used throughout:
- `Loc` component wraps `Qrz` for entity positions
- `Map<EntityType>` resource stores terrain tiles
- Physics and movement systems use hex distance
- Pathfinding operates on hex coordinates
- NNTree uses qrz for spatial queries
- Input system branches on `map.orientation()` for arrow key mapping

## Testing

```bash
cargo test -p qrz
```

Tests are parameterized over both orientations for roundtrip conversion, vertex shape, and origin.
