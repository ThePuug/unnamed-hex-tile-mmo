# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play
Currently not alot to do, but getting a base down to build on
- `<ArrowUp> <ArrowLeft> <ArrowRight> <ArrowDown>` to move
- `<KeyQ>` to spawn a curious dog
- `<Num0>` to jump
- `<KeyG>` to toggle debug grid overlay
 
# Game features
## things you care about
- sun/moon/season cycles
- hexagonal movement
- perlin noise terrain generation
- organic terrain slopes (tiles slope toward neighbors at different elevations)

## things i care about
- authoritative server
- R*-tree backed spatial index
- isolate streaming input from input on global cooldown
- generated mesh terrain
- pathfinding behaviour using A* algorithm