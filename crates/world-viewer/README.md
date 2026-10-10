# world-viewer

Renders the world to an image two ways: the event stack composed from a bird's
eye, or one event's published index on its own. It builds the stack the
server runs, so the same seed gives the same world.

## Run

```bash
cargo run --bin world-viewer -- --layers elevation,thrusting-fronts --radius 5000 --format png --output terrain.png
cargo run --bin world-viewer -- --layers elevation,channels --radius 30000 --scale 16
cargo run --bin world-viewer -- --help
```

Defaults: centre (0, 0), radius 15000 world units, 8 world units per pixel,
QOI to `world.qoi` (`--format png` writes `world.png`), view `elevation`.
Renders made for review land under `proofs/world-viewer/` at the repo root,
ignored by git, the way the assets repo keeps its proof sheets.

## Views

| View | Reads | Draws |
|------|-------|-------|
| `elevation` | composite | the composed surface on the terrain shader's ramp, slope-shaded |
| `plate-edges` | plate index | every edge of the plate graph along its chain, drawn where a tile reads it through the warp, coasts white and interior edges grey, a dot at each seed |
| `boundaries` | motion index | each edge along its chain: hue by regime, width by convergence, a tick toward the plate going under |
| `thrusting-fronts` | motion index | every convergent edge along its chain, ticks onto the overriding plate, longer for a harder edge: what thrusting builds on |
| `drainage-reaches` | drainage index | every reach as its node chain, width by catchment; a channel narrower than a pixel is not drawn |
| `channels` | channel index | every channel as its train across its flow line, paler where the river holds the line, width by catchment |
| `forest` | composite | each tile's cover over what is drawn beneath: the canopy's green by fullness, pine blue-green, deciduous green, scrub olive |
| `stands` | stand index | the density the stands give each position, in four plain bands (open, thin, half, closed) over what is drawn beneath: where the woods are and where they open, without the tiles' own draws; the treeline and the galleries along channels are the tile's, and not in it |

Views stack bottom to top in the order given: fills first, markers as
overdraw.

`--lod <r>` renders the viewport as the client draws a distance band
instead of the views: summaries of radius `r` (the ladder is 1, 4, 13, 40;
0 is the tiles), each the height and water the seven-sample rule selects
from tiles read through the whole stack, on the elevation ramp with water
in blue by depth. It logs what the band cost — the first tile, then the
summaries and their samples, as wall time over every core — and shows what
survives it: a river narrower than the summary vanishes into its valley. The unique summaries are the pixels' count, so
match `--scale` and `--radius` to the band, or a fine band over a wide
viewport is a full-resolution render.

## What a view may read

A view is one of two kinds.

- **A composite view** reads tiles through the composite and nothing else. It
  is the bird's eye: what the stack composes at each tile, every layer's
  contribution in. It pays the whole stack per tile, so it materialises the
  tiles under the pixels in view, once, and never a bounding box of tiles.
- **An index view** reads one event's published index and nothing else, from
  the registry through `Composite::with_indexes` after the tiles under the
  viewport are materialised, because deform is what fills an index.

A view never re-derives an event's quantity, and never calls a layer's own
functions: a field a layer computes is seen only as the tiles it composes
into or the index it publishes (INV-009). A view that recomputes is a second
implementation, and it drifts; a view that reads a layer directly is a
bypass that becomes load-bearing.

A view never reaches into the composite's internals. Cells, caches and deform
order are the framework's; a view sees the registry and the tiles.

## When a view is added

- An event gets a view for each index it publishes, before the event is
  judged. The viewer is how a layer is judged; a field with no index is
  judged through the composite views and the world crate's probes.
- A second view of the same product is added only when it answers a question
  the first cannot. The doc comment on the view names the question.
- A composite view exists per composed tile quantity. Elevation is one; tags
  get one the day a layer writes a tag.
- When a product is deleted, its view is deleted in the same commit.

Not added: a view of a tunable, of an intermediate the event does not expose,
or of anything the game never reads.

## Colour

- `elevation` uses the terrain shader's ramp, stop for stop, so the viewer and
  the game read a height the same way. Change one and change the other.
- One hue per quantity. Where a quantity has a sign, hue carries the sign and
  saturation or width carries the magnitude, so a chain of boundaries reads as
  one sign along its length.
- Markers and lines use hues no fill uses, so overdraw reads as overdraw.

## The stack

The stack is built here and in the server's registry, in the same order. A
layer added to one is added to the other in the same commit, or the composite
views show a world the server does not generate.
