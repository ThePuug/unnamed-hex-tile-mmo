@AGENTS.md

Project context, conventions, and comment style live in `AGENTS.md`. Per-crate
guidance sits alongside the crate it governs — `crates/qrz/AGENTS.md` for the
hex coordinate system.

Design specs live in the `unnamed-indie-studio-internal` repo, sibling checkout,
`projects/unnamed-hex-tile-mmo/`:

| Location | Purpose |
|----------|---------|
| `design.md` | Game pitch / north-star |
| `design/` | Technical specs, one per system |
| `networking.md` | Transport, movement sync, visual interpolation |

Check the relevant spec before changing a system, and update it there when
behavior changes.
