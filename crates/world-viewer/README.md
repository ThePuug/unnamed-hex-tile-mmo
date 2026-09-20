# world-viewer

Renders the world to an image two ways: the event stack composed from a bird's
eye, or one event's own product, a field or an index, on its own. It builds
the stack the server runs, so the same seed gives the same world.

## Run

```bash
cargo run --bin world-viewer -- --layers plates,elevation,thrusting-fronts --radius 5000 --format png --output terrain.png
cargo run --bin world-viewer -- --layers tilt --radius 30000 --scale 16
cargo run --bin world-viewer -- --help
```

Defaults: centre (0, 0), radius 15000 world units, 8 world units per pixel,
QOI to `world.qoi` (`--format png` writes `world.png`), views
`plates,elevation`.

## Views

| View | Reads | Draws |
|------|-------|-------|
| `plates` | plate index | the substrate on its own ramp, from the coasts under the viewport; sea level the only edge in it |
| `age` | plate field | each plate's age as a grey ramp, black new, white aged: which plates keep their lakes and which are drained |
| `elevation` | composite | the composed surface on the terrain shader's ramp, slope-shaded |
| `plate-edges` | plate index | every edge of the plate graph along its chain, coasts white and interior edges grey, a dot at each seed |
| `boundaries` | motion index | each edge along its chain: hue by regime, width by convergence, a tick toward the plate going under |
| `tilt` | tilt field | the potential as a diverging ramp, arrows downslope |
| `thickening-field` | thickening field | the plateau on the substrate, hillshaded; builds the coasts and plate outlines under the viewport itself |
| `lithology-field` | lithology field | the rock at the surface by kind, shale grey, sandstone tan, limestone pale, basement red, the cuestas hillshaded; logs the shares of the land in view |
| `dissection-field` | dissection field | the cut on its own, hillshaded, every valley a depression in a flat sheet; routes the drainage cells under the viewport itself |
| `water-field` | dissection field | the dissected ground hillshaded, and every surface standing over it in blue, darker with depth: the sea, the lakes, the channels; rounded to steps as a tile reads it |
| `thrusting-fronts` | motion index | every convergent edge along its chain, ticks onto the overriding plate, longer for a harder edge: what thrusting builds on |
| `drainage-reaches` | drainage index | every reach as its node chain, width by catchment; a channel narrower than a pixel is not drawn |
| `drainage-lakes` | drainage index | flooded nodes at their surface, a white dot at each outlet |
| `channels` | channel index | every channel as its train across its flow line, paler where the river holds the line, width by catchment |

Views stack bottom to top in the order given: fills first, markers as
overdraw. A field view paints every pixel and cannot stack; asking for one
with anything else is an error, never a silent drop.

## What a view may read

A view is one of two kinds.

- **A composite view** reads tiles through the composite and nothing else. It
  is the bird's eye: what the stack composes at each tile, every layer's
  contribution in. It pays the whole stack per tile, so it materialises the
  tiles under the pixels in view, once, and never a bounding box of tiles.
- **An event view** reads one event's product and nothing else. An index view
  reads the index from the registry after the tiles under the viewport are
  materialised, because deform is what fills an index. A field view calls the
  event's own functions at each pixel; nothing is materialised.

A view never re-derives an event's quantity. If a view needs a number the
event computes, the event exposes the function and the view calls it: the
thickening view paints the field through the event's own function. A view
that recomputes is a second implementation, and it drifts.

A view never reaches into the composite's internals. Cells, caches and deform
order are the framework's; a view sees the registry and the tiles.

## When a view is added

- An event gets a view for each product it publishes, before the event is
  judged. The viewer is how a layer is judged.
- A second view of the same product is added only when it answers a question
  the first cannot. Reaches and lakes are two questions about the drainage
  index and get two views. The doc comment on the view names the question.
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
