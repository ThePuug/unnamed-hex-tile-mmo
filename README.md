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
You spawn at a safe haven. Every 400 tiles out from it, the dens you meet stand a level deeper - but never within five levels of your own, so a fresh character meets the weakest packs wherever it walks, and a levelled one picks its challenge by how far it goes. **Your distance from the haven determines difficulty** - new players stay close, confident players push further. The UI shows how far you are: "Haven: 437 tiles".

Ground matters too. Dens stand on the sites the world makes for them, and each kind of ground raises its own archetype, built on one attribute with a signature skill:
- **Berserkers** (open ground) - Wild dogs whose Frenzy bites in a burst
- **Juggernauts** (rock) - Heavy brutes whose Overpower is one hard blow
- **Flankers** (woods) - Forest sprites whose Perfect Stride strikes on the run
- **Defenders** (ranges) - Reactive fighters whose Counter sends a share of what you throw back at you
- **Skirmishers** (scrub) - Leapers that clear your reach, or dive onto you from beyond it
- **Ambushers** (rivers) - Opportunists whose Punish lands harder while you are overcommitted

**The Reaction Queue** (the unique hook)
You see incoming attacks before they hit. Timers above your health bar show when each attack will land. This creates a decision window: do you spend endurance on a Parry or a Counter to clear the threats about to land, or take the damage and strike while your enemy is recovering? A reaction clears what is in its band at the hit line and nothing else, so timing is the skill.

Both you and enemies can die simultaneously if attacks are in-flight. Endurance refuses nothing, but everything spent tires you: a tired fighter recovers slower and gets less warning of each threat. Enemy level hexagons color-code relative difficulty (gray = trivial, green = easy, yellow = fair fight, red = dangerous).

**Combat That Feels Responsive**
Movement uses client-side prediction so there's no perceived lag. Arrow keys move you on the hex grid and turn your heading. Enemies within the three faces ahead of you are targeted automatically. The combat HUD shows each skill's recovery, resource bars, and those critical threat timers.

Your skills sit on Q W E R A S D F: four strikes - Frenzy, Feint, Overpower, Punish - two reactions, Parry and Counter, a Leap clear of a target in reach or onto one beyond it, and Perfect Stride, which lets you strike past the forward faces on the run. Every enemy carries its archetype's own skill with a Feint and a Parry.

**A Living World**
Hex-based terrain generation with organic slopes, rivers and sea, forests and outcrops, a day/night cycle, and streaming chunks. Trees can be felled and boulders mined (G) into a bag that weighs what it carries. **Dens are raised as you explore** - a site near you with no den gets one as you approach, and a pack you leave unwatched for 30 seconds stands down and comes back whole when you return. A den you clear lies cleared for ten minutes before the world takes its site back. Exploration drives content discovery.

## What to Expect

This is a combat prototype, not a full game. You spawn at the haven, pick a direction and distance based on how much challenge you want, then fight the dens you find. Death respawns you at the haven with no penalty. There's no gear to find and no quests yet.

**The questions being tested:**
1. **Does seeing attacks coming and choosing how to respond create interesting moment-to-moment decisions?** If you find yourself thinking "should I parry now or save my endurance?" then it's working.
2. **Does self-directed difficulty feel good?** Can you find the "sweet spot" distance where fights are exciting but winnable? Or does pushing further into dangerous territory scratch that risk/reward itch?
3. **Do different enemy archetypes force tactical adaptation?** Does fighting a Skirmisher feel different than fighting a Berserker?

Try this: Start near the haven. When it feels easy, push 400-800 tiles out. When that's comfortable, venture to 1200+ and see how long you survive. Each kind of ground raises a different archetype - find your favorite to fight.

Build with `cargo build`, then run `cargo run --bin server` and `cargo run --bin client` in separate terminals.

# Technical Notes

**What's Built So Far:**
- Reaction-based combat with visible threat timers
- Responsive movement (client-side prediction eliminates lag feel)
- **Distance-based difficulty scaling** (a den level per 400 tiles from the haven, capped five below the player's)
- **Six enemy archetypes** with distinct profiles (Berserker/Juggernaut/Flanker/Defender/Skirmisher/Ambusher)
- **Dens raised as you explore** (the world's sites get their packs as players approach)
- **Spatial difficulty UI** (distance indicator, color-coded enemy levels)
- Directional targeting system (face enemies to target them)
- Combat HUD with skill recovery and resource bars
- Enemy minds that weigh their skills and their steps (Chase, Engage, Hold)
- Procedural hex-based terrain with day/night cycles
- Gathering (trees felled and boulders mined into a weighed bag)
- Networked client-server architecture

**Design Target:**
Eventually MMO-scale (1000+ concurrent players, large shared world). Current prototype validates core combat mechanics before scaling up. Built on authoritative server architecture with client prediction, ECS (Bevy engine), and custom hex coordinate system (`qrz` library).
