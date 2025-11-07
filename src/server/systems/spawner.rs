use bevy::{prelude::*, ecs::hierarchy::ChildOf};
use qrz::Qrz;
use rand::Rng;

use crate::{
    common::{
        components::{*, spawner::*, entity_type::*, behaviour::Behaviour, reaction_queue::*, resources::*, gcd::Gcd, heading::Heading, offset::Offset},
        message::*,
        plugins::nntree::*,
        resources::map::Map,
        systems::combat::{
            queue as queue_calcs,
            resources as resource_calcs,
        },
    },
    server::systems::behaviour::{
        chase::Chase,
        kite::Kite,
    },
};

/// System that ticks spawners and spawns NPCs when conditions are met
pub fn tick_spawners(
    mut commands: Commands,
    time: Res<Time>,
    mut spawners: Query<(Entity, &Loc, &mut Spawner)>,
    spawned: Query<&ChildOf>,
    players: Query<(&Loc, &Behaviour)>,
    mut writer: EventWriter<Do>,
    map: Res<Map>,
) {
    let elapsed = time.elapsed().as_millis();

    for (spawner_ent, &spawner_loc, mut spawner) in &mut spawners {
        // Check cooldown
        let time_since_last = elapsed - spawner.last_spawn_attempt;
        if time_since_last < spawner.respawn_timer_ms as u128 {
            continue;
        }

        // Check if any player is nearby (only Behaviour::Controlled entities are players)
        let player_distances: Vec<_> = players.iter()
            .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
            .map(|(player_loc, _)| spawner_loc.distance(player_loc))
            .collect();

        let has_nearby_player = player_distances.iter()
            .any(|&dist| dist <= spawner.player_activation_range as i16);

        if !has_nearby_player {
            continue;
        }

        // Count how many NPCs this spawner has alive
        let alive_count = spawned
            .iter()
            .filter(|child_of| child_of.parent() == spawner_ent)
            .count();

        if alive_count >= spawner.max_count as usize {
            continue;
        }

        // Spawn new NPC at random location within radius
        let spawn_qrz = random_hex_within_radius(*spawner_loc, spawner.spawn_radius);

        spawn_npc(
            &mut commands,
            spawner.npc_template,
            spawn_qrz,
            spawner.spawn_radius,
            spawner.leash_distance,
            spawner_ent,
            &mut writer,
            &map,
            &time,
        );

        // Update cooldown
        spawner.last_spawn_attempt = elapsed;
    }
}

/// Helper function to spawn an NPC from a template
fn spawn_npc(
    commands: &mut Commands,
    template: NpcTemplate,
    qrz: impl Into<Qrz>,
    spawn_radius: u8,
    leash_distance: u8,
    spawner_ent: Entity,
    writer: &mut EventWriter<Do>,
    map: &Map,
    time: &Time,
) {
    let qrz = qrz.into();
    // Search from higher Z to account for sloped terrain
    // On slopes, spawn location might be uphill from spawner
    let search_start = Qrz {
        q: qrz.q,
        r: qrz.r,
        z: qrz.z + spawn_radius as i16,
    };
    let Some((terrain_qrz, _entity_type)) = map.find(search_start, -60) else {
        warn!("Failed to find terrain for spawn location {:?}, skipping spawn", search_start);
        return;
    };
    // Spawn NPC one tile ABOVE terrain (not inside it)
    let spawn_qrz = Qrz {
        q: terrain_qrz.q,
        r: terrain_qrz.r,
        z: terrain_qrz.z + 1,
    };
    let loc = Loc::new(spawn_qrz);
    let actor_impl = template.actor_impl();
    let typ = EntityType::Actor(actor_impl);

    // ADR-010 Phase 2: Vary Grace (might_grace_axis) to create different movement speeds
    // Grace range: -40 to +40 (Might-focused to Grace-focused)
    // This creates speed variation: 80% to 120% of baseline
    // Formula: max(75, 100 + (grace / 2))
    //   Grace -40: speed = max(75, 100 - 20) = 80%
    //   Grace   0: speed = 100% (baseline)
    //   Grace +40: speed = 100 + 20 = 120%
    let mut rng = rand::rng();
    let grace_axis = rng.random_range(-40..=40); // Vary from Might-focused to Grace-focused

    // Level 10: Instinct focused distribution
    // axis varies (Grace variation), spectrum=2 (2 levels) = 5 levels might_grace
    // vitality_focus: no investment (0 levels)
    // instinct_presence: instinct-focused (5 levels)
    let attrs = ActorAttributes::new(
        grace_axis, 2, 0,  // might_grace: varied for movement speed
        0, 0, 0,           // vitality_focus: no investment (0 levels)
        -6, 2, 0,          // instinct_presence: instinct-focused (5 levels)
    );

    // Calculate initial resources from attributes
    let max_health = attrs.max_health();
    let max_stamina = resource_calcs::calculate_max_stamina(&attrs);
    let max_mana = resource_calcs::calculate_max_mana(&attrs);
    let stamina_regen = resource_calcs::calculate_stamina_regen_rate(&attrs);
    let mana_regen = resource_calcs::calculate_mana_regen_rate(&attrs);

    let health = Health {
        state: max_health,
        step: max_health,
        max: max_health,
    };
    let stamina = Stamina {
        state: max_stamina,
        step: max_stamina,
        max: max_stamina,
        regen_rate: stamina_regen,
        last_update: time.elapsed(),
    };
    let mana = Mana {
        state: max_mana,
        step: max_mana,
        max: max_mana,
        regen_rate: mana_regen,
        last_update: time.elapsed(),
    };
    let combat_state = CombatState {
        in_combat: false,
        last_action: time.elapsed(),
    };
    // Initialize reaction queue with capacity based on Focus attribute
    let queue_capacity = queue_calcs::calculate_queue_capacity(&attrs);
    let reaction_queue = ReactionQueue::new(queue_capacity);

    let ent = commands
        .spawn((
            typ,
            loc,
            Physics,
            ChildOf(spawner_ent),
            Name::new(format!("NPC {:?}", template)),
            attrs,
            health,
            stamina,
            mana,
            combat_state,
            reaction_queue,
            Gcd::new(),
            LastAutoAttack::default(),
        ))
        .id();

    // Add behavior component based on template (ADR-010 Phase 4)
    match template {
        NpcTemplate::ForestSprite => {
            // Ranged kiter - maintains distance, fires projectiles
            let kite = Kite::forest_sprite();
            commands.entity(ent).insert((
                NearestNeighbor::new(ent, loc),
                kite,
                Heading::default(),
                Offset::default(),
                AirTime::default(),
            ));
        }
        NpcTemplate::Dog | NpcTemplate::Wolf => {
            // Melee aggressor - charges and attacks at close range
            let chase = Chase {
                acquisition_range: 20,
                leash_distance: leash_distance as i16,
                attack_range: 1,
            };
            commands.entity(ent).insert((
                NearestNeighbor::new(ent, loc),
                chase,
                Heading::default(),
                Offset::default(),
                AirTime::default(),
            ));
        }
    }

    info!("Spawned NPC {:?} at {:?}", template, spawn_qrz);

    // Send spawn event to clients
    writer.write(Do {
        event: crate::common::message::Event::Spawn { ent, typ, qrz: spawn_qrz, attrs: Some(attrs) },
    });

    // Send initial resource states to clients via Incremental
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Health(health) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Stamina(stamina) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Mana(mana) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::CombatState(combat_state) }});
}

/// Helper function to generate a random hex within a radius
/// Only randomizes horizontal position (q, r), keeps center's Z for terrain lookup
fn random_hex_within_radius(center: impl Into<Qrz>, radius: u8) -> Qrz {
    if radius == 0 {
        return center.into();
    }

    let center = center.into();
    let mut rng = rand::rng();

    // Generate random HORIZONTAL offset within radius
    // Z coordinate is NOT randomized - it will be determined by terrain height
    let radius = radius as i16;

    loop {
        let q_offset = rng.random_range(-radius..=radius);
        let r_offset = rng.random_range(-radius..=radius);

        // Check if within radius using flat hex distance (ignore Z)
        // Flat distance = max(|q|, |r|, |q+r|)
        let dist = q_offset.abs().max(r_offset.abs()).max((q_offset + r_offset).abs());
        if dist <= radius {
            return Qrz {
                q: center.q + q_offset,
                r: center.r + r_offset,
                z: center.z,  // Keep center's Z - map.find will adjust to terrain
            };
        }
    }
}

/// System that despawns NPCs when all players are beyond the despawn distance
pub fn despawn_out_of_range(
    spawners: Query<(Entity, &Loc, &Spawner)>,
    npcs: Query<(Entity, &Loc, &ChildOf), Without<Spawner>>,
    players: Query<(&Loc, &Behaviour)>,
    mut writer: EventWriter<Do>,
) {
    for (spawner_ent, &spawner_loc, spawner) in &spawners {
        // Check if any player is within despawn distance of this spawner (only Behaviour::Controlled entities are players)
        let player_distances: Vec<_> = players.iter()
            .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
            .map(|(player_loc, _)| spawner_loc.distance(player_loc))
            .collect();

        let has_nearby_player = player_distances.iter()
            .any(|&dist| dist <= spawner.despawn_distance as i16);

        if has_nearby_player {
            continue;
        }

        // Count and despawn all NPCs from this spawner
        let npcs_to_despawn: Vec<_> = npcs.iter()
            .filter(|(_, _, child_of)| child_of.parent() == spawner_ent)
            .map(|(ent, _, _)| ent)
            .collect();

        if !npcs_to_despawn.is_empty() {
            for npc_ent in npcs_to_despawn {
                // Send despawn event - the actual despawning will happen in PostUpdate
                // after send_do has sent the network message
                writer.write(Do {
                    event: crate::common::message::Event::Despawn { ent: npc_ent },
                });
                // Don't despawn here - let the send_do system handle it after sending the message
            }
        }
    }
}
