use std::time::Duration;

use bevy::{prelude::*, scene::SceneInstanceReady};
use qrz::Convert;

use crate::{components::*, systems::animator::Clip};
use common_bevy::{
    components::{
        behaviour::Behaviour,
        entity_type::{ actor::*, * },
        heading::*, keybits::*,
        position::{Position, VisualPosition},
        reaction_queue::ReactionQueue,
        *
    },
    message::{ Event, * },
    plugins::nntree::NearestNeighbor,
    resources::map::Map,
};

pub fn setup() {}

/// Plays an actor's scene once it is spawned: every clip its GLB holds, in the
/// order the asset numbers them (`animator::Clip`), the idle playing, which
/// the animator switches from.
pub(crate) fn ready(
    trigger: On<SceneInstanceReady>,
    mut commands: Commands,
    query: Query<&EntityType>,
    mut q_player: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
) {
    let entity = trigger.entity;
    for child in q_child.iter_descendants(entity) {
        if let Ok(mut player) = q_player.get_mut(child) {
            commands.entity(entity).insert(Animates(child));

            let &typ = query.get(entity).expect("couldn't get entity type");
            let asset = get_asset(typ);
            // The scene is loaded, so the file it came from is too, and its
            // animations come in file order: the order the asset numbers them.
            let clips: Vec<Handle<AnimationClip>> = match gltfs.get(&asset_server.load::<Gltf>(asset.clone())) {
                Some(gltf) => gltf.animations.clone(),
                None => (0..3).map(|i| asset_server.load(GltfAssetLabel::Animation(i).from_asset(asset.clone()))).collect(),
            };
            let (graph, _) = AnimationGraph::from_clips(clips);
            let handle = AnimationGraphHandle(graphs.add(graph));
            let mut transitions = AnimationTransitions::new();
            transitions.play(&mut player, Clip::Idle.node(), Duration::ZERO).set_speed(1.).repeat();
            commands.entity(child)
                .insert(handle)
                .insert(transitions);
        }
    }
}

pub fn update(
    mut query: Query<(&Loc, &Heading, &mut Transform, Option<&VisualPosition>), Without<DeathMarker>>,
    map: Res<Map>,
) {
    for (&loc, &heading, mut transform0, vis_pos) in &mut query {
        let final_pos = if let Some(vis) = vis_pos {
            // Use VisualPosition for smooth, jitter-free rendering
            vis.current()
        } else {
            // Fallback: tile center for entities without VisualPosition
            map.convert(*loc)
        };

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
    diagnostics: Res<crate::plugins::diagnostics::DiagnosticsState>,
) {
    for message in reader.read() {
        let Do { event: Event::Spawn { ent, typ, qrz, attrs } } = message else { continue };
        let ent = *ent;
        let typ = *typ;
        let qrz = *qrz;
        let attrs = *attrs;

        match typ {
            EntityType::Actor(desc) => {
                let loc = Loc::new(qrz);

                // Initialize reaction queue with capacity based on Focus attribute
                let attrs_val = attrs.unwrap_or_default();
                let queue_capacity = attrs_val.window_size();
                let reaction_queue = ReactionQueue::new(queue_capacity);

                // Handle entities that may have been evicted - spawn if needed
                let mut entity_cmd = if let Ok(e) = commands.get_entity(ent) {
                    e
                } else {
                    commands.spawn_empty()
                };

                let spawn_world: Vec3 = map.convert(qrz);

                entity_cmd
                    .insert((
                        loc,
                        typ,
                        // All actors need Behaviour::Controlled on client for movement interpolation
                        // (separate from PlayerControlled which marks player-controlled entities for ally/enemy logic)
                        Behaviour::Controlled,
                        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(get_asset(EntityType::Actor(desc))))),
                        Transform {
                            translation: spawn_world,
                            scale: Vec3::ONE * map.radius(),
                            ..default()},
                        GlobalTransform::default(),
                        AirTime { state: Some(0), step: None },
                        NearestNeighbor::new(ent, loc),
                        Heading::default(),
                        Turn::default(),
                        KeyBits::default(),
                        Visibility::default(),
                        Physics::default(),
                        // New position and visual interpolation components
                        Position::at_tile(qrz),
                        VisualPosition::at(spawn_world),
                    ))
                    .insert((
                        attrs_val,
                        reaction_queue,
                        common_bevy::components::gcd::Gcd::new(),
                        common_bevy::components::target::Target::default(), // For targeting system
                        common_bevy::components::LastAutoAttack::default(), // For auto-attack cooldown
                        common_bevy::components::AttackRange::default(), // Auto-attack range (melee default)
                        common_bevy::components::tier_lock::TierLock::new(), // Tier lock targeting
                    ))
                    .observe(ready);

                let actor_entity = entity_cmd.id();

                if diagnostics.grid_visible {
                    spawn_debug_sphere(&mut commands, &mut meshes, &mut materials, actor_entity);
                }

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
    for message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            writer.write(Do { event: Event::Gcd { ent: *ent, typ: *typ }});
        }
    }
}

/// The body an actor is drawn with: its rig under `actors/`, and the cut of
/// every piece it wears under `models/`. Identity decides it; the triumvirate
/// shapes combat only.
pub fn actor_name(typ: EntityType) -> &'static str {
    match typ {
        EntityType::Actor(desc) => match desc.identity {
            ActorIdentity::Player => "player",
            ActorIdentity::Npc(npc_type) => match npc_type {
                NpcType::WildDog => "dog",
                NpcType::ForestSprite => "sprite",
                NpcType::Juggernaut => "juggernaut",
                NpcType::Defender => "player",
            }
        },
        _ => panic!("couldn't find asset for entity type {:?}", typ)
    }
}

pub(crate) fn get_asset(typ: EntityType) -> String {
    format!("actors/{}-basic.glb", actor_name(typ))
}

/// Apply movement intent to predict remote entity movement ( +)

/// When a MovementIntent arrives, start interpolating toward the predicted destination.
/// Local player is skipped (already predicted via Input system).
/// Spawn a debug sphere as a child of the given actor entity.
/// Called at actor spawn time (if grid visible) and when grid is toggled on.
pub fn spawn_debug_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    actor_entity: Entity,
) {
    let debug_sphere = commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.05))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            emissive: Color::srgb(1.0, 0.0, 0.0).into(),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        PlayerOriginDebug,
        Visibility::Visible,
    )).id();

    commands.entity(actor_entity).add_child(debug_sphere);
}
