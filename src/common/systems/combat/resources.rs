use bevy::prelude::*;
use crate::common::{
    components::{ActorAttributes, Loc, position::Position, resources::*, entity_type::EntityType},
    message::{Component as MessageComponent, Event, *},
};

/// Calculate maximum stamina from actor attributes
/// Formula (scaled for u16 values): 100 + (might * 0.2) + (vitality * 0.06)
///
/// Examples (level 50 specialist = 500 reach):
/// - might=250: 100 + 50 = 150 stamina
/// - might=500: 100 + 100 = 200 stamina
/// - might=500, vitality=500: 100 + 100 + 30 = 230 stamina
pub fn calculate_max_stamina(attrs: &ActorAttributes) -> f32 {
    let might = attrs.might() as f32;
    let vitality = attrs.vitality() as f32;
    100.0 + (might * 0.2) + (vitality * 0.06)
}

/// Calculate maximum mana from actor attributes
/// Formula (scaled for u16 values): 100 + (focus * 0.1) + (presence * 0.06)
///
/// Examples (level 50 specialist = 500 reach):
/// - focus=250: 100 + 25 = 125 mana
/// - focus=500: 100 + 50 = 150 mana
/// - focus=500, presence=500: 100 + 50 + 30 = 180 mana
pub fn calculate_max_mana(attrs: &ActorAttributes) -> f32 {
    let focus = attrs.focus() as f32;
    let presence = attrs.presence() as f32;
    100.0 + (focus * 0.1) + (presence * 0.06)
}

/// Calculate stamina regeneration rate
/// Base: 10/sec (may scale with attributes in future)
pub fn calculate_stamina_regen_rate(_attrs: &ActorAttributes) -> f32 {
    10.0
}

/// Calculate mana regeneration rate
/// Base: 8/sec (may scale with attributes in future)
pub fn calculate_mana_regen_rate(_attrs: &ActorAttributes) -> f32 {
    8.0
}

/// Calculate health regeneration rate
/// Returns 5 HP/sec when out of combat, 0 HP/sec when in combat
pub fn calculate_health_regen_rate(in_combat: bool) -> f32 {
    if in_combat {
        0.0
    } else {
        5.0
    }
}

/// Calculate armor (physical damage reduction) from actor attributes
/// Formula (scaled for u16): base_armor + (vitality / 330.0)
/// Capped at 75% max
///
/// Examples (level 50 specialist = 500 reach):
/// - vitality=165: +0.5 armor (50% reduction)
/// - vitality=330: +1.0 armor (capped at 75%)
/// - vitality=500: +1.51 armor (capped at 75%)
pub fn calculate_armor(attrs: &ActorAttributes, base_armor: f32) -> f32 {
    let vitality = attrs.vitality() as f32;
    let armor = base_armor + (vitality / 330.0);
    armor.min(0.75)
}

/// Calculate resistance (magic damage reduction) from actor attributes
/// Formula (scaled for u16): base_resistance + (focus / 330.0)
/// Capped at 75% max
///
/// Examples (level 50 specialist = 500 reach):
/// - focus=165: +0.5 resistance (50% reduction)
/// - focus=330: +1.0 resistance (capped at 75%)
/// - focus=500: +1.51 resistance (capped at 75%)
pub fn calculate_resistance(attrs: &ActorAttributes, base_resistance: f32) -> f32 {
    let focus = attrs.focus() as f32;
    let resistance = base_resistance + (focus / 330.0);
    resistance.min(0.75)
}

/// Regenerate stamina, mana, and health for all entities with resources
/// Runs in FixedUpdate schedule (125ms ticks)
/// Health regenerates at:
/// - 100 HP/sec when Returning (leashing NPCs)
/// - 5 HP/sec when out of combat (normal regen)
/// - 0 HP/sec when in combat
pub fn regenerate_resources(
    mut query: Query<(&mut Health, &mut Stamina, &mut Mana, &CombatState, Option<&crate::common::components::returning::Returning>)>,
    time: Res<Time>,
) {
    let current_time = time.elapsed();
    // Cap dt to 1 second max to prevent instant regen from stale last_update values
    // (e.g., after network updates where last_update gets reset to Duration::ZERO)
    const MAX_DT_SECS: f32 = 1.0;

    for (mut health, mut stamina, mut mana, combat_state, returning_opt) in &mut query {
        // Calculate time delta for this tick
        let dt_stamina = current_time.saturating_sub(stamina.last_update).as_secs_f32().min(MAX_DT_SECS);
        let dt_mana = current_time.saturating_sub(mana.last_update).as_secs_f32().min(MAX_DT_SECS);

        // Regenerate stamina
        stamina.state = (stamina.state + stamina.regen_rate * dt_stamina).min(stamina.max);
        stamina.step = stamina.state; // Sync step with state for remote entities
        stamina.last_update = current_time;

        // Regenerate mana
        mana.state = (mana.state + mana.regen_rate * dt_mana).min(mana.max);
        mana.step = mana.state;
        mana.last_update = current_time;

        // Regenerate health
        // Priority: Returning (100 HP/s) > Out of combat (5 HP/s) > In combat (0 HP/s)
        let health_regen_rate = if returning_opt.is_some() {
            100.0  // Leashing NPC - rapid reset
        } else if !combat_state.in_combat {
            5.0    // Out of combat - normal regen
        } else {
            0.0    // In combat - no regen
        };

        if health_regen_rate > 0.0 {
            health.state = (health.state + health_regen_rate * dt_stamina).min(health.max);
            health.step = health.state;
        }
    }
}

/// Check for entities with health <= 0 and handle death immediately
/// Runs on server only, after damage application systems
/// For NPCs: emits Despawn event directly (no 1-frame delay)
/// For players: adds RespawnTimer and emits Despawn
pub fn check_death(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Entity, Option<&crate::common::components::behaviour::Behaviour>, &mut Health, &mut Stamina, &mut Mana), Without<RespawnTimer>>,
) {
    for (ent, behaviour, mut health, mut stamina, mut mana) in &mut query {
        if health.state <= 0.0 {
            // Set resources to 0 to prevent "zombie" state
            health.state = 0.0;
            health.step = 0.0;
            stamina.state = 0.0;
            stamina.step = 0.0;
            mana.state = 0.0;
            mana.step = 0.0;

            // Check if this is a player (Behaviour::Controlled)
            let is_player = behaviour
                .map(|b| matches!(b, crate::common::components::behaviour::Behaviour::Controlled))
                .unwrap_or(false);

            if is_player {
                // Player death: add respawn timer (5 seconds) and despawn from client view
                commands.entity(ent).insert(RespawnTimer::new(time.elapsed()));
            }

            // Emit Despawn event immediately (for both players and NPCs)
            // This avoids the 1-frame delay from using trigger_targets
            writer.write(Do {
                event: Event::Despawn { ent },
            });
        }
    }
}

/// Process respawn timers and respawn players at origin after 5 seconds
/// Runs on server only
pub fn process_respawn(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Entity, &RespawnTimer, &mut Health, &mut Stamina, &mut Mana, &mut Loc, &mut Position, &ActorAttributes, &EntityType, Option<&crate::common::components::behaviour::PlayerControlled>)>,
) {
    use qrz::Qrz;

    for (ent, timer, mut health, mut stamina, mut mana, mut loc, mut position, attrs, entity_type, player_controlled) in &mut query {
        if timer.should_respawn(time.elapsed()) {
            // Teleport to origin
            let spawn_qrz = Qrz { q: 0, r: 0, z: 4 };
            *loc = Loc::new(spawn_qrz);

            // Reset position to snap to new location
            position.tile = spawn_qrz;
            position.offset = Vec3::ZERO;

            // Restore resources to full
            health.state = health.max;
            health.step = health.max;
            stamina.state = stamina.max;
            stamina.step = stamina.max;
            mana.state = mana.max;
            mana.step = mana.max;

            // Remove respawn timer
            commands.entity(ent).remove::<RespawnTimer>();

            // Re-spawn the player on client (was despawned on death)
            // Send Spawn event to re-create client entity with original actor type
            writer.write(Do {
                event: Event::Spawn {
                    ent,
                    typ: *entity_type,  // Use actual entity type (preserves Triumvirate, etc.)
                    qrz: spawn_qrz,
                    attrs: Some(*attrs),
                },
            });

            // Broadcast resource updates (sent after Spawn so client entity exists)
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Health(*health),
                },
            });
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Stamina(*stamina),
                },
            });
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Mana(*mana),
                },
            });

            // Broadcast PlayerControlled if this entity is player-controlled (so other clients recognize as ally)
            if let Some(pc) = player_controlled {
                writer.write(Do {
                    event: Event::Incremental {
                        ent,
                        component: MessageComponent::PlayerControlled(*pc),
                    },
                });
            }
        }
    }
}

/// DEPRECATED: Death handling is now done directly in check_death to avoid 1-frame delay
/// This observer is no longer registered or used
#[allow(dead_code)]
fn handle_death(
    trigger: On<Try>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Option<&crate::common::components::behaviour::Behaviour>, &mut Health, &mut Stamina, &mut Mana)>,
) {
    let event = &trigger.event().event;
    if let Event::Death { ent } = event {
        // Check if this is a player or NPC and set resources to 0
        let is_player = if let Ok((behaviour, mut health, mut stamina, mut mana)) = query.get_mut(*ent) {
            // Set resources to 0 to prevent "zombie" state
            health.state = 0.0;
            health.step = 0.0;
            stamina.state = 0.0;
            stamina.step = 0.0;
            mana.state = 0.0;
            mana.step = 0.0;

            behaviour
                .map(|b| matches!(b, crate::common::components::behaviour::Behaviour::Controlled))
                .unwrap_or(false)
        } else {
            false
        };

        if is_player {
            // Player death: add respawn timer (5 seconds) and despawn from client view
            commands.entity(*ent).insert(RespawnTimer::new(time.elapsed()));

            // Send Despawn to client so player disappears visually
            writer.write(Do {
                event: Event::Despawn { ent: *ent },
            });
        } else {
            // NPC death: emit Despawn event (actual despawn happens in PostUpdate)
            writer.write(Do {
                event: Event::Despawn { ent: *ent },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes with simple values
    // Axes: might/grace (negative/positive), vitality/focus (negative/positive)
    fn test_attrs_simple(
        might_grace_axis: i8,     // Negative for might, positive for grace
        vitality_focus_axis: i8,  // Negative for vitality, positive for focus
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace_axis, 0, 0,      // might_grace: axis, spectrum, shift
            vitality_focus_axis, 0, 0,   // vitality_focus: axis, spectrum, shift
            0, 0, 0,                      // instinct_presence: axis, spectrum, shift
        )
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants (ADR-015)

    /// INV-007: Armor 75% Cap
    /// Armor (physical damage reduction) MUST cap at 75% to prevent invulnerability.
    /// Even with extreme vitality stacking, minimum 25% damage always goes through.
    #[test]
    fn test_armor_caps_at_75_percent() {
        // Use axis=-100 to get vitality=100 (on vitality side)
        let extreme_vitality = test_attrs_simple(
            0,      // might_grace_axis
            -100,   // vitality=100 (vitality_focus_axis=-100)
        );
        let base_armor = 0.0;

        let armor = calculate_armor(&extreme_vitality, base_armor);

        // Formula: armor = base + (vitality/66) = 0 + (100/66) = 1.51
        // Should cap at 0.75 despite exceeding cap
        assert_eq!(armor, 0.75, "Armor did not cap at 75%");

        // Verify minimum 25% damage always goes through
        // If incoming damage is 100, final should be 25 after 75% mitigation
        let incoming_damage = 100.0;
        let final_damage = incoming_damage * (1.0 - armor);
        assert_eq!(final_damage, 25.0, "Damage reduction exceeded 75% cap");
    }

    /// INV-007: Resistance 75% Cap
    /// Resistance (magic damage reduction) MUST cap at 75% to prevent invulnerability.
    /// Even with extreme focus stacking, minimum 25% damage always goes through.
    #[test]
    fn test_resistance_caps_at_75_percent() {
        // Use axis=100 to get focus=100 (on focus side)
        let extreme_focus = test_attrs_simple(
            0,      // might_grace_axis
            100,    // focus=100 (vitality_focus_axis=100)
        );
        let base_resistance = 0.0;

        let resistance = calculate_resistance(&extreme_focus, base_resistance);

        // Formula: resistance = base + (focus/66) = 0 + (100/66) = 1.51
        // Should cap at 0.75 despite exceeding cap
        assert_eq!(resistance, 0.75, "Resistance did not cap at 75%");

        // Verify minimum 25% damage always goes through
        let incoming_damage = 100.0;
        let final_damage = incoming_damage * (1.0 - resistance);
        assert_eq!(final_damage, 25.0, "Magic damage reduction exceeded 75% cap");
    }

    /// INV-007: Armor Below Cap
    /// Verify armor calculation works correctly when below the cap.
    #[test]
    fn test_armor_below_cap() {
        // Use axis=-33 to get vitality=33
        let moderate_vitality = test_attrs_simple(
            0,      // might_grace_axis
            -33,    // vitality=33 (vitality_focus_axis=-33)
        );
        let base_armor = 0.0;

        let armor = calculate_armor(&moderate_vitality, base_armor);

        // Formula: 0 + (33/66) = 0.5 (50%)
        assert!((armor - 0.5).abs() < 0.01, "Armor calculation incorrect: expected ~0.5, got {}", armor);
    }

    /// INV-007: Resistance Below Cap
    /// Verify resistance calculation works correctly when below the cap.
    #[test]
    fn test_resistance_below_cap() {
        // Use axis=33 to get focus=33
        let moderate_focus = test_attrs_simple(
            0,      // might_grace_axis
            33,     // focus=33 (vitality_focus_axis=33)
        );
        let base_resistance = 0.0;

        let resistance = calculate_resistance(&moderate_focus, base_resistance);

        // Formula: 0 + (33/66) = 0.5 (50%)
        assert!((resistance - 0.5).abs() < 0.01, "Resistance calculation incorrect: expected ~0.5, got {}", resistance);
    }

    /// INV-008: Resource Regeneration During Combat
    /// Stamina and mana MUST regenerate during combat.
    /// Health MUST NOT regenerate during combat (except when Returning).
    #[test]
    fn test_stamina_regenerates_in_combat() {
        let regen_rate = calculate_stamina_regen_rate(&test_attrs_simple(0, 0));
        assert!(regen_rate > 0.0, "Stamina should regenerate in combat");
    }

    #[test]
    fn test_mana_regenerates_in_combat() {
        let regen_rate = calculate_mana_regen_rate(&test_attrs_simple(0, 0));
        assert!(regen_rate > 0.0, "Mana should regenerate in combat");
    }

    #[test]
    fn test_health_does_not_regenerate_in_combat() {
        let in_combat = true;
        let regen_rate = calculate_health_regen_rate(in_combat);
        assert_eq!(regen_rate, 0.0, "Health must NOT regenerate in combat");
    }

    #[test]
    fn test_health_regenerates_out_of_combat() {
        let in_combat = false;
        let regen_rate = calculate_health_regen_rate(in_combat);
        assert!(regen_rate > 0.0, "Health should regenerate out of combat");
    }

    // ===== SYSTEM TESTS =====

    #[test]
    fn test_check_death_emits_event_when_health_zero() {
        use std::sync::{Arc, Mutex};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_message::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<Entity>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: MessageReader<Do>| {
            for event in reader.read() {
                if let Event::Despawn { ent } = event.event {
                    emitted_events_clone.lock().unwrap().push(ent);
                }
            }
        });

        // Create entity with 0 health (e.g., from fall damage, not combat)
        let entity = app.world_mut().spawn((
            Health {
                max: 100.0,
                state: 0.0,
                step: 0.0,
            },
            Stamina {
                max: 100.0,
                state: 0.0,
                step: 0.0,
                regen_rate: 10.0,
                last_update: std::time::Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 0.0,
                step: 0.0,
                regen_rate: 8.0,
                last_update: std::time::Duration::ZERO,
            },
        )).id();

        // Run check_death system
        app.add_systems(Update, check_death);
        app.update();

        // Verify Despawn event was emitted
        let events = emitted_events.lock().unwrap();
        assert_eq!(events.len(), 1, "Expected one Despawn event");
        assert_eq!(events[0], entity, "Despawn event should be for the correct entity");
    }

    #[test]
    fn test_check_death_ignores_entities_with_respawn_timer() {
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_message::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<()>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: MessageReader<Do>| {
            for event in reader.read() {
                if let Event::Despawn { ent: _ } = event.event {
                    emitted_events_clone.lock().unwrap().push(());
                }
            }
        });

        // Create entity with 0 health AND RespawnTimer (already dead)
        app.world_mut().spawn((
            Health {
                max: 100.0,
                state: 0.0,
                step: 0.0,
            },
            Stamina {
                max: 100.0,
                state: 0.0,
                step: 0.0,
                regen_rate: 10.0,
                last_update: Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 0.0,
                step: 0.0,
                regen_rate: 8.0,
                last_update: Duration::ZERO,
            },
            RespawnTimer::new(Duration::from_secs(0)),
        ));

        // Run check_death system
        app.add_systems(Update, check_death);
        app.update();

        // Verify NO Despawn event was emitted (entity already has respawn timer)
        let events = emitted_events.lock().unwrap();
        assert_eq!(events.len(), 0, "Should not emit Despawn event for entities with RespawnTimer");
    }

    #[test]
    fn test_check_death_ignores_alive_entities() {
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_message::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<()>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: MessageReader<Do>| {
            for event in reader.read() {
                if let Event::Despawn { ent: _ } = event.event {
                    emitted_events_clone.lock().unwrap().push(());
                }
            }
        });

        // Create entity with positive health
        app.world_mut().spawn((
            Health {
                max: 100.0,
                state: 50.0,
                step: 50.0,
            },
            Stamina {
                max: 100.0,
                state: 50.0,
                step: 50.0,
                regen_rate: 10.0,
                last_update: Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 50.0,
                step: 50.0,
                regen_rate: 8.0,
                last_update: Duration::ZERO,
            },
        ));

        // Run check_death system
        app.add_systems(Update, check_death);
        app.update();

        // Verify NO Despawn event was emitted
        let events = emitted_events.lock().unwrap();
        assert_eq!(events.len(), 0, "Should not emit Despawn event for alive entities");
    }

    #[test]
    fn test_health_regen_out_of_combat() {
        let rate = calculate_health_regen_rate(false);
        assert_eq!(rate, 5.0, "Health should regenerate at 5 HP/sec when out of combat");
    }

    #[test]
    fn test_health_regen_in_combat() {
        let rate = calculate_health_regen_rate(true);
        assert_eq!(rate, 0.0, "Health should not regenerate when in combat");
    }
}
