# ADR-004: Ability System and Directional Targeting

## Status

Proposed

## Context

### Combat System Requirements

From `docs/spec/combat-system.md`, the combat system is **directional** - no clicking or cursor required:

**Core Design Philosophy:**
> **"Directional combat"** - Face your enemies, position matters, no cursor required. Movement updates heading, heading determines targets.

1. **Heading System:**
   - Movement with arrow keys automatically updates heading (facing direction)
   - Heading persists after movement stops
   - Heading determines character rotation AND position on hex
   - Facing cone: 60° (one hex-face direction)

2. **Automatic Target Selection:**
   - Default: Nearest hostile in facing direction (within 60° cone)
   - Geometric tiebreaker: closest to exact facing angle
   - No clicking required - just face enemies and press ability key

3. **Tier Lock System:**
   - **Tier 1 (Close)**: 1-2 hexes
   - **Tier 2 (Mid)**: 3-6 hexes
   - **Tier 3 (Far)**: 7+ hexes
   - Press 1/2/3 to lock to range tier
   - Tier lock drops after 1 ability use
   - TAB to cycle through targets in current tier

4. **Target Indicators:**
   - Red indicator: One hostile target (current tier + facing)
   - Green indicator: Nearest ally in facing direction
   - Visual feedback: tier badges, lock markers, range highlights

5. **Ability Patterns:**
   - **Single Target**: Hits indicated target
   - **Self Target**: Affects caster (Dodge, buffs) - no targeting
   - **Line Pattern**: N hexes in facing direction
   - **Radius Pattern**: Area around you or target
   - **Point-Blank AOE**: Area centered on you

6. **Execution Patterns:**
   - **Instant**: Resolves immediately (Basic Attack)
   - **Projectile**: Travels to target hex, dodgeable (Volley)
   - **Ground Effect**: Telegraph + delay (Eruption, Trap)
   - **Unavoidable**: Bypass reaction queue (rare, expensive)

7. **Keyboard Controls:**
   - Arrow keys: Movement (updates heading automatically)
   - Q/W/E/R: Ability slots
   - 1/2/3: Tier lock (close/mid/far)
   - TAB: Cycle targets in tier
   - ESC: Clear manual targeting

### Current Codebase State

**Heading System**: Exists in `common/components/heading.rs` - 6 cardinal directions (NE, E, SE, SW, W, NW)

**GCD System**: `common/systems/gcd.rs` - GcdType enum and cooldown tracking

**Triumvirate**: `ActorImpl` in `common/components/entity_type/actor.rs` - Approach/Resilience enums

**Movement**: `controlled.rs` - Client-side prediction for movement with input queue

**No Targeting System**: No directional targeting, no tier lock, no target indicators

### Architectural Challenges

#### Challenge 1: Heading Integration

**Problem:** Movement updates heading, but abilities need to query heading for targeting.

**Considerations:**
- Heading component already exists (6 directions: NE, E, SE, SW, W, NW)
- Movement systems in `controlled.rs` already update heading
- Abilities need to convert heading → facing cone (60° arc)
- Heading determines both rotation AND position on hex (visual facing)

**Design Question:** How to convert 6-direction heading into 60° facing cone for targeting?
- Each heading corresponds to one hex-face (60°)
- Heading::NE = facing 30° (northeast)
- Cone extends ±30° from heading angle
- Use angular math to check if target within cone

#### Challenge 2: Automatic Target Selection Algorithm

**Problem:** Find "nearest hostile in facing direction" efficiently.

**Algorithm:**
1. Query all hostiles within max range (use NNTree spatial index)
2. Filter to entities within 60° facing cone (angular check)
3. Select nearest by distance (hex distance)
4. If tie: Select closest to exact heading angle (geometric tiebreaker)

**Performance Considerations:**
- NNTree query fast (spatial index)
- Angular filtering cheap (dot product or angle comparison)
- Runs every frame for local player (UI indicator updates)
- Runs on ability use for all entities (validation)

**Tier Lock Modification:**
- If tier locked: Filter to entities within tier range FIRST
- Then apply facing cone filter
- Then nearest selection

#### Challenge 3: Tier Lock State Management

**Problem:** Track tier lock per entity, drop after 1 ability use.

**Options:**
- **Component:** `TierLock { tier: Option<RangeTier>, expires_on_next_ability: bool }`
- **Resource:** Global HashMap<Entity, TierLock> (simpler, no component bloat)
- **Event-driven:** Set lock on key press, clear on ability use

**Considerations:**
- Tier lock is transient (drops after 1 ability)
- Only applies to player (NPCs use default targeting)
- Client-side only (server doesn't need tier lock state)

**Decision:** Client-side component `TierLock`, cleared by ability usage system

#### Challenge 4: TAB Cycling State

**Problem:** TAB cycles through valid targets, lock persists until ESC or target invalid.

**State:**
- `TargetingOverride { locked_target: Entity, tier: RangeTier }`
- Press TAB: increment to next target in valid list
- Press ESC: clear override, return to automatic
- Target dies or moves out of range: clear override

**Cycle Logic:**
1. Get valid targets in current tier + facing cone
2. Find index of current locked target
3. Increment index (wrap around to 0)
4. Lock to new target

#### Challenge 5: Target Indicator Rendering

**Problem:** Show red hostile / green ally indicators that update smoothly.

**Visual Design:**
- Red indicator: Outline or ground marker on hostile target
- Green indicator: Same for ally target
- Tier badge: Small "1", "2", "3" icon when tier locked
- Range highlight: Show valid hexes in tier when no targets (tier lock active but empty)

**Update Frequency:**
- Run every frame for local player (smooth indicator movement)
- Recalculate on:
  - Movement (heading changes)
  - Tier lock change (1/2/3 pressed)
  - TAB press (manual cycle)
  - Entities move (targets shift in/out of cone)

### MVP Requirements

From combat spec MVP:

**Player Abilities:**
- **Basic Attack** (Q key): Instant, adjacent hex (close tier), hits indicated hostile target
- **Dodge** (E key): Self-target, clears queue, no targeting required

**Enemy Abilities:**
- Wild Dog: Basic melee attack (auto-targets nearest player in facing direction)

**Targeting:**
- Automatic targeting (nearest in direction)
- Default tier (no tier lock initially)
- Red hostile indicator shows current target
- Face Wild Dog with arrow keys, press Q to attack

**Scope Simplifications:**
- No tier lock in MVP (add in Phase 2)
- No TAB cycling (automatic targeting sufficient)
- No complex patterns (Line, Radius, Adjacent) - Single and SelfTarget only
- No projectiles (instant abilities only)

## Decision

We will implement a **directional targeting system with automatic target selection based on heading and proximity**, prioritizing MVP simplicity while designing for tier lock and TAB cycling extensions.

### Core Architectural Principles

#### 1. Heading-Based Targeting

**Heading as Foundation:**
- All targeting derives from `Heading` component (already exists)
- Heading updated by movement systems (`controlled.rs`)
- Heading determines 60° facing cone for target selection
- No manual "turn in place" command (movement only)

**Facing Cone Calculation:**

```rust
pub fn is_in_facing_cone(
    caster_heading: Heading,
    caster_loc: Loc,
    target_loc: Loc,
) -> bool {
    let heading_angle = caster_heading.to_angle();  // NE=30°, E=90°, SE=150°, etc.
    let target_angle = (target_loc - caster_loc).angle();  // Angle from caster to target

    let delta = (target_angle - heading_angle).abs();
    let delta_normalized = if delta > 180.0 { 360.0 - delta } else { delta };

    delta_normalized <= 30.0  // 60° cone = ±30° from heading
}
```

**Heading → Angle Mapping:**
- Heading::NE → 30° (northeast)
- Heading::E → 90° (east)
- Heading::SE → 150° (southeast)
- Heading::SW → 210° (southwest)
- Heading::W → 270° (west)
- Heading::NW → 330° (northwest)

#### 2. Automatic Target Selection

**Target Selection Module:** `common/systems/targeting.rs`

Functions used by both client and server:

```rust
pub fn select_target(
    caster_loc: Loc,
    caster_heading: Heading,
    tier_lock: Option<RangeTier>,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc)>,
) -> Option<Entity>;

pub enum RangeTier {
    Close,   // 1-2 hexes
    Mid,     // 3-6 hexes
    Far,     // 7+ hexes
}

pub fn get_range_tier(distance: u32) -> RangeTier {
    match distance {
        1..=2 => RangeTier::Close,
        3..=6 => RangeTier::Mid,
        _ => RangeTier::Far,
    }
}
```

**`select_target` Algorithm:**

```rust
pub fn select_target(
    caster_loc: Loc,
    caster_heading: Heading,
    tier_lock: Option<RangeTier>,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc)>,
) -> Option<Entity> {
    // 1. Query entities within max range (20 hexes for MVP)
    let nearby = nntree.locate_within_distance(caster_loc, 20);

    // 2. Filter to hostiles in facing cone
    let mut candidates: Vec<(Entity, Loc, u32)> = Vec::new();
    for ent in nearby {
        let Ok((entity_type, loc)) = entity_query.get(ent) else { continue };

        // Filter to actors (NPCs and players)
        if !matches!(entity_type, EntityType::Actor(_)) { continue };

        // Check facing cone (60°)
        if !is_in_facing_cone(caster_heading, caster_loc, *loc) { continue };

        let distance = caster_loc.distance(*loc);

        // Apply tier filter if locked
        if let Some(tier) = tier_lock {
            if get_range_tier(distance) != tier { continue };
        }

        candidates.push((ent, *loc, distance));
    }

    // 3. Select nearest (distance)
    if candidates.is_empty() { return None; }

    candidates.sort_by_key(|(_, _, dist)| *dist);
    let nearest_distance = candidates[0].2;

    // 4. Geometric tiebreaker: If multiple at same distance, pick most "in front"
    let tied: Vec<_> = candidates.iter()
        .filter(|(_, _, dist)| *dist == nearest_distance)
        .collect();

    if tied.len() == 1 {
        return Some(tied[0].0);
    }

    // Calculate angle delta from exact heading for tiebreaker
    let heading_angle = caster_heading.to_angle();
    let mut best_target = tied[0].0;
    let mut smallest_delta = f32::MAX;

    for (ent, loc, _) in tied {
        let target_angle = (*loc - caster_loc).angle();
        let delta = (target_angle - heading_angle).abs();
        let delta_normalized = if delta > 180.0 { 360.0 - delta } else { delta };

        if delta_normalized < smallest_delta {
            smallest_delta = delta_normalized;
            best_target = *ent;
        }
    }

    Some(best_target)
}
```

**Benefits:**
- Deterministic (same inputs → same target)
- Client and server use identical logic (no desync)
- Geometric tiebreaker prevents arbitrary selection
- Extensible (add tier lock, faction filtering)

---

#### 3. Client-Side Tier Lock Management

**Component:** `client/components/tier_lock.rs`

```rust
#[derive(Component)]
pub struct TierLock {
    pub locked_tier: Option<RangeTier>,
    pub clear_on_next_ability: bool,
}
```

**System:** `client/systems/targeting::handle_tier_lock_input`

```rust
pub fn handle_tier_lock_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut TierLock, With<Controlled>>,  // Local player only
) {
    let Ok(mut tier_lock) = query.get_single_mut() else { return };

    if keyboard.just_pressed(KeyCode::Digit1) {
        tier_lock.locked_tier = Some(RangeTier::Close);
        tier_lock.clear_on_next_ability = true;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        tier_lock.locked_tier = Some(RangeTier::Mid);
        tier_lock.clear_on_next_ability = true;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        tier_lock.locked_tier = Some(RangeTier::Far);
        tier_lock.clear_on_next_ability = true;
    }
}
```

**Clearing Logic:**

```rust
// In ability usage system
pub fn clear_tier_lock_on_ability_use(
    mut query: Query<&mut TierLock, With<Controlled>>,
) {
    let Ok(mut tier_lock) = query.get_single_mut() else { return };

    if tier_lock.clear_on_next_ability {
        tier_lock.locked_tier = None;
        tier_lock.clear_on_next_ability = false;
    }
}
```

**Why Client-Only:**
- Tier lock is input assistance (doesn't affect game logic)
- Server doesn't care which tier was locked (just validates final target)
- Reduces network traffic (no tier lock sync needed)

---

#### 4. Target Indicator Rendering (Client-Side)

**System:** `client/systems/targeting_ui::render_target_indicators`

**Rendering Approach:**
- Query local player's heading, location, tier lock
- Call `select_target` to get current hostile/ally targets
- Spawn/update indicator entities (ground markers or outlines)
- Update every frame for smooth movement

**Visual Components:**

```rust
#[derive(Component)]
pub struct TargetIndicator {
    pub indicator_type: IndicatorType,  // Hostile or Ally
}

pub enum IndicatorType {
    Hostile,  // Red
    Ally,     // Green
}
```

**Rendering Logic:**

```rust
pub fn render_target_indicators(
    mut commands: Commands,
    local_player: Query<(&Loc, &Heading, &TierLock), With<Controlled>>,
    nntree: Res<NNTree>,
    entities: Query<(&EntityType, &Loc)>,
    mut indicators: Query<(Entity, &mut Transform, &TargetIndicator)>,
) {
    let Ok((loc, heading, tier_lock)) = local_player.get_single() else { return };

    // Select hostile target
    let hostile_target = select_target(
        *loc,
        *heading,
        tier_lock.locked_tier,
        &nntree,
        &entities,
    );

    // Select ally target (similar logic, filter to allies)
    let ally_target = select_ally_target(*loc, *heading, &nntree, &entities);

    // Update or spawn hostile indicator
    if let Some(target_ent) = hostile_target {
        let target_loc = entities.get(target_ent).unwrap().1;

        // Find existing indicator or spawn new
        let mut found = false;
        for (_, mut transform, indicator) in &mut indicators {
            if matches!(indicator.indicator_type, IndicatorType::Hostile) {
                transform.translation = target_loc.to_world_pos();  // Update position
                found = true;
                break;
            }
        }

        if !found {
            commands.spawn((
                SpriteBundle {
                    sprite: Sprite {
                        color: Color::rgb(1.0, 0.0, 0.0),  // Red
                        custom_size: Some(Vec2::new(1.5, 1.5)),  // Hex outline size
                        ..default()
                    },
                    transform: Transform::from_translation(target_loc.to_world_pos()),
                    ..default()
                },
                TargetIndicator {
                    indicator_type: IndicatorType::Hostile,
                },
            ));
        }
    } else {
        // No target: hide hostile indicator
        for (ent, _, indicator) in &indicators {
            if matches!(indicator.indicator_type, IndicatorType::Hostile) {
                commands.entity(ent).despawn();
            }
        }
    }

    // Similar logic for ally indicator...
}
```

**Visual Feedback Enhancements (Future):**
- Tier badge: Small "1"/"2"/"3" icon on indicator when tier locked
- Range highlight: Highlight valid hexes in tier when no targets (visual feedback that tier lock active but no enemies in range)
- Lock marker: Additional border when TAB manual lock active

---

#### 5. Ability Definitions (Hardcoded Enums)

**Ability Data Structure:**

```rust
pub enum AbilityType {
    // Offensive
    BasicAttack,
    Charge,          // Direct signature (future)
    Volley,          // Distant signature (future)

    // Defensive (Reactions)
    Dodge,           // Evasive signature
    Counter,         // Patient signature (future)
    Ward,            // Shielded signature (future)
}

pub struct AbilityDefinition {
    pub ability_type: AbilityType,
    pub cost: AbilityCost,
    pub gcd_type: GcdType,
    pub targeting: TargetingPattern,
    pub execution: ExecutionType,
    pub effects: Vec<AbilityEffect>,
}

pub enum AbilityCost {
    None,
    Stamina(f32),
    Mana(f32),
}

pub enum TargetingPattern {
    SingleTarget,     // Hits indicated target (red or green indicator)
    SelfTarget,       // No targeting, affects caster only
    LineFacing { length: u8 },     // N hexes in facing direction (future)
    RadiusSelf { radius: u8 },     // Area around caster (future)
    RadiusTarget { radius: u8 },   // Area around indicated target (future)
    AdjacentFacing,   // Hexes in front arc (future)
}

pub enum ExecutionType {
    Instant,
    Projectile { speed: f32, lifetime: Duration },  // Future
    GroundEffect { delay: Duration },  // Future
    Unavoidable,  // Future
}

pub enum AbilityEffect {
    Damage { amount: f32, damage_type: DamageType },
    ClearQueue { clear_type: ClearType },
    // Future: Heal, Buff, Debuff, Knockback, etc.
}
```

**Ability Registry:** `common/systems/abilities.rs`

```rust
pub fn get_ability_definition(ability_type: AbilityType) -> AbilityDefinition {
    match ability_type {
        AbilityType::BasicAttack => AbilityDefinition {
            ability_type: AbilityType::BasicAttack,
            cost: AbilityCost::None,
            gcd_type: GcdType::Attack,
            targeting: TargetingPattern::SingleTarget,  // Hits indicated hostile
            execution: ExecutionType::Instant,
            effects: vec![
                AbilityEffect::Damage {
                    amount: 20.0,  // Base damage, scaled by Might
                    damage_type: DamageType::Physical,
                }
            ],
        },
        AbilityType::Dodge => AbilityDefinition {
            ability_type: AbilityType::Dodge,
            cost: AbilityCost::Stamina(30.0),
            gcd_type: GcdType::Reaction,
            targeting: TargetingPattern::SelfTarget,  // No targeting required
            execution: ExecutionType::Instant,
            effects: vec![
                AbilityEffect::ClearQueue {
                    clear_type: ClearType::All,
                }
            ],
        },
        // Future abilities...
    }
}
```

---

#### 6. Client-Predicted Ability Usage

**Prediction Flow:**

1. **Client initiates:**
   - Player presses ability key (Q for Basic Attack, E for Dodge)
   - Client queries current target indicator (hostile or ally)
   - Client validates targeting (target exists, in range)
   - Client predicts ability effects:
     - Spend resource (stamina/mana step -= cost)
     - Apply effects optimistically (damage prediction in ADR-005)
   - Client sends `Try::UseAbility { ent, ability_type, target: Option<Entity> }`

2. **Server validates:**
   - Check resource cost available
   - Check GCD not active
   - Check target valid (exists, in range, in facing cone - server recalculates target)
   - If valid: spend resource, execute ability, emit confirmations
   - If invalid: emit `Do::AbilityFailed { ent, ability_type, reason }`

3. **Server broadcasts:**
   - `Do::AbilityUsed { ent, ability_type, target: Option<Entity> }`
   - `Do::Stamina/Mana { ent, current, max }` (updated resource)
   - `Do::Gcd { ent, typ, duration }`
   - Ability effects (damage, queue clear, etc. - separate events)

4. **Client receives confirmation:**
   - Local player: effects already applied (prediction correct)
   - Remote players: play ability animation, apply effects
   - If rollback: `Do::AbilityFailed` → undo predicted changes

**Example: BasicAttack Flow**

```rust
// Client: Player presses Q
pub fn handle_ability_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut local_player: Query<(&Loc, &Heading, &TierLock, &mut Stamina), With<Controlled>>,
    nntree: Res<NNTree>,
    entities: Query<(&EntityType, &Loc)>,
    mut writer: EventWriter<Try>,
) {
    if !keyboard.just_pressed(KeyCode::KeyQ) { return; }

    let Ok((loc, heading, tier_lock, mut stamina)) = local_player.get_single_mut() else { return };

    // Select target using same logic as server
    let target = select_target(*loc, *heading, tier_lock.locked_tier, &nntree, &entities);

    let Some(target_ent) = target else {
        // No target: show error message, don't send
        return;
    };

    // Validate range (BasicAttack range = 1, adjacent only)
    let target_loc = entities.get(target_ent).unwrap().1;
    let distance = loc.distance(*target_loc);
    if distance > 1 {
        // Out of range: show error
        return;
    }

    // Client predicts: No resource cost for BasicAttack, but we'd predict stamina spend here if needed

    // Send ability use
    writer.write(Try {
        event: Event::UseAbility {
            ent: local_player_ent,  // Need to store this somewhere
            ability_type: AbilityType::BasicAttack,
            target: Some(target_ent),
        },
    });

    // Clear tier lock if active
    // (handled by separate system)
}
```

**Server Validation:**

```rust
// Server: Process Try::UseAbility
pub fn execute_ability(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    query: Query<(&Loc, &Heading, &Stamina, &Mana, &Gcd)>,
    nntree: Res<NNTree>,
    entities: Query<(&EntityType, &Loc)>,
) {
    for Try { event: Event::UseAbility { ent, ability_type, target } } in reader.read() {
        let Ok((loc, heading, stamina, mana, gcd)) = query.get(*ent) else { continue };

        let def = get_ability_definition(*ability_type);

        // Validate resource cost
        match def.cost {
            AbilityCost::Stamina(cost) if stamina.state < cost => {
                writer.write(Do {
                    event: Event::AbilityFailed {
                        ent: *ent,
                        ability_type: *ability_type,
                        reason: "Not enough stamina".to_string(),
                    },
                });
                continue;
            },
            _ => {},
        }

        // Validate GCD
        if is_gcd_active(*ent, def.gcd_type, gcd) {
            writer.write(Do {
                event: Event::AbilityFailed {
                    ent: *ent,
                    ability_type: *ability_type,
                    reason: "Ability on cooldown".to_string(),
                },
            });
            continue;
        }

        // Validate targeting
        match def.targeting {
            TargetingPattern::SingleTarget => {
                // Recalculate target server-side (don't trust client's target)
                let server_target = select_target(*loc, *heading, None, &nntree, &entities);

                // Accept client's target if it matches server's selection
                // (allows for some latency tolerance)
                let final_target = if Some(server_target) == *target || server_target.is_some() {
                    server_target
                } else {
                    // Client's target doesn't match server's calculation
                    writer.write(Do {
                        event: Event::AbilityFailed {
                            ent: *ent,
                            ability_type: *ability_type,
                            reason: "Invalid target".to_string(),
                        },
                    });
                    continue;
                };

                let Some(target_ent) = final_target else {
                    writer.write(Do {
                        event: Event::AbilityFailed {
                            ent: *ent,
                            ability_type: *ability_type,
                            reason: "No target in range".to_string(),
                        },
                    });
                    continue;
                };

                // Execute ability effects
                for effect in &def.effects {
                    match effect {
                        AbilityEffect::Damage { amount, damage_type } => {
                            writer.write(Try {
                                event: Event::DealDamage {
                                    source: *ent,
                                    target: target_ent,
                                    base_damage: *amount,
                                    damage_type: *damage_type,
                                },
                            });
                        },
                        _ => {},
                    }
                }

                // Broadcast success
                writer.write(Do {
                    event: Event::AbilityUsed {
                        ent: *ent,
                        ability_type: *ability_type,
                        target: Some(target_ent),
                    },
                });

                // Apply GCD
                apply_gcd(*ent, def.gcd_type, Duration::from_millis(500));
            },

            TargetingPattern::SelfTarget => {
                // No targeting validation needed (Dodge, buffs, etc.)
                for effect in &def.effects {
                    match effect {
                        AbilityEffect::ClearQueue { clear_type } => {
                            // Clear queue (ADR-003 logic)
                            writer.write(Do {
                                event: Event::ClearQueue {
                                    ent: *ent,
                                    clear_type: *clear_type,
                                },
                            });
                        },
                        _ => {},
                    }
                }

                // Spend resource
                if let AbilityCost::Stamina(cost) = def.cost {
                    stamina.state -= cost;
                    writer.write(Do {
                        event: Event::Stamina {
                            ent: *ent,
                            current: stamina.state,
                            max: stamina.max,
                            regen_rate: stamina.regen_rate,
                        },
                    });
                }

                // Broadcast success
                writer.write(Do {
                    event: Event::AbilityUsed {
                        ent: *ent,
                        ability_type: *ability_type,
                        target: None,
                    },
                });

                // Apply GCD
                apply_gcd(*ent, def.gcd_type, Duration::from_millis(500));
            },

            _ => {
                // Future: Line, Radius, Adjacent patterns
            },
        }
    }
}
```

---

#### 7. Network Message Structure

**Updated Event Types (add to `common/message.rs`):**

```rust
pub enum Event {
    // Existing events...

    /// Client → Server: Attempt to use ability (Try event)
    UseAbility {
        ent: Entity,
        ability_type: AbilityType,
        target: Option<Entity>,  // Some for SingleTarget, None for SelfTarget
    },

    /// Server → Client: Ability successfully used (Do event)
    AbilityUsed {
        ent: Entity,
        ability_type: AbilityType,
        target: Option<Entity>,
    },

    /// Server → Client: Ability usage failed (Do event)
    AbilityFailed {
        ent: Entity,
        ability_type: AbilityType,
        reason: String,
    },
}
```

**Key Change from Old ADR-004:**
- `target: Option<Entity>` instead of `target_loc: Loc`
- Targeting is entity-based (hit the indicated target), not hex-based
- Server recalculates target using heading + facing cone (validates client's selection)

---

#### 8. MVP Scope and Simplifications

**MVP Includes:**
- Heading system (already exists, integrate with targeting)
- Automatic target selection (nearest in facing direction)
- Target indicator (red hostile, basic visual)
- BasicAttack (hits indicated target)
- Dodge (self-target, no indicator needed)
- Keyboard controls (Q for attack, E for dodge)

**MVP Excludes (Phase 2):**
- Tier lock system (1/2/3 keys) - defer to Phase 2
- TAB cycling - defer to Phase 2
- Green ally indicator - defer to Phase 2
- Complex patterns (Line, Radius, Adjacent)
- Projectiles (instant abilities only)
- Visual polish (tier badges, range highlights)

**Simplification Rationale:**
- MVP validates directional combat core (heading → target selection → ability execution)
- Wild Dog provides sufficient test (melee attacks on nearest player)
- Single automatic targeting simpler than tier lock + TAB
- Phase 2 adds tier lock once automatic targeting proven

---

## Consequences

### Positive

#### 1. Keyboard-Only, No Cursor Required

- Fully playable without mouse (accessibility win)
- Faster combat flow (no click targeting delay)
- Controller-friendly (easy to port to gamepad)
- Matches spec design philosophy ("no cursor required")

#### 2. Directional Combat Depth

- Positioning matters (face enemies to attack)
- Heading creates tactical decisions (turn to engage)
- Flanking gameplay (enemies harder to target if behind)
- Geometric tiebreaker rewards precise positioning

#### 3. Automatic Targeting Reduces Input Burden

- No need to click every attack (just face and press Q)
- Default targeting handles 90% of cases (nearest hostile)
- Tier lock and TAB for edge cases (future)
- Lowers skill floor, maintains skill ceiling

#### 4. Heading System Reuse

- Existing `Heading` component (`NE, E, SE, SW, W, NW`)
- Movement systems already update heading
- No new infrastructure needed
- Seamless integration with controlled movement

#### 5. Shared Targeting Logic

- Client and server use `select_target` function (no desync)
- Pure function, easy to test
- Deterministic (same inputs → same output)
- Extensible (add tier lock, faction filters)

### Negative

#### 1. No Mouse Targeting Flexibility

- Can't manually select specific target (must face it)
- Geometric tiebreaker may not match player intent (edge cases)
- TAB cycling needed for manual override (deferred to Phase 2)
- Some players may prefer click-targeting (design tradeoff)

**Mitigation:**
- Phase 2 adds TAB cycling for manual control
- Tier lock helps with "caster wants backline" scenario
- Playtesting will validate automatic targeting sufficiency

#### 2. Heading Discretization (6 Directions)

- Only 6 headings (60° each), not continuous 360°
- Facing cone always 60° (can't narrow/widen)
- Some targets may fall between heading boundaries
- Movement required to change heading (no turn-in-place)

**Mitigation:**
- Hex grid naturally discrete (6 directions align with hex faces)
- Tiebreaker handles boundary cases (closest to heading angle)
- Design intent: movement-based combat (turn by moving)

#### 3. Target Indicator Update Frequency

- Runs every frame for local player (potential overhead)
- Recalculates `select_target` each frame (spatial query + filtering)
- 100 entities in combat = 100 indicator updates/sec

**Mitigation:**
- NNTree queries fast (spatial index optimized)
- Facing cone filter cheap (angular comparison)
- Only local player updates every frame (remote entities don't need indicators)
- Optimize if profiling shows issue (dirty flag, reduce update rate)

#### 4. Server Target Recalculation Mismatch

- Client selects target, sends to server
- Server recalculates target (may differ due to latency)
- If mismatch: ability fails with "Invalid target"
- Frustrating if frequent (entity moved between client/server)

**Mitigation:**
- Tolerance: Server accepts client's target if "close enough"
- Small latency window (100-200ms typical)
- Rare for stationary targets (Wild Dog)
- Visual feedback ("Invalid target" message, retry)

#### 5. Tier Lock Deferred to Phase 2

- MVP lacks tier lock (no 1/2/3 keys)
- Can't manually select far target if close target exists
- Limits tactical options (caster can't target backline easily)

**Mitigation:**
- MVP combat simple (Wild Dog melee only, no backline)
- Phase 2 adds tier lock before complex encounters
- Automatic targeting sufficient for single-enemy MVP

### Neutral

#### 1. Heading Component Already Exists

- No new component needed (reuse existing)
- Movement systems already update heading
- Targeting just queries heading for cone calculation
- Risk: Heading may need modification for other features

#### 2. Target Indicator Visual Design TBD

- MVP uses simple red outline/ground marker
- Visual polish deferred (tier badges, lock markers)
- May need iteration based on playtest feedback
- Not architecturally critical (client-side only)

#### 3. TAB Cycling Complexity Deferred

- Adds state management (TargetingOverride component)
- Cycle logic non-trivial (wrap-around, invalidation)
- MVP avoids this complexity (automatic only)
- Phase 2 implementation clear (designed in this ADR)

---

## Implementation Phases

### Phase 1: Heading to Facing Cone Conversion (Foundation)

**Goal:** Convert 6-direction heading into 60° facing cone for targeting

**Tasks:**
1. Create `common/systems/targeting.rs`:
   - `Heading::to_angle() -> f32` (NE=30°, E=90°, etc.)
   - `is_in_facing_cone(heading, caster_loc, target_loc) -> bool`
   - Add tests: Heading::E (90°), target at 80° → true (within ±30°), target at 140° → false

2. Unit tests for angle calculations:
   - Heading::NE (30°) with target at 10° → true
   - Heading::E (90°) with target at 150° → false
   - Boundary cases: exactly 60° apart → true, 61° → false

**Success Criteria:**
- All heading-to-angle conversions correct (6 directions)
- Facing cone detection accurate (within 60° arc)
- Tests pass for all 6 headings and various target angles

**Duration:** 1 day

---

### Phase 2: Automatic Target Selection System

**Goal:** Implement `select_target` function with geometric tiebreaker

**Tasks:**
1. Extend `common/systems/targeting.rs`:
   - `select_target(loc, heading, tier_lock, nntree, query) -> Option<Entity>`
   - `get_range_tier(distance) -> RangeTier` (close 1-2, mid 3-6, far 7+)

2. Algorithm implementation:
   - Query nearby entities (NNTree within 20 hexes)
   - Filter to actors in facing cone
   - Apply tier filter if locked
   - Select nearest by distance
   - Geometric tiebreaker: closest to exact heading angle

3. Unit tests:
   - Caster facing E, entities at (1, 0) and (0, 1) → select (1, 0) (east-most)
   - Caster facing E, entities at (1, 0) distance 1 and (2, 0) distance 2 → select (1, 0) (nearest)
   - Tier lock close, entities at distance 1 and 5 → select distance 1 only

**Success Criteria:**
- Target selection deterministic (same inputs → same target)
- Geometric tiebreaker resolves equidistant targets correctly
- Tier lock filters correctly (close/mid/far)

**Duration:** 2 days

---

### Phase 3: Client-Side Target Indicator Rendering

**Goal:** Show red indicator on current hostile target

**Tasks:**
1. Create `client/systems/targeting_ui.rs`:
   - `render_target_indicators` system (runs in Update)
   - Query local player's Loc, Heading
   - Call `select_target` to get hostile target
   - Spawn/update indicator entity (red sprite at target's position)

2. Visual component:
   - Red outline around target (SpriteBundle with red color)
   - Ground marker (circle or hex highlight)
   - Follow target position (update every frame)

3. Indicator lifecycle:
   - Spawn when target exists
   - Update position when target moves or selection changes
   - Despawn when no target (out of range, facing changes)

**Success Criteria:**
- Red indicator visible on nearest hostile in facing direction
- Indicator updates smoothly as player moves/rotates
- Indicator disappears when no valid targets

**Duration:** 2 days

---

### Phase 4: Ability Execution with Directional Targeting

**Goal:** BasicAttack and Dodge integrate with targeting system

**Tasks:**
1. Update `common/systems/abilities.rs`:
   - Add `TargetingPattern::SingleTarget` (uses indicated target)
   - Add `TargetingPattern::SelfTarget` (no targeting)

2. Create `client/systems/abilities::handle_ability_input`:
   - Q key: Use BasicAttack
   - E key: Use Dodge
   - Query current target indicator (red hostile)
   - Validate range (BasicAttack = adjacent only)
   - Send `Try::UseAbility { ent, ability_type, target }`

3. Update `server/systems/abilities::execute_ability`:
   - Recalculate target server-side using `select_target`
   - Validate client's target matches server's (with tolerance)
   - Execute ability effects if valid
   - Emit `Do::AbilityFailed` if invalid

**Success Criteria:**
- Player faces Wild Dog, presses Q → BasicAttack hits Dog
- Player presses E → Dodge clears queue (no targeting)
- Player faces away from Dog, presses Q → "No target" error
- Dog attacks player (server uses `select_target` for AI)

**Duration:** 3 days

---

### Phase 5: Client Prediction for Abilities

**Goal:** Client predicts ability usage for local player

**Tasks:**
1. Create `client/systems/abilities::predict_ability_usage`:
   - Predict resource spend (stamina for Dodge)
   - Predict effects (queue clear, damage - ADR-005)
   - Update UI immediately (stamina bar, queue)

2. Rollback handling:
   - Receive `Do::AbilityFailed` → undo predictions
   - Restore stamina (snap to server's value)
   - Show error message

3. Confirmation handling:
   - Receive `Do::AbilityUsed` → prediction correct (no change)
   - Update GCD UI (dim ability icons)

**Success Criteria:**
- Local player presses E → stamina drops instantly (predicted)
- Server confirms → no visual snap (prediction correct)
- Server denies → stamina restores, error message shown

**Duration:** 2 days

---

### Phase 6: Enemy AI Directional Targeting

**Goal:** Wild Dog uses directional targeting to attack player

**Tasks:**
1. Update `server/systems/ai/wild_dog.rs`:
   - Query Dog's Loc, Heading
   - Use `select_target` to find nearest player in facing direction
   - If target in range 1 (adjacent): attack
   - If target farther: pathfind toward target, update heading to face target

2. Heading updates during movement:
   - When Dog moves toward player, update heading to face movement direction
   - Heading updates naturally from movement (same as player)

3. Attack pattern:
   - Dog faces player, attacks every 2 seconds
   - Uses `Try::UseAbility { ent, ability_type: BasicAttack, target: player }`
   - Server executes BasicAttack, inserts threat into player's queue

**Success Criteria:**
- Wild Dog faces player and attacks (hits player)
- Dog turns to track player movement (heading updates)
- Dog attacks insert threats into player's reaction queue

**Duration:** 2 days

---

## Validation Criteria

### Functional Tests

- **Heading Conversion:** Heading::E → 90°, Heading::NW → 330° (all 6 directions correct)
- **Facing Cone:** Heading::E, target at 80° → in cone, target at 150° → out of cone
- **Target Selection:** Caster facing E, entities at (1,0) and (0,1) → select (1,0) (geometric tiebreaker)
- **Tier Lock:** Lock to close tier, entities at distance 1 and 5 → select distance 1 only
- **BasicAttack:** Player faces Dog, presses Q → Dog takes damage (threat queued)
- **Dodge:** Player presses E → queue clears, stamina spent

### Network Tests

- **Target Sync:** Client selects target, server recalculates, match within tolerance (latency < 200ms)
- **Prediction:** Client predicts ability, server confirms within 100ms
- **Rollback:** Server denies ability, client rolls back within 1 frame

### Performance Tests

- **Indicator Update:** 60fps maintained with indicator updating every frame
- **Target Selection:** 100 entities, `select_target` runs < 1ms
- **AI Targeting:** 100 Wild Dogs targeting players, CPU < 5% (single core)

### UX Tests

- **Targeting Clarity:** Player understands which enemy will be hit (red indicator obvious)
- **Ability Responsiveness:** Ability executes within 16ms of key press (local player)
- **Directional Feel:** Positioning matters, facing enemies rewarded

---

## Open Questions

### Design Questions

1. **Heading Continuous vs Discrete?**
   - Current: 6 discrete headings (60° each)
   - Alternative: 360° continuous (more precision, more complexity)
   - MVP: Discrete (matches hex grid, simpler)

2. **Target Indicator Visibility?**
   - Always visible (even no targets)?
   - Only when targets exist (cleaner)?
   - MVP: Only when targets exist

3. **Tier Lock Drops on Ability vs Manual Clear?**
   - Spec: Drops after 1 ability use (single-use lock)
   - Alternative: Persists until ESC pressed
   - MVP: Follow spec (drops after 1 ability)

### Technical Questions

1. **Heading Storage: Component or Computed?**
   - Current: Component (exists in codebase)
   - Alternative: Compute from last movement direction (stateless)
   - MVP: Component (already exists, use as-is)

2. **Target Indicator: Entity or UI Overlay?**
   - Entity: Spawn sprite entity at target position (world-space)
   - UI Overlay: Project target to screen-space, draw UI (screen-space)
   - MVP: Entity (simpler, reuses existing sprite systems)

3. **Server Target Tolerance?**
   - How much mismatch acceptable between client/server target selection?
   - Option: Accept client's target if within 1 hex of server's target
   - MVP: Strict match (must be same entity), relax if issues arise

---

## Future Enhancements (Out of Scope)

### Phase 2 Extensions

- **Tier Lock System:** 1/2/3 keys lock to range tiers, drops after 1 ability
- **TAB Cycling:** Cycle through valid targets in tier, persist until ESC
- **Green Ally Indicator:** Show nearest ally for friendly abilities
- **Visual Polish:** Tier badges, lock markers, range highlights
- **Complex Patterns:** Line, Radius, Adjacent targeting
- **Projectile System:** Traveling projectiles with visual arcs

### Optimization

- **Target Selection Caching:** Cache valid targets for frame, update on movement
- **Indicator Dirty Flag:** Only update indicator when target changes (not every frame)
- **Spatial Event System:** Trigger target updates on entity movement (event-driven, not polling)

### Advanced Features

- **Facing Requirement Abilities:** Some abilities only work if target in front (back-stab immunity)
- **Flanking Bonuses:** Extra damage if attacking from behind (target's heading opposite)
- **Cone Width Modifiers:** Abilities that widen/narrow facing cone (focused vs wide attack)

---

## References

### Specifications

- **Combat System:** `docs/spec/combat-system.md` (directional combat, heading, tier lock, target indicators, MVP abilities)
- **Triumvirate System:** `docs/spec/triumvirate.md` (signature skill mapping)

### Codebase

- **Heading Component:** `src/common/components/heading.rs` (6 directions: NE, E, SE, SW, W, NW)
- **Movement Systems:** `src/client/systems/controlled.rs` (movement updates heading)
- **GCD System:** `src/common/systems/gcd.rs` (GcdType enum, cooldown tracking)
- **NNTree:** `src/common/plugins/nntree.rs` (spatial queries for entity detection)

### Related ADRs

- **ADR-002:** Combat Foundation (resources, GCD infrastructure)
- **ADR-003:** Reaction Queue System (ClearQueue effects, Dodge ability)
- **ADR-005:** Damage Pipeline (damage generation, InsertThreat event)

---

## Decision Makers

- ARCHITECT role evaluation
- Game design requirements: `docs/spec/combat-system.md` (directional combat design)
- Existing Heading and movement systems integration

## Date

2025-10-30 (Updated from 2025-10-29 to reflect directional combat system)
