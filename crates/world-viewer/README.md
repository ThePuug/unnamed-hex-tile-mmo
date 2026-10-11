# world-viewer

Renders the world to an image three ways: the event stack composed from a
bird's eye, one event's published index on its own, or one event's own
output at each tile. It builds the stack the server runs, so the same seed
gives the same world, and logs what each layer of it cost.

## Run

```bash
cargo run --bin world-viewer -- --layers elevation,thrusting-fronts --radius 5000 --format png --output terrain.png
cargo run --bin world-viewer -- --layers elevation,channels --radius 30000 --scale 16
cargo run --bin world-viewer -- --layers hillshade,contribution --event dissection --radius 1500 --scale 2
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
| `hillshade` | composite | the composed surface in grey, lit from the north-west at true scale: the cuts a few z deep that the ramp shows as one colour |
| `contribution` | one layer's output | the height `--event`'s query adds at each tile over what is drawn beneath, red where it raises and blue where it cuts, by the square root of its share of the largest in view |
| `plate-edges` | plate index | every edge of the plate graph along its chain, drawn where a tile reads it through the warp, coasts white and interior edges grey, a dot at each seed |
| `boundaries` | motion index | each edge along its chain: hue by regime, width by convergence, a tick toward the plate going under |
| `thrusting-fronts` | motion index | every convergent edge along its chain, ticks onto the overriding plate, longer for a harder edge: what thrusting builds on |
| `drainage-reaches` | drainage index | every node's link to the node its channel runs on to, width by catchment; a channel narrower than a pixel is not drawn |
| `channels` | channel index | every channel as its train across its flow line, paler where the river holds the line, width by catchment |
| `forest` | composite | each tile's cover over what is drawn beneath: the canopy's green by fullness, pine blue-green, deciduous green, scrub olive |
| `stands` | stand index | the density the stands give each position, in four plain bands (open, thin, half, closed) over what is drawn beneath: where the woods are and where they open, without the tiles' own draws; the treeline and the galleries along channels are the tile's, and not in it |

Views stack bottom to top in the order given: fills first, markers as
overdraw.

Every render logs what each layer cost making its tiles: how many times its
deform, prepare and query ran, the time in its own code in each, and the
cells of the layers beneath that its reads deformed, with the time those
took. A layer's cells no layer pulled were deformed by the tiles themselves.
The first tile in a fresh stack pays for every cell under it, so a viewport
of a few pixels reads what one first tile costs.

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

A view is one of three kinds.

- **A composite view** reads tiles through the composite and nothing else. It
  is the bird's eye: what the stack composes at each tile, every layer's
  contribution in. It pays the whole stack per tile, so it materialises the
  tiles under the pixels in view, once, and never a bounding box of tiles.
- **An index view** reads one event's published index and nothing else, from
  the registry through `Composite::with_indexes` after the tiles under the
  viewport are materialised, because deform is what fills an index.
- **A layer view** reads one event's output at each tile, through
  `Composite::outputs_at`: what the event's query returned there, before the
  composite folded it in. It materialises the tiles as a composite view does,
  and the composed tile comes with the outputs.

A view never re-derives an event's quantity, and never calls a layer's own
functions: a field a layer computes is seen only as the tiles it composes
into, its query's output at a tile, or the index it publishes (INV-009). A view that recomputes is a second
implementation, and it drifts; a view that reads a layer directly is a
bypass that becomes load-bearing.

A view never reaches into the composite's internals. Cells, caches and deform
order are the framework's; a view sees the registry, the tiles, each layer's
output at a tile, and the cost the composite reports.

## When a view is added

- An event gets a view for each index it publishes, before the event is
  judged. The viewer is how a layer is judged; a field with no index is
  judged through its own output in a layer view.
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
