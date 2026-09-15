# texgen

Read before adding or revising a texture.

## Constraints

- **Every primitive wraps.** Noise takes unit coordinates and a cell count and
  repeats at the tile edge; `Canvas` wraps every pixel write. Nothing draws
  through raw indices, so a texture cannot fail to tile except by a bug in
  those two modules.
- **Unit coordinates, not pixels.** A texture written against `u, v` in
  [0, 1) renders the same at any `--size`; only the fixed pixel-radius passes
  (equalize, grain) read the size.
- **Linear RGB inside, sRGB at the PNG.** Pick colors with `color::srgb` or
  copy a linear stop from `terrain.wgsl`; never hand-write sRGB values into an
  `Rgb`.
- **The module is the source.** The PNG is its output, committed so the
  client runs without a generation step. The asset is `VARIANTS` seeds
  from 0, stacked vertically, at the default size; the client loads the
  stack as a texture array and blends the layers by world position so the
  repeat never lines up. Other seeds are for exploration.
- **No detached shadows.** The engine lights the surface. Shading that stays
  inside a feature's own outline is fine; a shadow cast beyond it onto the
  ground detaches the feature, and a field of detached stones reads as
  bubbles on water or water-sorted gravel.
- **Pin the mean to a ramp stop only where the shader blends the tile with
  the ramp.** A tile that stands on its own is distinctive first.
- **Equalize before grain.** Equalize flattens luminance drift that shows as
  a light/dark checker once tiled; grain after it keeps the per-pixel noise
  equalize would otherwise smooth.

## Making one: creator and critics

A texture is done when two critics pass it, not when it looks fine to the one
who drew it. The `/texgen` skill drives the loop; the two critics are one run
of the `critic-round` workflow, which runs them in parallel on paths, never
images, with the Read tool only, since anything else holds the file against
the next build.

1. **Creator.** Write or revise the module, register it, run
   `cargo run --bin texgen -- <name>`. The seam check must print `tiles`.
   Look at both PNGs before asking anyone else to.
2. **Comparison critic.** Given the name, its `brief` from the registry,
   and the tile and the proof sheet. It answers: does
   the tile read as the brief; are there seams, banding, or an obvious repeat
   on the sheet; does the feature scale read as ground seen from a few metres
   up. PASS, or FAIL with at most three defects, worst first.
3. **Blind critic.** Given a copy of the tile under a neutral
   name (`tile.png` in the scratchpad) and nothing else: no texture name, no
   brief. The file name is context, so the real path never goes to it. It
   says in one line what material this is, then any defects it sees. It
   passes when its material matches the brief.
4. Fix the defects and return to 1. Cap at four rounds; past that, stop and
   put the remaining defects to the user rather than sanding forever.

Wait for the workflow's result; never poll for it with sleeps or timers,
which fire their own completions long after the answer and outlive the
round.
