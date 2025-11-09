# Haven System

## Starter Havens

### Purpose

Solve the bootstrap problem at game launch - without existing hubs, the entire world would be max encroachment (elite enemies everywhere), making it impossible for new players to establish the first settlements.

### Haven Specifications

**Three Permanent Starter Havens:**
* **Mountain Stronghold** (cold/mountain biome)
* **Prairie Fortress** (grassland/plains biome)
* **Forest Village** (forest/woodland biome)

**Properties:**

```
Population: 1000 (city-level influence)
Influence Radius: 3000 tiles
Max Influence: 1.0 (full city-level encroachment pushback)
Base Anger: 50 (town-level, NOT city-level)
Urban Core: 30 tiles (10% of standard, allows close building)
```

**Special Attributes:**
* **Indestructible** - cannot be destroyed by sieges
* **Permanent respawn point** for new/defeated players
* **Basic services always available** (starter vendors, basic crafting)
* **Full encroachment protection** (1.0 max influence like a city)
* **Minimal anger generation** (50 like a town, not 500 like a city)

### Placement

Havens placed 7000+ tiles apart to avoid influence overlap:

```
Mountain Stronghold: (-5000, -5000)
Prairie Fortress: (5000, -5000)
Forest Village: (0, 5000)
```

### Siege Behavior

Starter Havens still experience sieges for tutorial purposes:

```
Anger: 50 (just base, unless near player hubs)
Encroachment: 0.0 (at center)
Wave Size: 50
Mob Level: 0.0
Result: Small waves of weak enemies
        Always survivable, even unmanned
        Cannot be destroyed regardless of outcome
```

## First Camp Viability

### Pioneer Belt (500-750 tiles from Haven)

**Camp at 500 tiles:**
```
Wave Size: 38.9 (~4x normal)
Mob Level: 2.64 (weak-to-medium)
Recommended: 5-8 players, level 10-15
Status: Accessible pioneer content
```

**Camp at 750 tiles:**
```
Wave Size: 31.1 (~3x normal)
Mob Level: 4.2 (medium)
Recommended: 4-6 players, level 15-20
Status: Established pioneer challenge
```

### Veteran Belt (1000-1500 tiles)

**Camp at 1000 tiles:**
```
Wave Size: 24.8 (~2.5x normal)
Mob Level: 5.46 (medium-strong)
Recommended: 3-5 players, level 20-30
Status: Veteran frontier
```

**Camp at 1500 tiles:**
```
Wave Size: 16.25 (~1.6x normal)
Mob Level: 7.17 (strong)
Recommended: 2-3 players, level 30+
Status: Elite frontier
```

## Territory Zones

| Distance from Haven | Encroachment | Purpose |
|---------------------|--------------|---------------------------|
| 0-300 tiles         | 0-1          | Core/Tutorial Zone |
| 300-500 tiles       | 1-3          | Resource Gathering |
| 500-750 tiles       | 3-5          | Pioneer Camp Belt (FIRST SETTLEMENTS) |
| 750-1500 tiles      | 5-7          | Veteran Expansion |
| 1500-3000 tiles     | 7-9          | Elite Frontier |
| 3000+ tiles         | 9-10         | True Wilderness |

## Progression Timeline

### Launch - Day 2
* Players spawn at one of three Havens
* Explore 0-500 tile radius
* Gather resources, learn mechanics
* Scout potential camp locations

### Days 3-5
* First brave groups establish camps at 500-600 tiles
* Small coordinated parties (5-8 players)
* Face pioneer-tier sieges

### Week 2
* More camps fill in 500-1000 tile band
* Early camps begin forming settlement clusters
* Players start cooperating across nearby camps

### Week 3-4
* Successful camps merge into first player towns (100+ pop)
* Towns begin to provide their own influence
* Haven influence becomes less critical for inner camps

### Month 2
* Player towns at 800-1500 tiles grow to rival Haven influence
* Regional hubs emerge independent of Havens
* New players still funnel through Havens but quickly join player towns

### Month 3+
* Player cities (1000+ pop) emerge
* Player territories overshadow Haven influence
* Havens remain as permanent fallback/tutorial zones
* Economic activity shifts entirely to player hubs

## Long-Term Role

### Endgame Function

* Safe respawn point for defeated players
* Newbie tutorial zone (always safe)
* Fallback when player hubs are under siege
* Historical/lore significance
* Minimal economic activity (players use real hubs)
* Always present, never relevant to veteran gameplay

## Design Goals Achieved

* ✓ Solves launch bootstrap problem (safe zones exist immediately)
* ✓ Creates accessible pioneer content (500-750 tile camps)
* ✓ Natural difficulty curve (distance = challenge)
* ✓ Doesn't invalidate player cities (low anger, becomes overshadowed)
* ✓ Permanent safety net (indestructible, always respawnable)
* ✓ Encourages outward expansion (better resources further out)
* ✓ Multiple starting points (reduces crowding, regional identity)
