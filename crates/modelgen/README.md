# modelgen

Builds the low-poly models under `assets/models`. Each model is a Python
module that constructs one object from a seed inside Blender, checks it
against its box and triangle budget, and exports it; the Rust binary only
finds Blender and runs the package headless.

## Run

```bash
cargo run --bin modelgen -- boulder
cargo run --bin modelgen -- all --size 256
cargo run --bin modelgen -- boulder --seed 7 --variants 1 --out target/modelgen   # one seed, for exploration
cargo run --bin modelgen -- --list
cargo test -p modelgen                # skips where Blender is absent
```

Needs Blender 5.2: `$BLENDER`, the default install path, or `blender` on
`PATH`. Writes `assets/models/<name>.glb`, three seeds as glTF scenes 0 to
2 for the client to pick by world position, and each seed's proof sheet at
`target/modelgen/<name>-<seed>.png`: front, left side, back, and the game's
own angle, on a hex of the tile's radius. Defaults: 512 px views, seeds 0
to 2. Regenerate the asset after changing a module. When a Blender with the
MCP extension is listening on port 9877 (`BLENDER_MCP_PORT` overrides), every
build also rebuilds the first seed in its open scene, so the viewport follows
the command line.

Layout: `src/main.rs` is the launcher; `blender/main.py` the command line
Blender runs; `blender/modelgen/` the package — `mesh`, `noise`, `scene`,
`check`, `proof`, `export`, `follow`, and one module per model under
`models/`.

## Models

| Model | Reads as | Parts |
|-------|----------|-------|
| `boulder` | one cliff-grey boulder settled into a tile top, lichen on its crown | icosphere squashed wider than tall and leaned, three scales of noise swelling, a tilted top and two side faces cleaved flat, cut off at its widest section so the base is the widest part, flat facets, lichen on up-facing faces where a noise patch and height agree |
| `pine-tree` | one dark pine two to three times a person's height, standing on a tile | six-sided tapered trunk, five or six short cone tiers overlapping so each hides the point below, every rim vertex pushed in or out and drooped by its own hash, every tier turned to its own phase, two alternating needle greens, a slight lean |
