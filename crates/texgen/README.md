# texgen

Draws the tileable textures under `assets/textures`. Each texture is a module
that builds one image from a seed on a canvas whose coordinates wrap, so it
tiles by construction; every run measures the seam and fails past the limit.

## Run

```bash
cargo run --bin texgen -- grass-plain
cargo run --bin texgen -- all --size 512
cargo run --bin texgen -- grass-plain --seed 7 --variants 1 --out target/texgen   # one tile, for exploration
cargo run --bin texgen -- --list
cargo test --bin texgen
```

Writes `assets/textures/<name>.png`, three seeds of the tile stacked
vertically for the client to load as a texture array, and each seed's tile
and 2x2 proof sheet at `target/texgen/<name>-<seed>.png` and
`<name>-<seed>.tiled.png`. Defaults: 256 px, seeds 0 to 2. Regenerate the
asset after changing a module.

## Textures

| Texture | Reads as | Layers |
|---------|----------|--------|
| `grass-plain` | lowland turf averaging to the terrain ramp's plain green | growth drift, bare earth, blade strokes, clump mounds, equalize, mean pin, grain |
| `cliff-stone` | fractured stone averaging to the shader's cliff grey, up is up on the face | a few large slabs from a Voronoi taller than wide, tall shards from a finer one inside shattered slabs, edges with their own width and depth that fade into solid rock, tone and level drifting across the face, leaning top-lit faces, facets, ledges that pinch out with a lit top and shadow and drip streaks below, spalls, mineral flecks, moss in master-crack channels and ledge shadow, hairline fractures, grit, equalize, mean pin, grain |
| `mountain-scree` | exposed mountain ground seen from above, dark ochre earth covered in pebbles; not pinned to a ramp stop | mottled earth with dust and damp patches, clods and grit, flat bedrock patches with a crisp ragged outline shaded within it and crusty lichen spots, polygonal pebbles at two sizes shaded within their outlines, equalize, grain |
