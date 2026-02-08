use std::time::Duration;

use bevy::{prelude::*, scene::SceneInstanceReady};
use bevy_camera::primitives::Aabb;
use qrz::Convert;

use crate::{
    client::components::*,
    common::{
        components::{
            behaviour::Behaviour,
            entity_type::{ actor::*, * },
            heading::*, keybits::*, offset::*,
            reaction_queue::ReactionQueue,
            *
        },
        message::{ Event, * },
        plugins::nntree::NearestNeighbor,
        resources::{map::Map, InputQueues},
        systems::combat::queue as queue_calcs,
    }
};

pub fn setup() {}

fn ready(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    query: Query<&EntityType>,
    mut q_player: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    let entity = trigger.entity;
    for child in q_child.iter_descendants(entity) {
        if let Ok(mut player) = q_player.get_mut(child) {
            commands.entity(entity).insert(Animates(child));

            let &typ = query.get(entity).expect("couldn't get entity type");
            let asset = get_asset(typ);
            let (graph, _) = AnimationGraph::from_clips([
                asset_server.load(GltfAssetLabel::Animation(0).from_asset(asset.clone())),
                asset_server.load(GltfAssetLabel::Animation(1).from_asset(asset.clone())),
                asset_server.load(GltfAssetLabel::Animation(2).from_asset(asset.clone()))]);
            let handle = AnimationGraphHandle(graphs.add(graph));
            let mut transitions = AnimationTransitions::new();
            transitions.play(&mut player, 2.into(), Duration::ZERO).set_speed(1.).repeat();
            commands.entity(child)
                .insert(handle)
                .insert(transitions);
        }
    }
}

pub fn update(
    fixed_time: Res<Time<Fixed>>,
    time: Res<Time>,
    mut query: Query<(Entity, &Loc, &mut Offset, &Heading, &mut Transform)>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    let delta = time.delta_secs();

    for (ent, &loc, mut offset, &heading, mut transform0) in &mut query {
        let is_local = buffers.get(&ent).is_some();

        let interp_fraction = if is_local {
            // Local players: use FixedUpdate overstep fraction
            fixed_time.overstep_fraction()
        } else {
            // NPCs and remote players: use time-based interpolation
            offset.interp_elapsed += delta;
            if offset.interp_duration > 0.0 {
                (offset.interp_elapsed / offset.interp_duration).min(1.0)
            } else {
                1.0
            }
        };

        let prev_pos = map.convert(*loc) + offset.prev_step;
        let curr_pos = map.convert(*loc) + offset.step;

        // Interpolate between previous and current physics positions
        let final_pos = prev_pos.lerp(curr_pos, interp_fraction);

        transform0.translation = final_pos;
        transform0.rotation = heading.into();
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    asset_server: Res<AssetServer>,
    map: Res<Map>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for &message in reader.read() {
        let Do { event: Event::Spawn { ent, typ, qrz, attrs } } = message else { continue };

        match typ {
            EntityType::Actor(desc) => {
                let loc = Loc::new(qrz);

                // Initialize reaction queue with capacity based on Focus attribute
                let attrs_val = attrs.unwrap_or_default();
                let queue_capacity = queue_calcs::calculate_queue_capacity(&attrs_val);
                let reaction_queue = ReactionQueue::new(queue_capacity);

                // Handle entities that may have been evicted - spawn if needed
                let mut entity_cmd = if let Ok(e) = commands.get_entity(ent) {
                    e
                } else {
                    commands.spawn_empty()
                };

                entity_cmd
                    .insert((
                        loc,
                        typ,
                        // All actors need Behaviour::Controlled on client for movement interpolation
                        // (separate from PlayerControlled which marks player-controlled entities for ally/enemy logic)
                        Behaviour::Controlled,
                        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(get_asset(EntityType::Actor(desc))))),
                        Transform {
                            translation: map.convert(qrz),
                            scale: Vec3::ONE * map.radius(),
                            ..default()},
                        AirTime { state: Some(0), step: None },
                        NearestNeighbor::new(ent, loc),
                        Heading::default(),
                        Offset::default(),
                        KeyBits::default(),
                        Visibility::default(),
                        Physics::default(),
                    ))
                    .insert((
                        attrs_val,
                        reaction_queue,
                        crate::common::components::gcd::Gcd::new(),
                        crate::common::components::target::Target::default(), // For targeting system
                        crate::common::components::LastAutoAttack::default(), // For auto-attack cooldown
                        crate::common::components::tier_lock::TierLock::new(), // ADR-010 Phase 1: Tier lock targeting
                    ))
                    .observe(ready);

                // Health/Stamina/Mana/CombatState will be inserted by Incremental events from server
                // (do_incremental handles inserting missing components)
            }
            _ => continue,
        }
    }
}

pub fn try_gcd(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            writer.write(Do { event: Event::Gcd { ent, typ }});
        }
    }
}

fn get_asset(typ: EntityType) -> String {
    match typ {
        EntityType::Actor(desc) => {
            // Model is determined by identity, not triumvirate
            // Triumvirate (origin/approach/resilience) affects combat behavior only
            match desc.identity {
                ActorIdentity::Player => "actors/player-basic.glb".to_string(),
                ActorIdentity::Npc(npc_type) => match npc_type {
                    NpcType::WildDog => "actors/dog-basic.glb".to_string(),
                    NpcType::ForestSprite => "actors/sprite-basic.glb".to_string(),
                    NpcType::Juggernaut => "actors/juggernaut-basic.glb".to_string(),
                    NpcType::Defender => "actors/player-basic.glb".to_string(), // Reuse player model
                }
            }
        },
        _ => panic!("couldn't find asset for entity type {:?}", typ)
    }
}

/// Apply movement intent to predict remote entity movement (ADR-011)
///
/// When a MovementIntent arrives, start interpolating toward the predicted destination.
/// Local player is skipped (already predicted via Input system).
pub fn apply_movement_intent(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut query: Query<(&mut Offset, &Loc, &Heading)>,
    map: Res<Map>,
    time: Res<Time>,
    buffers: Res<InputQueues>,
) {
    for &message in reader.read() {
        let Do { event: Event::MovementIntent { ent, destination, duration_ms } } = message
            else { continue };

        // Skip intent for local player (we predict using Input, not Intent)
        if buffers.get(&ent).is_some() {
            continue;
        }

        let Ok((mut offset, loc, heading)) = query.get_mut(ent) else {
            continue;
        };

        // Calculate target position (destination tile + heading-adjusted offset)
        // Use actual Heading component (updated by broadcast_heading_changes)
        let dest_tile_center = map.convert(destination);
        let current_tile_world = map.convert(**loc);

        // Calculate heading-adjusted offset using actual Heading component
        let dest_offset = if **heading != default() {
            use crate::common::components::heading::HERE;
            let heading_neighbor = map.convert(destination + **heading);
            let direction = heading_neighbor - dest_tile_center;
            (direction * HERE).xz()
        } else {
            // No heading - stay at tile center
            Vec2::ZERO
        };
        let dest_world = dest_tile_center + Vec3::new(dest_offset.x, 0.0, dest_offset.y);

        // Calculate where we are RIGHT NOW in the current interpolation (before overwriting it)
        let current_interp_fraction = if offset.interp_duration > 0.0 {
            (offset.interp_elapsed / offset.interp_duration).min(1.0)
        } else {
            1.0  // Completed or no interpolation - we're at offset.state
        };
        let current_visual_offset = offset.prev_step.lerp(offset.step, current_interp_fraction);

        // Set up NEW interpolation: current visual position → predicted destination
        offset.prev_step = current_visual_offset;  // Start from where we are NOW
        offset.step = dest_world - current_tile_world; // Target is destination tile

        // Set interpolation duration based on intent
        offset.interp_duration = duration_ms as f32 / 1000.0;
        offset.interp_elapsed = 0.0;

        // Insert prediction tracking component for validation (only if entity exists)
        if let Ok(mut entity_cmd) = commands.get_entity(ent) {
            entity_cmd.insert(crate::common::components::movement_prediction::MovementPrediction::new(
                destination,
                time.elapsed() + Duration::from_millis(duration_ms as u64),
                time.elapsed(),
            ));
        }
    }
}
