# world-viewer

Renders the world to an image two ways: the event stack composed from a bird's
eye, or one event's own product, a field or an index, on its own. It builds
the stack the server runs, so the same seed gives the same world.

## Run

```bash
cargo run --bin world-viewer -- --layers plates,elevation,boundaries --radius 5000 --format png --output terrain.png
cargo run --bin world-viewer -- --layers tilt --radius 30000 --scale 16
cargo run --bin world-viewer -- --help
```

Defaults: centre (0, 0), radius 15000 world units, 8 world units per pixel,
QOI to `world.qoi` (`--format png` writes `world.png`), views
`plates,elevation`.

## Views

| View | Reads | Draws |
|------|-------|-------|
| `plates` | plate field | the substrate on its own ramp, sea level the only edge in it |
| `elevation` | composite | the composed surface on the terrain shader's ramp, slope-shaded |
| `centroids` | plate index | macro plate centroids |
| `boundaries` | motion index | each boundary as its Voronoi edge: hue by regime, width by convergence, a tick toward vergence |
| `tilt` | tilt field | the potential as a diverging ramp, arrows downslope |
| `orogen-field` | orogen field | the hillshaded relief |
| `orogen-belts` | orogen field | the belt mask over the coastline |
| `orogen-section` | orogen field | a profile through the centre, cut across the belt axis |

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
section view takes the belt axis from the orogen's own crest lookup. A view
that recomputes is a second implementation, and it drifts.

A view never reaches into the composite's internals. Cells, caches and deform
order are the framework's; a view sees the registry and the tiles.

## When a view is added

- An event gets a view for each product it publishes, before the event is
  judged. The viewer is how a layer is judged.
- A second view of the same product is added only when it answers a question
  the first cannot. Shape, profile and coverage are three questions about the
  orogen field and get three views. The doc comment on the view names the
  question.
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
