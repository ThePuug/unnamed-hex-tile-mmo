use bevy::prelude::*;
use crate::common::{
    components::{ActorAttributes, Loc, offset::Offset, resources::*, entity_type::EntityType},
    message::{Component as MessageComponent, Event, *},
};

/// Calculate maximum stamina from actor attributes
/// Formula: 100 + (might * 1.0) + (vitality * 0.3)
/// 50 might = 150 stamina, 100 might = 200 stamina, 150 might = 250 stamina
pub fn calculate_max_stamina(attrs: &ActorAttributes) -> f32 {
    let might = attrs.might() as f32;
    let vitality = attrs.vitality() as f32;
    100.0 + (might * 1.0) + (vitality * 0.3)
}

/// Calculate maximum mana from actor attributes
/// Formula: 100 + (focus * 0.5) + (presence * 0.3)
pub fn calculate_max_mana(attrs: &ActorAttributes) -> f32 {
    let focus = attrs.focus() as f32;
    let presence = attrs.presence() as f32;
    100.0 + (focus * 0.5) + (presence * 0.3)
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
/// Formula: base_armor + (vitality / 66.0)
/// Capped at 75% max
pub fn calculate_armor(attrs: &ActorAttributes, base_armor: f32) -> f32 {
    let vitality = attrs.vitality() as f32;
    let armor = base_armor + (vitality / 66.0);
    armor.min(0.75)
}

/// Calculate resistance (magic damage reduction) from actor attributes
/// Formula: base_resistance + (focus / 66.0)
/// Capped at 75% max
pub fn calculate_resistance(attrs: &ActorAttributes, base_resistance: f32) -> f32 {
    let focus = attrs.focus() as f32;
    let resistance = base_resistance + (focus / 66.0);
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
    mut writer: EventWriter<Do>,
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
    mut writer: EventWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Entity, &RespawnTimer, &mut Health, &mut Stamina, &mut Mana, &mut Loc, &mut Offset, &ActorAttributes, &EntityType, Option<&crate::common::components::behaviour::PlayerControlled>)>,
) {
    use qrz::Qrz;
    use bevy::math::Vec3;

    for (ent, timer, mut health, mut stamina, mut mana, mut loc, mut offset, attrs, entity_type, player_controlled) in &mut query {
        if timer.should_respawn(time.elapsed()) {
            // Teleport to origin
            let spawn_qrz = Qrz { q: 0, r: 0, z: 4 };
            *loc = Loc::new(spawn_qrz);

            // Clear offset to snap to new position (no interpolation)
            offset.state = Vec3::ZERO;
            offset.step = Vec3::ZERO;
            offset.prev_step = Vec3::ZERO;
            offset.interp_elapsed = 0.0;
            offset.interp_duration = 0.0;

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
    trigger: Trigger<Try>,
    mut commands: Commands,
    mut writer: EventWriter<Do>,
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

    // Helper to create test attributes
    fn test_attrs(
        might_grace: (i8, u8, i8),
        vitality_focus: (i8, u8, i8),
        instinct_presence: (i8, u8, i8),
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace.0, might_grace.1, might_grace.2,
            vitality_focus.0, vitality_focus.1, vitality_focus.2,
            instinct_presence.0, instinct_presence.1, instinct_presence.2,
        )
    }

    // Resource calculation formulas are expected to change during balancing.
    // No detailed formula tests - systems tests verify the pipeline works.

    #[test]
    fn test_check_death_emits_event_when_health_zero() {
        use std::sync::{Arc, Mutex};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_event::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<Entity>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: EventReader<Do>| {
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
        app.add_event::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<()>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: EventReader<Do>| {
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
        app.add_event::<Do>();

        // Track emitted events using a system
        let emitted_events: Arc<Mutex<Vec<()>>> = Arc::new(Mutex::new(Vec::new()));
        let emitted_events_clone = emitted_events.clone();

        app.add_systems(Update, move |mut reader: EventReader<Do>| {
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
