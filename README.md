# Build
- `cargo build`

# Run
- `cargo run --bin server`
- `cargo run --bin client`

# Play

**Combat MVP Prototype** - The tactical reaction-based combat loop is fully playable.

## What's Actually Working

**Movement & Positioning**
- Hex-based movement with persistent facing direction (you keep looking the direction you last moved)
- Directional targeting - enemies in front of you are automatically targeted based on where you're facing
- Terrain discovery with procedural generation and day/night cycles

**The Combat Loop**
- Fight Wild Dogs using 5 distinct abilities with clear tactical roles
- **Threat Queue System** (the unique part) - incoming attacks appear as circular timers above your health bar. You have 1-2 seconds to decide: clear the queue with a defensive ability, or take the hits and counter-attack
- Resource management - balance stamina costs across offensive abilities (Lunge/Overpower) and defensive reactions (Knockback/Deflect)
- Mutual destruction - both you and your enemy can die simultaneously if attacks are already in-flight

**Combat HUD**
- Action bar shows ability cooldowns and resource costs
- Threat icons display incoming attacks with countdown timers
- World-space health bars and target indicators (red for hostile, green for allies)
- Resource bars for health, stamina, and mana

## Controls
**Movement:** Arrow keys - Move and set facing direction
**Abilities:**
- Q - Lunge (40 dmg, 20 stam, 4 hex range) - Gap closer
- W - Overpower (80 dmg, 40 stam, 1 hex range) - Big hit
- E - Knockback (30 stam) - Counter the most recent attacker, push them away, clear that threat
- R - Deflect (50 stam) - Panic button, clears ALL queued threats
- Spacebar - Auto-Attack (20 dmg, adjacent only) - Basic attack

**Other:**
- G - Toggle hex grid visualization
- ` (backtick) - Developer console

## What to Expect

You spawn, Wild Dogs aggro you, combat begins. Watch the threat queue - those circular icons show incoming attacks. Let threats resolve and you take damage. Use Knockback to counter-punch specific attackers. Use Deflect when overwhelmed. Run out of stamina and you're in trouble.

**The Core Question:** Does having time to react to incoming damage create interesting tactical decisions, or does it just delay the inevitable? That's what this prototype tests.

**Death:** You die, you respawn nearby. No penalties yet - this is about testing combat feel.

# Game features
## Currently Implemented
- **Client-side prediction** - Movement feels instant, zero perceived lag
- **Tactical reaction-based combat** - Designed for conscious decisions over twitch reflexes
  - Reaction queue with visible threat timers (1-2 second windows to respond)
  - Mutual destruction mechanics (both combatants can die simultaneously)
  - Five-ability MVP set with offensive/defensive/reactive options
- **Directional keyboard combat** - No mouse required, automatic targeting based on facing direction
- **Hex movement system** - Persistent heading, 4-key directional movement
- **Combat HUD** - Action bar, threat queue display, target indicators, resource bars
- **Enemy AI** - Wild Dogs with aggro detection, pursuit behavior, and attack cycles
- **Procedural terrain** - Perlin noise generation with organic slope transitions
- **Day/night cycles** - Dynamic sun/moon with seasonal lighting
- **Chunk-based world** - Terrain streaming with smart caching

## Architectural Foundations (for MMO scale)
- Authoritative server with client-side prediction
- R*-tree spatial indexing for O(log n) entity queries
- Custom hexagonal coordinate system (`qrz` library)
- Chunk-based terrain discovery with LRU world cache
- Boundary-triggered fog-of-war (not per-movement)
- Input stream isolation (streaming vs GCD)
- A* pathfinding on hex grid
- Do/Try event pattern for client-server authority
- Four-stage damage pipeline: Deal → Insert → Resolve → Apply
- Hybrid damage timing: outgoing at attack time, mitigation at resolution
- ECS architecture (Bevy engine)
- Network protocol with client prediction and rollback
- Contextual developer console
- Shared game logic in `common/` (client and server use identical physics/behavior)

**Scale Target:** 1000+ concurrent players, 100 km² world (designed for it, not yet proven)