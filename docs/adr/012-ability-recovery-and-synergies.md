# ADR-012: Ability Recovery System and Tactical Synergies

## Status
Accepted (2025-11-07)

**Acceptance:** [012-acceptance.md](012-acceptance.md)

## Context

### Problem Statement

The current combat system uses a fixed 0.5s Global Cooldown (GCD) that creates uniform pacing regardless of ability commitment. This fails to reward tactical sequencing and creates monotonous combat flow.

**Current Issues:**
- All abilities share identical 0.5s GCD (no commitment differentiation)
- No reward for tactical sequencing - all ability orders feel the same
- High-impact abilities (Overpower) have same recovery as quick reactions (Knockback)
- Resource costs are the only throttle, leading to binary "spam until empty" gameplay

**Design Goals (from spec Lines 7-13, 1095-1096):**
- Recovery timers reflect ability commitment weight (quick reactions vs heavy strikes)
- Synergies reward tactical adaptation, not memorized rotations
- Conscious but decisive - no artificial delays between different abilities
- Fluid combos through reduced recovery on tactical follow-ups

### Spec References

**Ability Recovery System:** [combat-system.md](../spec/combat-system.md) Lines 352-381
**Tactical Synergies:** [combat-system.md](../spec/combat-system.md) Lines 383-456
**Feature Matrix:** [combat-system-feature-matrix.md](../spec/combat-system-feature-matrix.md) Lines 122-164

---

## Decision

**Replace fixed GCD with per-ability recovery durations that create universal lockout periods. Add tactical synergies that allow early unlock of specific abilities during lockout.**

When you use an ability, ALL abilities are locked for that ability's recovery duration. However, synergizing abilities can unlock early, glowing immediately to show they'll be available before the full recovery completes. This rewards tactical sequencing without forcing memorized rotations.

---

## Technical Design

### Core Mechanic Summary

**Recovery = Universal Lockout**
- Using an ability creates a lockout period where ALL abilities are disabled
- Lockout duration varies by ability: Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s
- Single `GlobalRecovery` component tracks lockout (not per-ability cooldowns)

**Synergies = Early Unlock During Lockout**
- Certain ability sequences allow specific abilities to unlock before full recovery
- Example: Use Lunge (1s lockout) → Overpower unlocks at 0.5s (instead of 1s)
- `SynergyUnlock` component marks which abilities can be used early
- Glow appears **immediately** when ability is used (not when unlock time arrives)
- Glow persists until full recovery completes (shows "this will unlock early")

**Visual Layering**
- Synergy glow is **additive** - gold border/particles layered on top of base state color
- Grey + Gold Glow = "Locked but will unlock early"
- Green + Gold Glow = "Unlocked via synergy, ready to use"

---

### Phase 1: Recovery Lockout System

#### Component Structure

```rust
/// Universal ability lockout timer (single component per player)
#[derive(Component, Debug, Clone)]
pub struct GlobalRecovery {
    pub remaining: f32,          // Seconds until ALL abilities unlock
    pub duration: f32,           // Total duration of current recovery
    pub triggered_by: AbilityKey, // Which ability triggered this lockout
}

/// Marks an ability as synergy-available (glowing, can use early)
#[derive(Component, Debug, Clone)]
pub struct SynergyUnlock {
    pub ability_key: AbilityKey,  // Which ability can unlock early
    pub unlock_at: f32,           // When this ability becomes available (relative to recovery)
    pub triggered_by: AbilityKey, // Which ability triggered this synergy
}
```

**Key Changes from Current System:**
- `GlobalRecovery` replaces per-ability cooldowns (universal lockout)
- `SynergyUnlock` marks abilities that can be used before lockout expires
- Glow starts immediately when ability is used (not when window opens)

#### MVP Ability Recovery Durations (Universal Lockout)

| Ability | Type | Lockout Duration | Notes |
|---------|------|------------------|-------|
| Lunge | Gap Closer | 1.0s | ALL abilities locked for 1s after use |
| Overpower | Heavy Strike | 2.0s | ALL abilities locked for 2s after use |
| Knockback | Push | 0.5s | ALL abilities locked for 0.5s after use |
| Deflect | Defensive | 1.0s | ALL abilities locked for 1s after use |

**Design Rationale:**
- Recovery creates universal lockout (not per-ability cooldown)
- Heavy commitment (Overpower) locks you out longer than quick reactions (Knockback)
- MVP uses longer durations for clearer feedback and testing
- Production values can be tuned down to spec ranges (0.2-1.2s) after validation
- Synergies allow early unlock during lockout (see Phase 2)

#### Recovery System

```rust
pub fn global_recovery_system(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut GlobalRecovery)>,
) {
    for (entity, mut recovery) in query.iter_mut() {
        if recovery.remaining > 0.0 {
            recovery.remaining -= time.delta_seconds();

            // Lockout expired, remove component
            if recovery.remaining <= 0.0 {
                commands.entity(entity).remove::<GlobalRecovery>();
            }
        }
    }
}
```

**Execution Order:** After ability execution, before UI update

#### Ability Execution Integration

When an ability is used:

```rust
pub fn trigger_recovery_lockout(
    ability_key: AbilityKey,
    caster_entity: Entity,
    commands: &mut Commands,
) {
    let duration = get_ability_recovery_duration(ability_key);

    // Insert universal lockout (replaces any existing lockout)
    commands.entity(caster_entity).insert(GlobalRecovery {
        remaining: duration,
        duration,
        triggered_by: ability_key,
    });
}
```

**Integration Points:**
- Called from ability execution systems (lunge.rs, overpower.rs, etc.)
- Replaces GCD insertion logic
- Single `GlobalRecovery` component per player (overwrites previous lockout)
- All abilities check `GlobalRecovery` to determine if usable

---

### Phase 2: Tactical Synergies

#### Synergy Rule Definition

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynergyTrigger {
    GapCloser,  // Lunge
    HeavyStrike, // Overpower
    Push,       // Knockback
    Defensive,  // Deflect
}

#[derive(Debug, Clone)]
pub struct SynergyRule {
    pub trigger: SynergyTrigger,
    pub target: AbilityKey,
    pub unlock_reduction: f32, // How much earlier to unlock target (in seconds)
}

/// MVP Synergy Rules (hardcoded for Phase 1, data-driven later)
pub const MVP_SYNERGIES: &[SynergyRule] = &[
    // Gap Closer → Heavy Strike: Overpower unlocks 0.5s early during Lunge recovery
    SynergyRule {
        trigger: SynergyTrigger::GapCloser,
        target: AbilityKey::Overpower,
        unlock_reduction: 0.5, // Overpower available at 0.5s instead of 1.0s
    },

    // Heavy Strike → Push: Knockback unlocks 1.0s early during Overpower recovery
    SynergyRule {
        trigger: SynergyTrigger::HeavyStrike,
        target: AbilityKey::Knockback,
        unlock_reduction: 1.0, // Knockback available at 1.0s instead of 2.0s
    },
];
```

**MVP Synergy Chain:**
```
t=0.0s: Use Lunge → 1s lockout starts, Overpower glows immediately
t=0.5s: Overpower unlocks (synergy), still locked otherwise
t=1.0s: Full recovery, all abilities unlock, Overpower stops glowing

t=0.0s: Use Overpower → 2s lockout starts, Knockback glows immediately
t=1.0s: Knockback unlocks (synergy), still locked otherwise
t=2.0s: Full recovery, all abilities unlock, Knockback stops glowing
```

**Tactical Logic:**
- **Lunge → Overpower**: Closed gap, capitalize with AoE before full recovery
- **Overpower → Knockback**: Heavy hit destabilizes, push immediately to create space

#### Synergy Detection System

```rust
pub fn detect_synergies_system(
    ability_events: EventReader<AbilityUsedEvent>,
    mut commands: Commands,
    player_query: Query<(Entity, &AbilityBar, &GlobalRecovery)>,
    synergy_rules: Res<SynergyRules>,
) {
    for event in ability_events.iter() {
        let trigger_type = get_synergy_trigger_type(event.ability_key);

        if let Ok((player_entity, ability_bar, recovery)) = player_query.get(event.caster) {
            // Find synergizing abilities
            for rule in synergy_rules.iter() {
                if rule.trigger == trigger_type {
                    // Check if player has the target ability slotted
                    if ability_bar.has_ability(rule.target) {
                        // Calculate unlock time: recovery.remaining - unlock_reduction
                        let unlock_at = (recovery.remaining - rule.unlock_reduction).max(0.0);

                        // Apply synergy unlock immediately (glow starts now)
                        commands.entity(player_entity).insert(SynergyUnlock {
                            ability_key: rule.target,
                            unlock_at,
                            triggered_by: event.ability_key,
                        });

                        // Trigger audio feedback
                        commands.spawn(AudioEvent::SynergyTriggered);
                    }
                }
            }
        }
    }
}
```

**Execution Order:** Immediately after `trigger_recovery_lockout`, same frame as ability use

**Timing Notes:**
- Synergy detection runs AFTER lockout is applied (needs `GlobalRecovery` component)
- Glow appears immediately (not delayed until unlock time)
- Unlock time is relative to lockout start (not absolute game time)

#### Checking Synergy Availability

When player attempts to use an ability:

```rust
pub fn can_use_ability(
    ability_key: AbilityKey,
    player_entity: Entity,
    recovery_query: &Query<&GlobalRecovery>,
    synergy_query: &Query<&SynergyUnlock>,
) -> bool {
    // Check if universal lockout is active
    if let Ok(recovery) = recovery_query.get(player_entity) {
        // Check if synergy unlocks this ability early
        if let Ok(synergy) = synergy_query.get(player_entity) {
            if synergy.ability_key == ability_key {
                // Synergy active: check if unlock time reached
                return recovery.remaining <= synergy.unlock_at;
            }
        }

        // No synergy: locked until full recovery
        return false;
    }

    // No lockout: ability available
    true
}
```

**Integration:**
- Ability systems check this before executing
- Synergy allows early use during lockout
- Non-synergized abilities remain locked until full recovery
- Synergy consumed when ability is used (remove `SynergyUnlock` component)

#### Synergy Cleanup

```rust
pub fn synergy_cleanup_system(
    mut commands: Commands,
    recovery_query: Query<Entity, Without<GlobalRecovery>>,
    synergy_query: Query<(Entity, &SynergyUnlock)>,
) {
    // Find players without active lockout
    for player_entity in recovery_query.iter() {
        // Remove any synergy unlocks (lockout expired)
        for (synergy_entity, synergy) in synergy_query.iter() {
            commands.entity(synergy_entity).remove::<SynergyUnlock>();
        }
    }
}
```

**Cleanup Logic:**
- Synergy glow removed when `GlobalRecovery` expires (full recovery complete)
- Glow persists for entire lockout duration (spec Line 422)
- Using synergized ability also removes its `SynergyUnlock` component
- Multiple synergies can coexist (different `SynergyUnlock` components per ability)

---

### Phase 3: Visual Feedback

#### Recovery Timer UI (Action Bar)

**Location:** Action bar ability icons (existing system in [ADR-008](008-combat-hud.md))

**Visual Elements:**
1. **Circular progress indicator** around ability icon (matches reaction window UI pattern)
   - Empty when lockout starts
   - Fills as lockout completes
   - Full circle = all abilities ready
2. **Icon state colors** (existing system, unchanged):
   - **Green**: Ability available
   - **Yellow**: Out of range (available but target invalid)
   - **Grey**: Locked (in recovery or insufficient resources)
3. **Recovery timer text** (optional, configurable): "1.2s"

**Bevy UI Implementation:**
```rust
// Universal lockout progress (applies to all ability icons)
fn update_recovery_ui(
    recovery_query: Query<&GlobalRecovery>,
    mut icon_query: Query<&mut AbilityIcon>,
    synergy_query: Query<&SynergyUnlock>,
) {
    if let Ok(recovery) = recovery_query.get_single() {
        let progress = 1.0 - (recovery.remaining / recovery.duration);

        for mut icon in icon_query.iter_mut() {
            // Show lockout progress on all icons
            icon.set_progress(progress); // 0.0 = empty, 1.0 = full

            // Check if this ability has synergy unlock
            let has_synergy = synergy_query.iter().any(|s| s.ability_key == icon.ability_key);

            if has_synergy && recovery.remaining <= synergy.unlock_at {
                // Synergy unlocked early
                icon.set_state(AbilityState::Available);
            } else if recovery.remaining > 0.0 {
                // Still locked
                icon.set_state(AbilityState::Locked);
            }
        }
    } else {
        // No lockout: all abilities available
        for mut icon in icon_query.iter_mut() {
            icon.set_progress(1.0);
            icon.set_state(AbilityState::Available);
        }
    }
}
```

**Integration Point:** Combat HUD update system (runs every frame)

#### Synergy Glow UI (Additive Effect)

**IMPORTANT:** Synergy glow is **layered on top of** existing ability state indicators. It does not replace green/yellow/grey colors.

**Combined Visual States:**
| Base State | + Synergy Glow | Result |
|------------|----------------|--------|
| Grey (locked) | Gold border + particles | "Will unlock early" |
| Green (available) | Gold border + particles | "Unlocked via synergy, ready" |
| Yellow (out of range) | Gold border + particles | "Unlocked via synergy, target invalid" |

**Glow Visual Elements:**
1. **Bright gold border** (3-5px) around ability icon (additive, doesn't replace base color)
2. **Particle effects** (gold sparkles) emanating from icon edges
3. **Pulsing animation** (subtle scale or brightness pulse)
4. **Icon brightness boost** (+20% luminosity, preserves base color)

**Bevy UI Implementation:**
```rust
fn update_synergy_glow_ui(
    synergy_query: Query<&SynergyUnlock>,
    mut icon_query: Query<&mut AbilityIcon>,
) {
    // Clear all glows first
    for mut icon in icon_query.iter_mut() {
        icon.set_glow(false);
    }

    // Apply glows to synergized abilities
    for synergy in synergy_query.iter() {
        if let Some(mut icon) = icon_query.iter_mut().find(|i| i.ability_key == synergy.ability_key) {
            // Add glow effect (does NOT change base color)
            icon.set_glow(true);
            icon.set_glow_color(Color::rgba(1.0, 0.9, 0.3, 0.8)); // Gold with alpha
            icon.set_particle_effects(true);
            icon.set_brightness_boost(1.2); // 20% brighter
        }
    }
}
```

**Particle System:**
- Small gold sparkles around icon edges (3-4px offset)
- Upward drift animation (20px/sec)
- Spawns 2-3 particles per frame while glowing
- Fade out over 0.3s lifetime
- Respects icon's base color (doesn't obscure it)

#### Audio Feedback

**Synergy Trigger Sound:**
- File: `assets/audio/synergy_trigger.ogg`
- Volume: 0.6 (noticeable but not overwhelming)
- Description: Satisfying "ding" or "chime" sound
- Plays when synergy is detected and glow applied

**Synergy Use Sound:**
- File: `assets/audio/synergy_use.ogg`
- Volume: 0.8 (reinforcing feedback)
- Description: Impactful "whoosh" or "power-up" sound
- Plays when glowing ability is activated
- Layered on top of ability's normal activation sound

**Implementation:**
```rust
pub fn play_synergy_audio(
    audio_events: EventReader<AudioEvent>,
    audio: Res<Audio>,
    audio_assets: Res<AudioAssets>,
) {
    for event in audio_events.iter() {
        match event {
            AudioEvent::SynergyTriggered => {
                audio.play(audio_assets.synergy_trigger.clone())
                    .with_volume(0.6);
            }
            AudioEvent::SynergyUsed => {
                audio.play(audio_assets.synergy_use.clone())
                    .with_volume(0.8);
            }
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Recovery System Foundation (MVP)
**Goal:** Replace GCD with individual recovery timers

1. **Create recovery components** (`AbilityRecovery`)
2. **Implement recovery tick system** (countdown logic)
3. **Integrate with ability execution** (trigger recovery on use)
4. **Remove GCD system** (delete old components/systems)
5. **Add recovery UI** (circular progress on action bar)
6. **Set MVP recovery values** (Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s)

**Test Criteria:**
- Each ability has independent recovery timer
- Can use different abilities while one is recovering
- UI shows recovery progress clearly
- Resource costs still apply (stamina)

---

### Phase 2: Synergy Detection (MVP)
**Goal:** Detect tactical sequences and apply bonuses

1. **Define synergy rules** (`SynergyRule` data structure)
2. **Implement MVP synergies** (Gap Closer → Heavy, Heavy → Push)
3. **Create synergy detection system** (listens to `AbilityUsedEvent`)
4. **Apply `SynergyGlow` component** when synergy detected
5. **Modify recovery application** (check for glow, use reduced recovery)
6. **Implement glow expiration** (timer-based cleanup)

**Test Criteria:**
- Using Lunge makes Overpower glow
- Using Overpower makes Knockback glow (instant)
- Glowing abilities use reduced recovery when activated
- Glow expires after window closes (~2.5s)

---

### Phase 3: Visual Feedback (MVP Polish)
**Goal:** Make synergies discoverable through UI

1. **Add glow border rendering** (gold outline on icons)
2. **Implement particle effects** (sparkles around glowing icons)
3. **Add audio feedback** (synergy trigger + use sounds)
4. **Create pulsing animation** (draw attention to glow)
5. **Test visibility** (ensure glow clear during combat)

**Test Criteria:**
- Glowing abilities are immediately obvious
- Audio cues reinforce synergy activation
- New players naturally press glowing abilities
- Visual feedback doesn't obscure other UI elements

---

### Phase 4: Data-Driven Synergies (Post-MVP)
**Goal:** Extensible synergy system for future abilities

1. **Move synergies to data files** (RON/JSON configuration)
2. **Define synergy tagging system** (ability tags: `gap_closer`, `aoe`, `defensive`)
3. **Implement tag-based rules** (any `gap_closer` → any `heavy_strike`)
4. **Create synergy editor tools** (for designers to add new synergies)
5. **Support multiple synergy sources** (ability glows from multiple triggers)

**Test Criteria:**
- New abilities can define synergy tags in data
- Synergy rules configurable without code changes
- Multiple synergies can stack on same ability
- System scales to 50+ abilities without performance issues

---

## System Execution Order

```
--- Ability Use (Single Frame) ---
1. ability_execution_system (lunge.rs, overpower.rs, etc.)
   └─> Triggers ability effects (damage, movement, etc.)

2. trigger_recovery_lockout
   └─> Inserts GlobalRecovery component (universal lockout starts)

3. detect_synergies_system
   └─> Checks for synergy rules matching used ability
   └─> Inserts SynergyUnlock for target abilities (glow starts immediately)
   └─> Spawns synergy audio event

--- Every Frame ---
4. global_recovery_system
   └─> Decrements lockout timer (remaining -= delta)
   └─> Removes GlobalRecovery when expired

5. synergy_cleanup_system
   └─> Removes SynergyUnlock when GlobalRecovery expires

6. update_recovery_ui
   └─> Updates circular progress on action bar icons
   └─> Updates ability state (locked vs available)
   └─> Checks synergy unlocks for early availability

7. update_synergy_glow_ui
   └─> Renders glow effects (border, particles) on synergized abilities
   └─> Additive layer on top of base ability state colors

8. play_synergy_audio
   └─> Plays audio feedback for synergy events
```

**Critical Dependencies:**
- Synergy detection MUST run AFTER lockout insertion (needs `GlobalRecovery` component)
- UI updates MUST run after recovery/synergy systems (read latest state)
- Synergy cleanup MUST run before UI updates (prevent stale glows)
- Glow UI MUST NOT override base ability state colors (additive only)

---

## Data Structures Summary

### Components

```rust
/// Universal ability lockout (single component per player)
#[derive(Component)]
pub struct GlobalRecovery {
    pub remaining: f32,          // Seconds until ALL abilities unlock
    pub duration: f32,           // Total duration of lockout
    pub triggered_by: AbilityKey, // Which ability triggered this lockout
}

/// Synergy early unlock marker (multiple per player, one per synergized ability)
#[derive(Component)]
pub struct SynergyUnlock {
    pub ability_key: AbilityKey,  // Which ability can unlock early
    pub unlock_at: f32,           // Lockout time when this ability becomes available
    pub triggered_by: AbilityKey, // Which ability triggered this synergy
}
```

**Component Lifetime:**
- `GlobalRecovery`: Inserted on ability use, removed when lockout expires
- `SynergyUnlock`: Inserted on ability use (immediately), removed when lockout expires OR ability is used

### Resources

```rust
/// Global synergy rule definitions (loaded from data in Phase 4)
#[derive(Resource)]
pub struct SynergyRules {
    pub rules: Vec<SynergyRule>,
}

#[derive(Debug, Clone)]
pub struct SynergyRule {
    pub trigger: SynergyTrigger,
    pub target: AbilityKey,
    pub unlock_reduction: f32, // How much earlier to unlock (in seconds)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SynergyTrigger {
    GapCloser,   // Lunge
    HeavyStrike, // Overpower
    Push,        // Knockback
    Defensive,   // Deflect
}
```

**MVP Hardcoded Rules:**
- Lunge (GapCloser) → Overpower unlocks 0.5s early
- Overpower (HeavyStrike) → Knockback unlocks 1.0s early

### Events

```rust
/// Fired when ability is used (existing system)
pub struct AbilityUsedEvent {
    pub caster: Entity,
    pub ability_key: AbilityKey,
}

/// Audio feedback events
pub enum AudioEvent {
    SynergyTriggered,
    SynergyUsed,
}
```

---

## Consequences

### Positive

✅ **Variable commitment pacing** - Heavy abilities (2s lockout) feel impactful, quick reactions (0.5s) stay responsive

✅ **Tactical depth without memorization** - Synergies reward smart sequencing, not rigid rotations

✅ **Self-teaching system** - Immediate glow + audio guides players to discover synergies naturally

✅ **Build diversity foundation** - Different weapons/armor can have unique synergy patterns

✅ **Accessible skill ceiling** - Works without synergies (base lockouts acceptable), better with them

✅ **Visible mastery** - Skilled players chain glowing abilities early, creating satisfying flow state

✅ **Spec alignment** - Directly implements spec Lines 352-456 design intent

✅ **Queued input ready** - Glow starts immediately, supports future input queue system

### Negative

⚠️ **Increased UI complexity** - Additive glow layer on top of existing state indicators

⚠️ **Balancing challenge** - Lockout durations and unlock reductions require iteration

⚠️ **State tracking complexity** - Universal lockout + per-ability synergy unlocks + base state colors

⚠️ **Animation timing dependencies** - Lockout must sync with ability animations for feel

⚠️ **Multiple components per player** - `GlobalRecovery` + N×`SynergyUnlock` (one per synergized ability)

### Neutral

🔹 **Replaces fixed GCD with variable lockout** - Commitment to per-ability pacing system

🔹 **Data-driven future** - Post-MVP requires configuration system for synergy rules (Phase 4)

🔹 **Performance consideration** - Synergy detection runs on every ability use (acceptable for 4-12 abilities)

---

## Open Questions

**Lockout Tuning:**
- Are MVP lockout durations correct? (Lunge 1s, Overpower 2s, Knockback 0.5s, Deflect 1s)
- Should defensive abilities (Deflect) have shorter lockouts than offensive abilities?
- Do lockout durations scale with attribute stats? (e.g., Instinct reduces lockout duration?)
- Should using a synergized ability early trigger shorter lockout? (reward for synergy use?)

**Synergy Design:**
- Should Knockback → Lunge be a reverse synergy? (push creates gap, close back in?)
- Should defensive abilities trigger synergies? (Deflect → Counter-attack pattern?)
- How many synergy chains should be possible before stamina depletes?
- Can multiple abilities trigger the same synergy target? (if so, does it extend the window?)

**Visual Feedback:**
- Is additive glow (gold border + particles) obvious enough during combat chaos?
- Should glow intensity scale with unlock reduction magnitude? (bigger reduction = brighter glow?)
- Do we need tutorial hints for first-time synergy discoveries?
- Should circular progress show synergy unlock time? (mark at 50% for Overpower during Lunge?)

**Technical:**
- How do synergies interact with interrupt/stagger mechanics? (future system - does interrupt clear synergies?)
- Should synergies work with ability queue system? (future: queue glowing ability, executes at unlock time?)
- Do NPC enemies get synergies, or player-only mechanic?
- Should synergies persist through death/respawn? (probably not, but worth documenting)

---

## References

- **Spec:** [combat-system.md](../spec/combat-system.md) Lines 352-456
- **Feature Matrix:** [combat-system-feature-matrix.md](../spec/combat-system-feature-matrix.md) Lines 122-164
- **Related ADRs:**
  - [ADR-003: Reaction Queue System](003-reaction-queue-system.md) - Similar timer UI pattern
  - [ADR-008: Combat HUD](008-combat-hud.md) - Action bar integration point
  - [ADR-009: MVP Ability Set](009-mvp-ability-set.md) - Abilities receiving recovery timers

---

**Document Version:** 1.0
**Created:** 2025-11-07
**Author:** ARCHITECT
