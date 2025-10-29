# Attribute System

## Core Philosophy

The attribute system uses **opposing pairs** on sliding scales rather than independent stats. Players develop both their **Axis** (permanent center position) and **Spectrum** (tactical flexibility range), creating meaningful choices between specialization and adaptability.

**Key Principles:**
* **Dual Progression:** Invest in specialization (Axis) vs flexibility (Spectrum)
* **Tactical Adaptation:** Adjust position within Spectrum before encounters
* **Earned Refinement:** Prestige levels allow redistribution of investment
* **Trade-offs Matter:** Cannot max everything - must commit to identity

---

## The Three Attribute Pairs

### 1. MIGHT ↔ GRACE (Physical Expression)

**MIGHT:** Raw power, force, impact damage  
**GRACE:** Precision, timing, finesse, efficiency

### 2. VITALITY ↔ FOCUS (Endurance Type)

**VITALITY:** Physical/spiritual health, life force, stamina  
**FOCUS:** Mental stamina, concentration, willpower, acuity

### 3. INSTINCT ↔ PRESENCE (Engagement Style)

**INSTINCT:** Reaction speed, intuition, spontaneous response, gut feeling  
**PRESENCE:** Deliberate action, intimidation, commanding attention, calculated behavior

---

## Derived Stats by Attribute

Each attribute affects three core derived stats that apply universally to all builds. These ensure every attribute provides tangible value regardless of build archetype.

| Attribute | Derived Stat 1 | Derived Stat 2 | Derived Stat 3 |
|-----------|----------------|----------------|----------------|
| **MIGHT** | Base Physical Damage | Stagger Multiplier | Stamina Pool |
| **GRACE** | Movement Speed | Hit Chance | Dodge Recovery |
| **VITALITY** | Health Pool | Stagger Resist | DoT Resistance |
| **FOCUS** | Mana Pool | Base Magic Damage | Resist Recovery |
| **INSTINCT** | Critical Hit Chance | Physical Penetration | Parry Recovery |
| **PRESENCE** | Threat Generation | AoE Multiplier | CC Duration |

**Design Principles:**
* **No dump stats:** Every attribute provides universally valuable mechanics
* **Complementary pairs:** Opposing attributes serve different playstyles without invalidating each other
* **Build diversity:** Different combinations enable distinct archetypes (glass cannon, tank, duelist, support)

**Key Interactions:**
* **Offense:** Might (physical damage), Focus (magic damage), Instinct (crit/penetration)
* **Defense:** Grace (dodge), Vitality (HP/resists), Focus (magic resist)
* **Resources:** Might (stamina), Vitality (health), Focus (mana)
* **Control:** Presence (threat/AoE/CC), Instinct (precision)

This ensures that extreme specialization (e.g., 100 Might, 0 Grace) has meaningful trade-offs (maximum damage but immobile with no evasion), while balanced builds maintain competence across multiple dimensions.

---

## Axis & Spectrum Mechanics

### Terminology

**Axis:** Your permanent center position on each attribute pair (-100 to +100)
* Where you naturally sit when not adjusted
* Expensive to change (requires Prestige levels)
* Represents your core identity

**Spectrum:** Your adjustment range from center (0 to 100)
* How far you can shift from your Axis in either direction
* Freely adjustable out of combat
* Determines your tactical flexibility

**Shift:** Your current tactical adjustment (-spectrum to +spectrum)
* Where you've positioned yourself for this encounter
* Added to Axis to get actual attribute value

### Formulas

```
Actual Position = Axis + Shift
```

**When on LEFT side (negative axis):**
```
left_reach = abs(axis - spectrum * 1.5)
left = abs(axis + shift * 0.5 - spectrum)
right = spectrum + shift * 0.5
right_reach = spectrum * 1.5
```

**When on RIGHT side (positive axis):**
```
right_reach = abs(axis + spectrum * 1.5)
right = abs(axis + shift * 0.5 + spectrum)
left = spectrum - shift * 0.5
left_reach = spectrum * 1.5
```

**Key Properties:**
* Investing in spectrum grants `1.5x` reach on the axis side and `1.5x` reach on the opposite side
* Shifting within spectrum uses a `0.5x` multiplier, making the full spectrum range accessible
* The formulas are perfectly symmetrical between left and right sides
* At parity (axis=0), both reaches equal `spectrum * 1.5`

### Visual Metaphor: Scrollbar

The attribute system uses a horizontal scrollbar visualization for each attribute pair:

**Bar Elements:**
* **Gold/orange section** - Current attribute value on the left side (Might, Vitality, or Instinct)
* **Blue section** - Current attribute value on the right side (Grace, Focus, or Presence)
* **Center marker** - Your axis position (permanent investment point)
* **Outer numbers** - Reach values (maximum potential for spectrum skills)
* **Inner numbers** - Current values (available for normal skills right now)

**What the bar shows:**
* **Bar length** - Total attribute capacity (grows with axis + spectrum investment)
* **Gold/blue ratio** - Current tactical position (determined by shift)
* **Scrollable range** - How much you can adjust between encounters (determined by spectrum)

**Reading a bar:**
* Longer bar = more total investment
* More gold than blue = currently favoring left attribute
* Wider gap between outer/inner numbers = more tactical flexibility
* Center marker position = permanent specialization direction

**The scrollbar makes clear:**
* Specialists have long bars that can't scroll much
* Generalists have shorter bars with wide scroll ranges
* Your current power (inner numbers) vs potential power (outer numbers)
* Trade-off between commitment (axis) and flexibility (spectrum)

---

## Starting Position

All players (Evolved origin) start at:

```
MIGHT:    axis=0, spectrum=0, shift=0  (at parity, no flexibility)
VITALITY: axis=0, spectrum=0, shift=0  (at parity, no flexibility)
INSTINCT: axis=0, spectrum=0, shift=0  (at parity, no flexibility)
```

Everyone begins as a blank slate with no biases or flexibility.

---

## Leveling Progression (Levels 1-50)

Each level grants **1 investment point**:
* **+2% Axis shift** in any attribute pair, OR
* **+1% Spectrum expansion** in any attribute pair

**Total by level 50:** 50 invested points distributed however the player chooses

### Example Attribute Values Table

The following table shows how different investment strategies affect attribute values at various shift positions (using Might/Grace as the example pair):

| Investment | Shift | Might Reach | Might | Grace | Grace Reach |
|------------|-------|-------------|-------|-------|-------------|
| **Pure Specialist** | | | | | |
| axis=-100, spectrum=0 | 0 | 100 | 100 | 0 | 0 |
| axis=100, spectrum=0 | 0 | 0 | 0 | 100 | 100 |
| **Flexible Specialist** | | | | | |
| axis=-80, spectrum=10 | -10 | 95 | 95 | 5 | 15 |
| axis=-80, spectrum=10 | 0 | 95 | 90 | 10 | 15 |
| axis=-80, spectrum=10 | 10 | 95 | 85 | 15 | 15 |
| axis=80, spectrum=10 | -10 | 15 | 15 | 85 | 95 |
| axis=80, spectrum=10 | 0 | 15 | 10 | 90 | 95 |
| axis=80, spectrum=10 | 10 | 15 | 5 | 95 | 95 |
| **Generalist** | | | | | |
| axis=-10, spectrum=40 | -40 | 70 | 70 | 20 | 60 |
| axis=-10, spectrum=40 | -10 | 70 | 55 | 35 | 60 |
| axis=-10, spectrum=40 | 0 | 70 | 50 | 40 | 60 |
| axis=-10, spectrum=40 | 10 | 70 | 45 | 45 | 60 |
| axis=-10, spectrum=40 | 40 | 70 | 30 | 60 | 60 |
| **Pure Flexible** | | | | | |
| axis=0, spectrum=50 | -50 | 75 | 75 | 25 | 75 |
| axis=0, spectrum=50 | -10 | 75 | 55 | 45 | 75 |
| axis=0, spectrum=50 | 0 | 75 | 50 | 50 | 75 |
| axis=0, spectrum=50 | 10 | 75 | 45 | 55 | 75 |
| axis=0, spectrum=50 | 50 | 75 | 25 | 75 | 75 |

**Key Observations:**
* **Reach values** represent maximum potential with spectrum skills
* **Current values** (Might/Grace) represent what's available for normal skills
* Pure specialists (100 axis, 0 spectrum) have maximum power but zero flexibility
* Pure flexible builds (0 axis, 50 spectrum) have equal reach on both sides (75/75)
* The 1.5x multiplier on spectrum creates meaningful trade-offs between commitment and adaptability

---

## Prestige Progression (Level 51+)

Each Prestige level grants **ONE redistribution action**:

### Within Same Attribute Pair

* Convert **2% Spectrum → 1% Axis**, OR
* Convert **1% Axis → 2% Spectrum**

### Between Attribute Pairs

* Move **1% Spectrum** from one pair to another (direct transfer), OR
* Move **2% Axis** from one pair to another (direct transfer), OR
* Move **2% Spectrum** from one pair → **1% Axis** in another (conversion), OR
* Move **1% Axis** from one pair → **2% Spectrum** in another (conversion)

### Key Properties

* Total invested points **always remains 50**
* Complete respec requires **50 Prestige levels** minimum
* Small adjustments are cheap (5-10 Prestiges)
* Prestige points can be banked (limited amount, TBD)

### Example Redistribution

**Before (Level 50 Berserker):**
```
MIGHT:    axis=-50, spectrum=25  (range: -75 to -25)
VITALITY: axis=-25, spectrum=25  (range: -50 to 0)
```

**After 10 Prestiges (Sharpening Might):**
```
MIGHT:    axis=-60, spectrum=15  (range: -75 to -45)
          Converted 20 Spectrum → 10 Axis
VITALITY: axis=-25, spectrum=25  (unchanged)
```

**After 20 More Prestiges (Becoming Generalist):**
```
MIGHT:    axis=-40, spectrum=15  (range: -55 to -25)
          Moved 20 Axis to Vitality
VITALITY: axis=-45, spectrum=25  (range: -70 to -20)
          Received 20 Axis from Might
```

---

## Overclock Mechanic

**Going Beyond 100%:**
* Gear and Spectrum can push attributes above 100%
* Creates risk/reward trade-offs

**Example at 120% Might:**
* +20% damage bonus
* -20% attack speed penalty
* +20% stamina cost
* Increased vulnerability to interruption

**No hard caps** - balance will be tuned during development

---

## Integration with Triumvirate System

### Attribute Leanings by Approach/Resilience

Each Approach and Resilience has a **primary attribute** and supporting secondary/tertiary attributes. Skills from each category primarily scale from these attributes.

#### Approach Attribute Leanings

| Approach | Primary | Secondary | Tertiary |
|----------|---------|-----------|----------|
| **Direct** | Vitality | Might | Instinct |
| **Distant** | Focus | Grace | Presence |
| **Ambushing** | Grace | Vitality | Instinct |
| **Patient** | Presence | Might | Vitality |
| **Binding** | Might | Vitality | Presence |
| **Evasive** | Instinct | Grace | Vitality |
| **Overwhelming** | Presence | Might | Focus |

#### Resilience Attribute Leanings

| Resilience | Primary | Secondary | Tertiary |
|------------|---------|-----------|----------|
| **Vital** | Vitality | Might | Instinct |
| **Mental** | Focus | Grace | Presence |
| **Hardened** | Might | Vitality | Presence |
| **Shielded** | Grace | Focus | Instinct |
| **Blessed** | Presence | Vitality | Grace |
| **Primal** | Instinct | Vitality | Might |
| **Eternal** | Vitality | Grace | Focus |

#### Opposing Pairs

Perfect attribute opposites:
* **Direct ↔ Distant** (Vitality/Might/Instinct vs Focus/Grace/Presence)
* **Ambushing ↔ Patient** (Grace/Vitality/Instinct vs Presence/Might/Vitality)
* **Binding ↔ Evasive** (Might/Vitality/Presence vs Instinct/Grace/Vitality)
* **Vital ↔ Mental** (Vitality/Might/Instinct vs Focus/Grace/Presence)
* **Hardened ↔ Shielded** (Might/Vitality/Presence vs Grace/Focus/Instinct)
* **Blessed ↔ Primal** (Presence/Vitality/Grace vs Instinct/Vitality/Might)
* **Overwhelming** and **Eternal** are unique (no clean opposites)

### Signature Skills Scale From Attributes

Each Triumvirate class has **4 signature skills** (2 from Approach, 2 from Resilience) that scale from their category's attribute leanings.

**Example: Direct/Vital Berserker**
* Direct skills scale from: Vitality (primary), Might, Instinct
* Vital skills scale from: Vitality (primary), Might, Instinct
* Perfect alignment - all skills benefit from same stat distribution

**Example: Patient/Mental Duelist**
* Patient skills scale from: Presence (primary), Might, Vitality
* Mental skills scale from: Focus (primary), Grace, Presence
* Mixed attributes - requires balanced investment across multiple pairs

### "Reach" Skills (Max Stat Skills)

Some skills scale from your **maximum potential** (Axis + Spectrum) rather than current position:

```rust
// "Limit Break" skill example
damage = base * (1 + might_reach / 100)

// Even if currently shifted toward Grace (shift=+25),
// Limit Break still uses your maximum Might reach!
```

This rewards broad progression - even "wrong" investments contribute to ultimate abilities.

---

## Derived Stat Formulas

Movement speed and other derived stats scale from their associated attributes as defined in the Derived Stats table above. Specific formulas will be balanced during development, but the core principle is that each attribute provides meaningful baseline value even at 0, with scaling benefits up to 100.

**Example - Movement Speed (Grace):**

```rust
let movement_speed = max(75, 100 + (grace / 2));
```

* Grace = 0 (parity): speed = 75 (clamped at -25%, not worse)
* Grace = 50: speed = 125 (+25%)
* Grace = 100: speed = 150 (+50%)

**Trade-off:** Might specialists sacrifice mobility for power, Grace specialists gain speed and evasion.

Similar scaling curves apply to other derived stats, ensuring that investing in any attribute provides tangible combat benefits beyond just skill damage scaling.

---

## Design Goals Achieved

* ✅ **No dump stats** - Every attribute pair relevant to all builds
* ✅ **Meaningful choices** - Cannot max everything, must commit to identity
* ✅ **Tactical depth** - Pre-encounter adjustment within Spectrum
* ✅ **Progression = options** - Unlocking flexibility, not just raw power
* ✅ **Respects experimentation** - Can respec but requires effort (50 Prestiges for full rebuild)
* ✅ **Supports all Triumvirate combinations** - Different Approach/Resilience combos favor different distributions
* ✅ **Fresh but familiar** - Slider mechanic is unique, but concepts are intuitive
* ✅ **Visual clarity** - Scrollbar metaphor makes system immediately graspable

---

## Open Questions / Future Design

1. **Prestige banking limits** - How many can be saved? Prevent respec-on-demand
2. **Adjustment frequency** - When can players shift within Spectrum? Out of combat only? At rest points?
3. **Stance skills** - Should some abilities temporarily modify Shift mid-combat?
4. **Gear effects** - How much can gear modify Axis vs Spectrum vs Shift?
5. **Other Origins** - When added (Synthetic, Essential, etc.), different starting Axis positions?
6. **Parity bonuses** - Should balanced builds (near 0 Axis) get special perks?
7. **Visual representation** - UI/UX refinement for scrollbar metaphor

---

## Balance Considerations

### Specialist vs Generalist

**Pure Specialist (axis=-100, spectrum=0, shift=0):**
* Current: 100 Might, 0 Grace
* Reach: 100 Might, 0 Grace
* Dominates completely in their domain
* Zero access to opposite attribute (hard-countered)
* No tactical flexibility
* Predictable, easy to counter-pick

**Flexible Specialist (axis=-80, spectrum=10, shift=-10):**
* Current: 95 Might, 5 Grace
* Reach: 95 Might, 15 Grace
* Still very strong in specialization (-5 from pure)
* Small emergency access to opposite side
* Minor tactical adjustments (±10 shift range)
* Slightly less predictable

**Generalist (axis=-10, spectrum=40, shift=-40):**
* Current: 70 Might, 20 Grace
* Reach: 70 Might, 60 Grace
* Moderate power on primary side (-30 from pure)
* Significant opposite-side access via reach skills
* Wide tactical adjustments (±40 shift range)
* Can adapt to many situations

**Pure Flexible (axis=0, spectrum=50, shift=-50):**
* Current: 75 Might, 25 Grace
* Reach: 75 Might, 75 Grace
* Balanced reach on both sides
* Maximum tactical flexibility (±50 shift range)
* Never optimal, never helpless
* Can play any style situationally

### Investment Trade-offs

The 1.5x spectrum multiplier creates meaningful choices:

**Investing 10 points in Axis (Might specialist):**
* Gain: +20 Might, +20 Might reach
* Cost: Deeper commitment (harder to pivot later)

**Investing 10 points in Spectrum:**
* Gain: +15 Might reach, +15 Grace reach
* Cost: -10 current Might (when fully shifted)

**The Spectrum Advantage:**
* 10 spectrum points grant 30 total reach (15 each side)
* 10 axis points grant 20 total (20 on one side, 0 opposite)
* Spectrum provides more **total potential** but less **current power**
