# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play

An early prototype testing a reaction-based combat system for a future MMO. The core loop is playable - explore a procedural hex world, engage enemies, and manage incoming attacks through a visible threat queue.

## What's Actually Working

**The Reaction Queue** (the unique hook)
You see incoming attacks before they hit. Circular timers appear above your health bar showing when each attack will land. This creates a decision window: do you spend stamina to clear threats defensively, or tank the damage and counter-attack while your enemy is vulnerable?

Both you and enemies can die simultaneously if attacks are in-flight. Stamina management matters - run dry and you can't react.

**Combat That Feels Responsive**
Movement uses client-side prediction so there's no perceived lag. Arrow keys move you on the hex grid and set your facing direction. Enemies in front of you are automatically targeted. The combat HUD shows ability cooldowns, resource bars, and those critical threat timers.

You have abilities - a gap closer, a heavy hit, a counter-punch that removes specific threats, and a panic button that clears everything. Enemies include melee Wild Dogs and ranged Forest Sprites that force you to close distance or kite.

**A Procedural World**
Hex-based terrain generation with organic slopes, day/night cycles, and streaming chunks. Exploration reveals the map as you move. The world feels bigger than it is.

## What to Expect

This is a combat prototype, not a full game. You spawn, enemies aggro, you fight. Death respawns you nearby with no penalty. There's no progression system, no gear to find, no quests.

The question being tested: **Does seeing attacks coming and choosing how to respond create interesting moment-to-moment decisions?** If you find yourself thinking "should I deflect now or save stamina?" then it's working.

Build with `cargo build`, then run `cargo run --bin server` and `cargo run --bin client` in separate terminals.

# Technical Notes

**What's Built So Far:**
- Reaction-based combat with visible threat timers
- Responsive movement (client-side prediction eliminates lag feel)
- Directional targeting system (face enemies to target them)
- Combat HUD with ability tracking and resource management
- Enemy AI (Wild Dogs that pursue and attack, Forest Sprites that kite and shoot)
- Procedural hex-based terrain with day/night cycles
- Networked client-server architecture

**Design Target:**
Eventually MMO-scale (1000+ concurrent players, large shared world). Current prototype validates core combat mechanics before scaling up. Built on authoritative server architecture with client prediction, ECS (Bevy engine), and custom hex coordinate system (`qrz` library).