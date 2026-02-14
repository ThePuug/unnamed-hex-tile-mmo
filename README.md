# Game Thesis

**A world that fights back — you don't beat it, you survive it, together.**

This thesis sits above everything: combat is *how* you survive it, the transforming world is *what* you're surviving, cities are *where* you survive it, and "together" is *why* it's an MMO.

## The Three Pillars

**Combat:** *Combat that feels like reading a fight, not executing a rotation.*

Combat is reading because the world is a worthy opponent you need to understand. You see attacks coming and decide how to respond in the moment.

**World:** *A living world that transforms itself, not one stuck in static repetition.*

The world transforms because it's alive and fighting. Exploration drives discovery, and the further you venture from safety, the harder it pushes back.

**Cities:** *Build anywhere, but beware a world that wants to reclaim it.*

Cities are contested because survival isn't guaranteed. Players create safe spaces together, but the world pushes back.

---

# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play

An early prototype testing a reaction-based combat system for a future MMO. The core loop is playable - explore a procedural hex world, **pick your difficulty by how far you venture**, engage enemies, and manage incoming attacks through a visible threat queue.

## What's Actually Working

**Pick Your Challenge**
You spawn at a safe haven. Walk 100 tiles north and enemies are level 1. Walk 500 tiles and they're level 5. Go 1000+ tiles and face level 10 threats. **Your distance from spawn determines difficulty** - new players stay close, confident players push further. The UI shows exactly where you are: "Haven: 437 tiles | Zone: North | Enemy Lv. 4".

Direction matters too. Each compass direction spawns different enemy archetypes with unique tactics:
- **North: Berserkers** - Aggressive gap-closers that hit hard and fast
- **East: Juggernauts** - Tanky brutes that soak damage and pressure you
- **South: Kiters** - Ranged harassers that maintain distance
- **West: Defenders** - Reactive counter-attackers that punish your aggression

**The Reaction Queue** (the unique hook)
You see incoming attacks before they hit. Circular timers appear above your health bar showing when each attack will land. This creates a decision window: do you spend stamina to clear threats defensively, or tank the damage and counter-attack while your enemy is vulnerable?

Both you and enemies can die simultaneously if attacks are in-flight. Stamina management matters - run dry and you can't react. Enemy level hexagons color-code relative difficulty (gray = trivial, green = easy, yellow = fair fight, red = dangerous).

**Combat That Feels Responsive**
Movement uses client-side prediction so there's no perceived lag. Arrow keys move you on the hex grid and set your facing direction. Enemies in front of you are automatically targeted. The combat HUD shows ability cooldowns, resource bars, and those critical threat timers.

You have abilities - a gap closer (Lunge), a heavy hit (Overpower), a reactive Counter that reflects damage, and a panic button (Deflect) that clears everything. Enemies vary by archetype: melee chargers, tanky juggernauts, ranged kiters, and defensive counters.

**A Living World**
Hex-based terrain generation with organic slopes, day/night cycles, and streaming chunks. **Enemies spawn dynamically as you explore** - no static spawn camps to farm. Venture into uncharted territory and engagements appear. Abandon an area and it despawns after 30 seconds. Exploration drives content discovery.

## What to Expect

This is a combat prototype, not a full game. You spawn at the haven, pick a direction and distance based on how much challenge you want, then fight dynamic engagements. Death respawns you at the haven with no penalty. There's no progression system, no gear to find, no quests yet.

**The questions being tested:**
1. **Does seeing attacks coming and choosing how to respond create interesting moment-to-moment decisions?** If you find yourself thinking "should I deflect now or save stamina?" then it's working.
2. **Does self-directed difficulty feel good?** Can you find the "sweet spot" distance where fights are exciting but winnable? Or does pushing further into dangerous territory scratch that risk/reward itch?
3. **Do different enemy archetypes force tactical adaptation?** Does fighting a Kiter feel different than fighting a Berserker?

Try this: Start safe (100-200 tiles). When it feels easy, push to 400-500 tiles. When that's comfortable, venture to 700+ and see how long you survive. Each compass direction plays differently - find your favorite archetype to fight.

Build with `cargo build`, then run `cargo run --bin server` and `cargo run --bin client` in separate terminals.

# Technical Notes

**What's Built So Far:**
- Reaction-based combat with visible threat timers
- Responsive movement (client-side prediction eliminates lag feel)
- **Distance-based difficulty scaling** (0-10 levels, 100 tiles per level)
- **Four enemy archetypes** with distinct combat profiles (Berserker/Juggernaut/Kiter/Defender)
- **Dynamic engagement spawning** (enemies appear as you explore new areas)
- **Spatial difficulty UI** (distance indicator, color-coded enemy levels)
- Directional targeting system (face enemies to target them)
- Combat HUD with ability tracking and resource management
- Enemy AI behaviors (Chase for melee, Kite for ranged)
- Procedural hex-based terrain with day/night cycles
- Networked client-server architecture

**Design Target:**
Eventually MMO-scale (1000+ concurrent players, large shared world). Current prototype validates core combat mechanics before scaling up. Built on authoritative server architecture with client prediction, ECS (Bevy engine), and custom hex coordinate system (`qrz` library).