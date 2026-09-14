# texgen

Draws the tileable textures under `assets/textures`. Each texture is a module
that builds one image from a seed on a canvas whose coordinates wrap, so it
tiles by construction; every run measures the seam and fails past the limit.

## Run

```bash
cargo run --bin texgen -- grass-plain
cargo run --bin texgen -- all --size 512
cargo run --bin texgen -- --list
cargo test --bin texgen
```

Writes `assets/textures/<name>.png` and a 2x2 proof sheet at
`target/texgen/<name>.tiled.png`. Defaults: 256 px, seed 0. The shipped asset
is seed 0 at the default size; regenerate it after changing a module.

## Textures

| Texture | Reads as | Layers |
|---------|----------|--------|
| `grass-plain` | lowland turf averaging to the terrain ramp's plain green | growth drift, bare earth, blade strokes, clump mounds, equalize, mean pin, grain |
