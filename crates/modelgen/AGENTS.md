# modelgen

Read before adding or revising a model.

## Constraints

- **Data API, never operators.** A build runs headless, in a live session,
  and inside the tests, and must come out identical each time. `bpy.ops`
  reads the context — active object, mode, selection — so it depends on
  what ran before; bmesh and `bpy.data` do not. `mesh.py` wraps what a
  model needs; add to it rather than reaching for an operator.
- **Seeded noise, never `random`.** `noise.py` is a function of position
  and seed. `random` and `mathutils.noise.random` are not, and a model
  using them differs between runs and fails the test.
- **The unit is the hex radius.** One Blender unit is one world unit; the
  client scales a scene by `map.radius()`. A prop on a tile top fits inside
  the hex's flats, 1.73 across, and its `box` in the registry says so.
- **Ground at z = 0, front at +Y.** The check requires the lowest vertex on
  the ground. The export is Y-up, so Blender +Y becomes the client's -Z,
  which a `Heading` of `North` faces with no rotation.
- **The module is the source.** The GLB is its output, committed so the
  client runs without Blender. The asset is `VARIANTS` seeds from 0, one
  glTF scene each in seed order, so the client can load `Scene(k)` by world
  position. Other seeds are for exploration.
- **One flat colour per material, linear RGB.** No textures: the client
  lights the surface. Pick colours with `mesh.srgb` or copy a linear stop
  from `terrain.wgsl` or a texgen module, so a prop matches the ground it
  stands on.
- **Facets, not smoothing.** Flat-shaded faces are the look; a smooth-shaded
  low-poly mesh reads as a blob.
- Actors are unbuilt. The client plays glTF animation 1 as idle and 2 as
  walk, so an actor module must export three actions sorting into that
  order; nothing checks it yet.

## Making one: creator and critics

A model is done when two critics pass it, not when it looks fine to the one
who built it. The `/modelgen` skill drives the loop; the two critics are one
run of the `critic-round` workflow, which runs them in parallel on paths,
never images, with the Read tool only, since anything else holds the file
against the next build.

1. **Creator.** Write or revise the module, register it, run
   `cargo run --bin modelgen -- <name>`. Every seed must print `fits`.
   Look at the sheets before asking anyone else to.
2. **Comparison critic.** Given the name, its `brief` from the registry,
   the sheet's layout (front, left, back, game view; the hex plate is one
   tile, a player about two units), and the seed sheets. It answers: does
   the model read as the brief; is its silhouette distinct from the three
   views; does it sit on the tile at the right scale; do the seeds differ
   without one being wrong. PASS, or FAIL with at most three defects, worst
   first.
3. **Blind critic.** Given a copy of one sheet under a neutral
   name (`sheet.png` in the scratchpad), the layout, and nothing else: no
   model name, no brief. The file name is context, so the real path never
   goes to it. It says in one line what object this is, then any defects
   it sees. It passes when its object matches the brief.
4. Fix the defects and return to 1. Cap at four rounds; past that, stop and
   put the remaining defects to the user rather than sanding forever.

Wait for the workflow's result; never poll for it with sleeps or timers,
which fire their own completions long after the answer and outlive the
round.

### Following in a live Blender

The build never needs the GUI. With Blender open and the MCP extension
listening, every build pushes its first seed into the open scene through
that socket, so the person watching sees each round as it lands. The
same rebuild runs by hand from `execute_blender_code`, which is faster
than the command for trying a shape:

```python
import sys; sys.path.insert(0, r"C:\...\unnamed-hex-tile-mmo\crates\modelgen\blender")
import modelgen; result = modelgen.live("boulder", seed=0)
```

`live` reloads the package, clears the current scene, builds the model
there, and returns the check report, so edit the module on disk and call
it again; `get_screenshot_of_area_as_image` or `render_thumbnail_to_path`
shows the result. The live scene is scratch: nothing done to it by hand
survives, so a change worth keeping goes into the module and through the
build.
