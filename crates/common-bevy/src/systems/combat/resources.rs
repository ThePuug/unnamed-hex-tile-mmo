use bevy::prelude::*;
use crate::{
    components::{ActorAttributes, Loc, position::Position, resources::*, entity_type::EntityType},
    message::{Component as MessageComponent, Event, *},
};

/// How far a Lunge reaches, in tiles: it lands beside anything from
/// adjacent out to this.
pub const LUNGE_RANGE: u32 = 8;

/// What every actor is spawned fighting with, all of it from its
/// attributes: its pools full, out of combat, a reaction queue with the
/// window its Awareness sees, no target, and loaded by no one. A player and
/// an NPC are both spawned with it, so what one starts with the other does.
#[derive(Bundle)]
pub struct Fighter {
    pub attrs: ActorAttributes,
    pub health: Health,
    pub stamina: Stamina,
    pub mana: Mana,
    pub combat_state: CombatState,
    pub queue: crate::components::reaction_queue::ReactionQueue,
    pub target: crate::components::target::Target,
    pub loaded_by: crate::components::loaded_by::LoadedBy,
}

impl Fighter {
    /// An actor with `attrs`, spawned at `now`
    pub fn new(attrs: ActorAttributes, now: std::time::Duration) -> Self {
        Self {
            attrs,
            health: Health::full(attrs.max_health()),
            stamina: Stamina::full(attrs.max_stamina(), now),
            mana: Mana::full(now),
            combat_state: CombatState { in_combat: false, last_action: now },
            queue: crate::components::reaction_queue::ReactionQueue::new(attrs.window_size()),
            target: default(),
            loaded_by: default(),
        }
    }
}

/// Regenerate stamina, mana, and health for all entities with resources
/// Runs in FixedUpdate schedule (125ms ticks)
/// Health regenerates at:
/// - 100 HP/sec when Returning (leashing NPCs)
/// - 5 HP/sec when out of combat (normal regen)
/// - 0 HP/sec when in combat
pub fn regenerate_resources(
    mut query: Query<(&mut Health, &mut Stamina, &mut Mana, &CombatState, Option<&crate::components::returning::Returning>)>,
    time: Res<Time>,
) {
    let current_time = time.elapsed();
    // Cap dt to 1 second max to prevent instant regen from stale last_update values
    // (e.g., after network updates where last_update gets reset to Duration::ZERO)
    const MAX_DT_SECS: f32 = 1.0;

    for (mut health, mut stamina, mut mana, combat_state, returning_opt) in &mut query {
        // The dead regenerate nothing, in combat or out
        if health.state <= 0.0 {
            continue;
        }

        // Calculate time delta for this tick
        let dt_stamina = current_time.saturating_sub(stamina.last_update).as_secs_f32().min(MAX_DT_SECS);
        let dt_mana = current_time.saturating_sub(mana.last_update).as_secs_f32().min(MAX_DT_SECS);

        // Regenerate stamina
        stamina.state = (stamina.state + stamina.regen_rate * dt_stamina).min(stamina.max);
        stamina.last_update = current_time;

        // Regenerate mana
        mana.state = (mana.state + mana.regen_rate * dt_mana).min(mana.max);
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
    mut query: Query<(Entity, Has<crate::components::behaviour::PlayerControlled>, &mut Health, &mut Stamina, &mut Mana), Without<RespawnTimer>>,
) {
    for (ent, is_player, mut health, mut stamina, mut mana) in &mut query {
        if health.state <= 0.0 {
            // Set resources to 0 to prevent "zombie" state
            health.state = 0.0;
            stamina.state = 0.0;
            mana.state = 0.0;

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
    spawn_point: Res<SpawnPoint>,
    mut query: Query<(Entity, &RespawnTimer, &mut Health, &mut Stamina, &mut Mana, &mut Loc, &mut Position, &ActorAttributes, &EntityType, Option<&crate::components::behaviour::PlayerControlled>)>,
) {
    for (ent, timer, mut health, mut stamina, mut mana, mut loc, mut position, attrs, entity_type, player_controlled) in &mut query {
        if timer.should_respawn(time.elapsed()) {
            let spawn_qrz = spawn_point.0;
            *loc = Loc::new(spawn_qrz);

            // Reset position to snap to new location
            position.tile = spawn_qrz;
            position.offset = Vec3::ZERO;

            // Restore resources to full
            health.state = health.max;
            stamina.state = stamina.max;
            mana.state = mana.max;

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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes with simple values
    // Axes: might/agility (negative/positive), vitality/discipline (negative/positive)
    fn test_attrs_simple(
        might_agility_axis: i8,     // Negative for might, positive for agility
        vitality_discipline_axis: i8,  // Negative for vitality, positive for discipline
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_agility_axis, 0, 0,      // might_agility: axis, spectrum, shift
            vitality_discipline_axis, 0, 0,   // vitality_discipline: axis, spectrum, shift
            0, 0, 0,                      // instinct_resolve: axis, spectrum, shift
        )
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants

    /// Resource Regeneration During Combat
    /// Stamina and mana MUST regenerate during combat.
    #[test]
    fn the_dead_regenerate_nothing() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(1));
        world.insert_resource(time);
        let body = world.spawn((
            Health { state: 0.0, max: 100.0 },
            Stamina { state: 0.0, max: 100.0, regen_rate: 10.0, last_update: std::time::Duration::ZERO },
            Mana { state: 0.0, max: 100.0, regen_rate: 10.0, last_update: std::time::Duration::ZERO },
            CombatState { in_combat: false, last_action: std::time::Duration::ZERO },
        )).id();

        world.run_system_once(regenerate_resources).unwrap();

        assert_eq!(world.get::<Health>(body).unwrap().state, 0.0, "out of combat, still dead");
        assert_eq!(world.get::<Stamina>(body).unwrap().state, 0.0);
    }

    #[test]
    fn an_actor_is_spawned_with_its_pools_full_and_its_window_open() {
        let now = std::time::Duration::from_secs(3);
        let attrs = test_attrs_simple(0, -5);
        let fighter = Fighter::new(attrs, now);
        assert_eq!((fighter.health.state, fighter.health.max), (attrs.max_health(), attrs.max_health()));
        assert_eq!((fighter.stamina.state, fighter.stamina.max), (attrs.max_stamina(), attrs.max_stamina()));
        assert_eq!(fighter.mana.state, fighter.mana.max);
        assert!(fighter.stamina.regen_rate > 0.0 && fighter.mana.regen_rate > 0.0, "both regenerate, in combat or out");
        assert_eq!(fighter.stamina.last_update, now);
        assert!(!fighter.combat_state.in_combat);
        assert_eq!(fighter.queue.window_size, attrs.window_size());
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
            },
            Stamina {
                max: 100.0,
                state: 0.0,
                regen_rate: 10.0,
                last_update: std::time::Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 0.0,
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
            },
            Stamina {
                max: 100.0,
                state: 0.0,
                regen_rate: 10.0,
                last_update: Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 0.0,
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
            },
            Stamina {
                max: 100.0,
                state: 50.0,
                regen_rate: 10.0,
                last_update: Duration::ZERO,
            },
            Mana {
                max: 100.0,
                state: 50.0,
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

}
