# Hub System

## Dynamic Settlement Growth

Player hubs grow or shrink based on active player population. NPCs dynamically migrate to settlements based on hub size, offering services, quests, and vendors. Player presence pushes back hostile territory, creating safer zones around hubs.

**Hub tier progression:** Frontier Camps (5-10 players) → Towns (20-50 players) → Cities (100+ players)

## Player Investment

Players build homesteads (personal investment), establish markets (player-to-player trade), and construct factories at higher hub tiers (complex crafting, requires large stable populations). Investment creates stakes—hub loss means loss of infrastructure and economic access.

## Hub Tiers & Capabilities

**Frontier Camps:** Basic homesteads, NPC vendors, personal storage. High-risk, resource-rich territory. Frequent small-scale assaults.

**Towns:** Player markets, basic crafting stations, moderate defenses. Mix of player levels. Regional trade hubs.

**Cities:** Advanced factories, complex crafting, economic centers. Rare but massive sieges. Primarily safe zones for economic activity.

---

## Influence System

### Influence Radius

```
influence_radius = population × 3.0 tiles
```

**Purpose:** Pushes back encroachment in the surrounding world, creating safer zones for travel and resource gathering.

**Examples:**
* 25 pop camp → 75 tile influence radius
* 100 pop town → 300 tile influence radius
* 1000 pop city → 3000 tile influence radius

### Influence Falloff

```
influence = (1.0 - distance/radius)³
```

Steep dropoff curve: slow decline near center, rapid falloff through mid-range, slow approach to zero at edge.

### Maximum Influence

```
max_influence = √(population / 1000), capped at 1.0
```

**Examples:**
* 25 pop camp → 0.158 max influence
* 100 pop town → 0.316 max influence
* 1000 pop city → 1.0 max influence

### Encroachment Calculation

At any hex, sum all hub influences (additive, capped at 1.0):

```
total_influence = Σ(hub.max_influence × distance_factor)
encroachment = MAX_ENCROACHMENT × (1.0 - total_influence)
```

**Key properties:**
* Multiple hubs of similar size extend safe territory cooperatively
* Smaller hubs near larger ones contribute minimally (overshadowed)
* Only the largest hubs (1000+ pop) can fully suppress encroachment at their center

---

## Urban Core & Protection Zones

### Urban Core Radius

```
urban_core_radius = influence_radius × 0.1 = population × 0.3 tiles
```

**Purpose:** Defines the fully protected and defended area where walls are actively maintained.

**Examples:**
* 25 pop camp → 7.5 tile urban core
* 100 pop town → 30 tile urban core
* 1000 pop city → 300 tile urban core

### Intelligent Boundary Optimization

The urban core is **not a perfect circle**. The boundary is optimized to encompass as many constructs as possible within the area budget:

**Area budget:**
```
urban_core_area = π × (population × 0.3)² square tiles
```

**Optimization goal:**
* Maximize construct coverage (weighted by mass)
* Maintain reasonable compactness
* For merged hubs: maintain corridors between neighborhoods when viable

**Result:** Organic boundary shapes that follow actual development patterns (ellipses, dumbbells, figure-8s for merged hubs).

### Protection Zones

**Urban Core (0-10% of influence):**
* Walls maintained at 100% integrity
* Fully repaired after sieges
* Lowest siege spawn weighting
* Maximum protection

**Standard Urban (10-100% of influence):**
* Walls maintained at 100% integrity
* Repaired after sieges
* Normal siege spawn weighting
* Good protection

**Peripheral Zone (Beyond influence):**
* Walls NOT maintained (degrade to 30% over 90 days)
* High siege spawn weighting
* High risk—constructs likely to be destroyed
* Can be manually maintained at high cost

---

## Center of Mass

Hubs have a dynamic center of mass calculated from all constructs:

```
center_of_mass = Σ(construct_position × construct_mass) / Σ(construct_mass)
```

where `construct_mass = number of tiles occupied by the construct`

**The center shifts based on:**
* Where new constructs are built (pulls center toward development)
* Where constructs are destroyed (releases pull from that area)
* Construct mass distribution (factories pull harder than homesteads)

**Protection zones radiate from this center**—constructs farther from center are at higher risk.

---

## Hub Evolution Lifecycle

Hubs evolve through a predictable lifecycle based on encroachment and anger:

### Frontier Camps (Early Stage)
* High encroachment → strong individual enemies
* Low anger → small waves
* Challenge: Elite threats in manageable numbers
* Ideal for veteran players

### Growing Towns (Mid Stage)
* Medium encroachment → moderate enemy strength
* Medium anger → medium waves
* Challenge: Balanced threat requiring skilled coordination

### Established Cities (Late Stage)
* Low encroachment → weaker individual enemies
* High anger → massive waves
* Challenge: Overwhelming numbers of manageable enemies
* Defended primarily by mid-level players

---

## Anger Generation

### Anger Sources

* Player homesteads: moderate anger
* NPC homesteads: moderate anger (but don't attract more NPCs)
* Factories: significantly more anger than homesteads
* Economic activity and resource extraction: scaling anger

### Anger Accumulation

Each hub accumulates anger from all overlapping hub influences:

```
total_anger = base_anger + Σ(other_hub.anger × influence_factor)
```

**No size multiplier**—hubs accumulate the same anger they would generate at that distance, regardless of relative sizes.

---

## Strategic Hub Placement

### Urbanization Incentives

**Two small hubs near each other:**
* Each faces independent sieges
* Redundant defense costs
* Minimal cooperation benefit

**One merged hub:**
* Single larger siege
* Concentrated defense
* Economy of scale
* Larger influence radius

### Deadly Proximity Zones

Small hub near large city:
* Accumulates massive anger from city (8-10x base anger)
* Provides minimal encroachment benefit (overshadowed by city)
* Faces overwhelming waves of weak enemies
* Economic death trap—forces merger or relocation

### Optimal Patterns

**Frontier outposts:** Small camps far apart, deep in wilderness (high encroachment, low anger—elite small waves)

**Regional towns:** Medium hubs spaced 1000+ tiles apart (balanced sieges)

**Capital cities:** One massive hub per region (low encroachment, high anger—massive waves of weak enemies)

---

## Hub Merging

### Automatic Merge Trigger

Hubs automatically merge when their urban cores overlap:

```
urban_core = radius × 0.1
merge_triggered when: distance < (urban_core_a + urban_core_b)
```

**Examples:**
* Two 100 pop towns, 50 tiles apart: 50 < (30 + 30) → **MERGE**
* 1000 pop city + 100 pop town, 250 tiles apart: 250 < (300 + 30) → **MERGE**

### Merge Mechanics

**The larger hub absorbs the smaller hub:**
* Larger hub maintains primary identity
* Smaller hub becomes a **neighborhood** within the merged entity
* Both maintain their own center of mass for visual/structural purposes

**Single unified entity:**
```
combined_population = pop_a + pop_b
combined_center_of_mass = Σ(all_construct_positions × construct_mass) / Σ(all_construct_mass)
new_influence_radius = combined_population × 3.0
new_urban_core_radius = new_influence_radius × 0.1
```

**Urban core optimization:**
* Boundary shaped to encompass constructs from both neighborhoods
* Creates bulges around each neighborhood center
* Maintains corridor connection between them (when viable)
* Results in dumbbell or figure-8 shapes

### Merged Hub Behavior

**All constructs evaluated from combined center of mass:**
* Distance from unified center determines protection level
* Original hub locations become irrelevant for calculations
* Protection follows physics, not identity

**Once merged, always merged:**
* Merged hubs never split back into separate entities
* They remain one unified hub
* As population changes, center of mass shifts dynamically
* Peripheral areas may be destroyed, but hub stays merged

---

## Shrinking Hub Mechanics

### As Population Decreases

```
Population drops → influence_radius decreases → urban_core shrinks → center_of_mass may shift
```

**Boundary optimization adapts:**
1. Calculate new smaller urban core area budget
2. Recalculate center of mass (may shift as constructs are destroyed)
3. Optimize boundary to cover maximum constructs within new budget
4. Constructs beyond new boundary become peripheral

### Smart Contraction Process

**Phase 1: Contract empty sectors**
* Sectors with no constructs contract immediately
* Boundary shape becomes irregular
* Maintains protection for developed areas

**Phase 2: Accept peripheral zones**
* Outlying constructs beyond new urban core become peripheral
* Walls in these areas stop being maintained
* Degradation begins (100% → 30% over 90 days)

**Phase 3: Natural attrition**
* Sieges exploit peripheral weak points
* Peripheral constructs destroyed
* Center of mass shifts away from destroyed areas
* Further contraction (feedback loop)

### Result

Natural consolidation toward the center through siege pressure. Players must choose: defend extensive periphery at high cost OR let peripheral areas fall and consolidate to the urban core.

---

## Shrinking Merged Hubs

### Corridor Maintenance

When a merged hub shrinks, the boundary optimization attempts to maintain corridors between neighborhoods **while viable**.

**Corridor viability criteria:**
```
corridor_width ≥ minimum_width (50 tiles)
AND
maintaining_corridor doesn't reduce overall coverage by >15%
```

**When corridor becomes non-viable:**
* Area saved by breaking corridor exceeds threshold (20%)
* Corridor width drops below minimum
* Better to focus protection on one neighborhood cluster

**Result:** Smaller/sparser neighborhood becomes peripheral, eventually destroyed. Larger/denser neighborhood remains protected. The hub never formally "splits"—it just loses peripheral areas through attrition.

### Shrinkage Example

**Healthy merged hub (1000 pop):**
```
   ___     ___
   /   \___/   \
  | A  |   | B |
   \___/   \___/
```
Both neighborhoods protected, wide corridor, 90% construct coverage.

**Moderate shrinkage (600 pop):**
```
  ___   ___
  /   \_/   \
 | A  | | B |
  \___/ \___/
```
Narrower corridor, some peripheral constructs, 75% coverage.

**Severe shrinkage (300 pop):**
```
  ___
  | A |
   ¯¯¯
```
Corridor broken, neighborhood B peripheral and destroyed, 60% coverage focused on A.

---

## Example Scenarios

### Town 300 Tiles from City
* Anger: 414 (8.3x base)
* Encroachment: 0.0
* Result: Massive waves of weakest enemies—unsustainable

### Town 1500 Tiles from City
* Anger: 112 (2.25x base)
* Encroachment: 5.59
* Result: Moderate waves of medium-strength enemies—challenging but viable

### Town Between Two Cities (1250 Tiles Each)
* Anger: 447 (8.9x base)
* Encroachment: 0.0
* Result: No-man's land death trap

### Two Towns 200 Tiles Apart
* Anger: 52 each (1.04x base)
* Encroachment: 6.72
* Result: Cooperative territory extension, frontier-level threats

### Lone Frontier Camp
* Anger: 10 (1x base)
* Encroachment: 8.42
* Result: Small waves of elite enemies—perfect for veterans

---

## Design Principles

1. **Distance from center = risk level** — Constructs farther from hub center face higher siege pressure
2. **Urban core scales with population** — Bigger hubs have bigger safe zones (10% of influence radius)
3. **Boundaries optimize for construct coverage** — Not perfect circles; shaped to protect actual development
4. **Merging uses center of mass physics** — One unified center, original locations become irrelevant
5. **Never splits, only shrinks** — Merged hubs stay merged; peripheral areas destroyed through attrition
6. **Player choice in building location** — Build near center for safety OR far from center for space (accept risk)
