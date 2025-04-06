# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play
Currently not alot to do, but getting a base down to build on
- `<ArrowUp> <ArrowLeft> <ArrowRight> <ArrowDown>` to move
- `<Num0>` to jump

# Game features
## things you care about
- sun/moon/season cycles
- hexagonal movement
- perlin noise terrain generation

## things i care about
- authoritative server
- spatial indexing using KdTree
- isolate streaming input from input on global cooldown
