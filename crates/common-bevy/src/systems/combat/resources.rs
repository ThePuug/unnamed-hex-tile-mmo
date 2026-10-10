use bevy::prelude::*;
use crate::{
    components::{ActorAttributes, Loc, position::Position, resources::*},
    message::{Component as MessageComponent, Event, *},
};
use crate::tuning::Tuning;

/// What every actor is spawned fighting with, all of it from its
/// attributes: its pools full, out of combat, an empty reaction queue,
/// no target, and loaded by no one. A player and
/// an NPC are both spawned with it, so what one starts with the other does.
#[derive(Bundle)]
pub struct Fighter {
    pub attrs: ActorAttributes,
    pub health: Health,
    pub endurance: Endurance,
    pub mana: Mana,
    pub combat_state: CombatState,
    pub queue: crate::components::reaction_queue::ReactionQueue,
    pub target: crate::components::target::Target,
    pub loaded_by: crate::components::loaded_by::LoadedBy,
}

impl Fighter {
    /// An actor with `attrs`, spawned at `now`
    pub fn new(tuning: &Tuning, attrs: ActorAttributes, now: std::time::Duration) -> Self {
        Self {
            attrs,
            health: Health::full(attrs.max_health(tuning)),
            endurance: Endurance::full(attrs.max_endurance(tuning)),
            mana: Mana::full(now),
            combat_state: CombatState { in_combat: false, last_action: now },
            queue: default(),
            target: default(),
            loaded_by: default(),
        }
    }
}

/// Health an NPC returning to its spawn regains each second: a leash
/// reset, not a regen (`Returning`)
pub const RETURNING_HEALTH_REGEN: f32 = 100.0;

/// Health an actor out of combat regains each second
pub const RESTING_HEALTH_REGEN: f32 = 5.0;

/// Health an actor in combat regains each second: none
pub const COMBAT_HEALTH_REGEN: f32 = 0.0;

/// Regenerate mana, endurance and health for all entities with resources
/// Runs in FixedUpdate schedule (125ms ticks)
/// Endurance regenerates steadily, in combat or out.
/// Health regenerates at `RETURNING_HEALTH_REGEN` a second while
/// Returning, else `RESTING_HEALTH_REGEN` out of combat and
/// `COMBAT_HEALTH_REGEN` in it.
pub fn regenerate_resources(
    tuning: Res<Tuning>,
    mut query: Query<(&mut Health, &mut Mana, Option<&mut Endurance>, &CombatState, Option<&crate::components::returning::Returning>)>,
    time: Res<Time>,
) {
    let current_time = time.elapsed();
    let dt = time.delta_secs();
    // `Mana::last_update` is not on the wire, so a pool that just arrived
    // reads ZERO: cap the gap or it would refill at once.
    const MAX_DT_SECS: f32 = 1.0;

    for (mut health, mut mana, endurance, combat_state, returning_opt) in &mut query {
        // The dead regenerate nothing, in combat or out
        if health.state <= 0.0 {
            continue;
        }

        // Calculate time delta for this tick
        let dt_mana = current_time.saturating_sub(mana.last_update).as_secs_f32().min(MAX_DT_SECS);

        if let Some(mut endurance) = endurance.filter(|endurance| endurance.state < endurance.max) {
            let regained = tuning.endurance_regen * endurance.max * dt;
            endurance.state = (endurance.state + regained).min(endurance.max);
        }

        // Regenerate mana
        mana.state = (mana.state + mana.regen_rate * dt_mana).min(mana.max);
        mana.last_update = current_time;

        let health_regen_rate = if returning_opt.is_some() {
            RETURNING_HEALTH_REGEN
        } else if !combat_state.in_combat {
            RESTING_HEALTH_REGEN
        } else {
            COMBAT_HEALTH_REGEN
        };

        if health_regen_rate > 0.0 {
            health.state = (health.state + health_regen_rate * dt).min(health.max);
        }
    }
}

/// Check for entities with health <= 0 and handle death immediately
/// Runs on server only, after damage application systems
/// For NPCs: emits Despawn
/// For players: adds RespawnTimer and emits Despawn
pub fn check_death(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Entity, Has<crate::components::behaviour::PlayerControlled>, &mut Health, &mut Mana), Without<RespawnTimer>>,
) {
    for (ent, is_player, mut health, mut mana) in &mut query {
        if health.state <= 0.0 {
            // Set resources to 0 to prevent "zombie" state
            health.state = 0.0;
            mana.state = 0.0;
            // Death ends every status effect: a body stands again with none
            commands.entity(ent).remove::<crate::components::status::Status>();

            if is_player {
                // Player death: add respawn timer (5 seconds) and despawn from client view
                commands.entity(ent).insert(RespawnTimer::new(time.elapsed()));
            }

            // Despawned at once, a player and an NPC alike
            writer.write(Do {
                event: Event::Despawn { ent },
            });
        }
    }
}

/// Stands every player whose respawn timer has run out at the `SpawnPoint`,
/// its pools full
/// Runs on server only
pub fn process_respawn(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    spawn_point: Res<SpawnPoint>,
    mut query: Query<(Entity, &RespawnTimer, &mut Health, &mut Mana, Option<&mut Endurance>, &mut Loc, &mut Position, Option<&crate::components::behaviour::PlayerControlled>)>,
) {
    for (ent, timer, mut health, mut mana, endurance, mut loc, mut position, player_controlled) in &mut query {
        if timer.should_respawn(time.elapsed()) {
            let spawn_qrz = spawn_point.0;
            *loc = Loc::new(spawn_qrz);

            // Reset position to snap to new location
            position.tile = spawn_qrz;
            position.offset = Vec3::ZERO;

            // Restore resources to full
            health.state = health.max;
            mana.state = mana.max;

            // Remove respawn timer
            commands.entity(ent).remove::<RespawnTimer>();

            // Clients stand it again at the spawn point, before its pools
            // say it is alive
            writer.write(Do { event: Event::Respawn { ent, qrz: spawn_qrz } });

            // Broadcast resource updates (sent after Respawn so the client
            // has placed it)
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Health(*health),
                },
            });
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Mana(*mana),
                },
            });
            if let Some(mut endurance) = endurance {
                endurance.state = endurance.max;
                writer.write(Do { event: Event::Incremental { ent, component: MessageComponent::Endurance(*endurance) } });
            }

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
    // Axes: might/agility (negative/positive), physique/discipline (negative/positive)
    fn test_attrs_simple(
        might_agility_axis: i8,     // Negative for might, positive for agility
        physique_discipline_axis: i8,  // Negative for physique, positive for discipline
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_agility_axis, 0, 0,      // might_agility: axis, spectrum, shift
            physique_discipline_axis, 0, 0,   // physique_discipline: axis, spectrum, shift
            0, 0, 0,                      // instinct_resolve: axis, spectrum, shift
        )
    }

    // ===== INVARIANT TESTS =====
    // These tests verify critical architectural invariants

    #[test]
    fn the_dead_regenerate_nothing() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(1));
        world.insert_resource(time);
        world.init_resource::<Tuning>();
        let body = world.spawn((
            Health { state: 0.0, max: 100.0 },
            Mana { state: 0.0, max: 100.0, regen_rate: 10.0, last_update: std::time::Duration::ZERO },
            CombatState { in_combat: false, last_action: std::time::Duration::ZERO },
        )).id();

        world.run_system_once(regenerate_resources).unwrap();

        assert_eq!(world.get::<Health>(body).unwrap().state, 0.0, "out of combat, still dead");
        assert_eq!(world.get::<Mana>(body).unwrap().state, 0.0);
    }

    #[test]
    fn endurance_comes_back_in_combat_as_out() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs(1));
        world.insert_resource(time);
        world.init_resource::<Tuning>();
        let pools = |in_combat: bool| (
            Health { state: 100.0, max: 100.0 },
            Mana { state: 100.0, max: 100.0, regen_rate: 0.0, last_update: std::time::Duration::ZERO },
            Endurance { state: 10.0, max: 100.0 },
            CombatState { in_combat, last_action: std::time::Duration::ZERO },
        );
        let fighting = world.spawn(pools(true)).id();
        let resting = world.spawn(pools(false)).id();

        world.run_system_once(regenerate_resources).unwrap();

        assert!(world.get::<Endurance>(fighting).unwrap().state > 10.0, "in combat, it comes back");
        assert_eq!(world.get::<Endurance>(fighting).unwrap().state, world.get::<Endurance>(resting).unwrap().state, "as fast as out of it");
    }

    #[test]
    fn an_actor_is_spawned_with_its_pools_full() {
        let tuning = Tuning::DEFAULT;
        let now = std::time::Duration::from_secs(3);
        let attrs = test_attrs_simple(0, -5);
        let fighter = Fighter::new(&tuning, attrs, now);
        assert_eq!((fighter.health.state, fighter.health.max), (attrs.max_health(&tuning), attrs.max_health(&tuning)));
        assert_eq!(fighter.mana.state, fighter.mana.max);
        assert_eq!((fighter.endurance.state, fighter.endurance.max), (attrs.max_endurance(&tuning), attrs.max_endurance(&tuning)));
        assert!(fighter.mana.regen_rate > 0.0, "it regenerates, in combat or out");
        assert_eq!(fighter.mana.last_update, now);
        assert!(!fighter.combat_state.in_combat);
        assert!(fighter.queue.is_empty());
    }

    // ===== SYSTEM TESTS =====

    /// A body at no health is despawned once: a respawn timer already
    /// running marks a death already handled, and a living body is left
    /// alone.
    #[test]
    fn the_dead_are_despawned_once() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<Do>>();
        let mana = Mana { state: 0.0, max: 100.0, regen_rate: 8.0, last_update: std::time::Duration::ZERO };
        let dead = world.spawn((Health { state: 0.0, max: 100.0 }, mana)).id();
        world.spawn((Health { state: 0.0, max: 100.0 }, mana, RespawnTimer::new(std::time::Duration::ZERO)));
        world.spawn((Health { state: 50.0, max: 100.0 }, mana));

        world.run_system_once(check_death).unwrap();

        let despawned: Vec<Entity> = world
            .resource::<Messages<Do>>()
            .iter_current_update_messages()
            .filter_map(|message| match message.event {
                Event::Despawn { ent } => Some(ent),
                _ => None,
            })
            .collect();
        assert_eq!(despawned, vec![dead]);
    }
}
