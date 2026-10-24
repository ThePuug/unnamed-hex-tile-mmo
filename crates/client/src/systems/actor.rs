use std::time::Duration;

use bevy::{gltf::GltfNode, prelude::*, world_serialization::WorldInstanceReady};

use crate::{components::*, systems::{following_turn, animator::{Clip, Clips, Rig}}};
use common_bevy::{
    components::{
        entity_type::{ actor::*, * },
        heading::*, keybits::*,
        position::{Position, VisualPosition},
        reaction_queue::ReactionQueue,
        status::Status,
        *
    },
    message::{ Event, * },
    plugins::nntree::NearestNeighbor,
    resources::map::Map,
    archetype::EnemyArchetype,
};

/// Plays an actor's scene once it is spawned: every clip its GLB holds, each
/// found by name (`animator::Clip`), the idle playing, which the animator
/// switches from.
pub(crate) fn ready(
    trigger: On<WorldInstanceReady>,
    mut commands: Commands,
    query: Query<(&EntityType, &Rig)>,
    mut q_player: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
) {
    let entity = trigger.entity;
    for child in q_child.iter_descendants(entity) {
        if let Ok(mut player) = q_player.get_mut(child) {
            commands.entity(entity).insert(Animates(child));

            let (&typ, rig) = query.get(entity).expect("couldn't get entity type");
            // The scene came from the file the rig holds, which names its
            // animations.
            let Some(gltf) = gltfs.get(&rig.0) else {
                warn!("{} has no Gltf asset loaded; its actor stays unanimated", get_asset(typ));
                continue;
            };
            animate(child, &mut player, gltf, &nodes, &mut graphs, &mut commands);
        }
    }
}

/// Gives the scene's `AnimationPlayer`, on `child`, a graph of every clip
/// `gltf` holds, the idle playing.
pub(crate) fn animate(
    child: Entity,
    player: &mut AnimationPlayer,
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    graphs: &mut Assets<AnimationGraph>,
    commands: &mut Commands,
) {
    let (graph, clips) = Clips::from_gltf(gltf, nodes);
    let handle = AnimationGraphHandle(graphs.add(graph));
    let mut transitions = AnimationTransitions::new();
    if let Some(idle) = clips.node(Clip::Idle) {
        transitions.play(player, idle, Duration::ZERO).set_speed(1.).repeat();
    }
    commands.entity(child)
        .insert(handle)
        .insert(transitions)
        .insert(clips);
}

/// Exponential decay constant of the facing's easing between headings at
/// whole turn pace: a heading steps by a bearing at a time, and the body
/// turns through it ([`following_turn`]).
const FACING_EASE: f32 = 15.0;
/// Radians short of the heading at which the facing lands on it. Eased all
/// the way, the slerp never arrives: its last steps are a lerp and a
/// normalise that flip a component by an ulp indefinitely, and far from
/// the origin an ulp of rotation rounds a joint's world position across
/// a float step — the body shakes on screen.
const FACING_SNAP: f32 = 0.001;

/// The facing after `dt` seconds of turning from `current` toward
/// `heading` at `turn_pace`, the share of its turn the actor keeps
/// (`Status::turn_pace`): eased, and landed exactly once within the snap.
pub fn face(current: Quat, heading: Heading, turn_pace: f32, dt: f32) -> Quat {
    let facing: Quat = heading.into();
    if current.angle_between(facing) < FACING_SNAP {
        return facing;
    }
    current.slerp(facing, 1.0 - (-following_turn(FACING_EASE, turn_pace) * dt).exp())
}

/// Places each actor where it is drawn, turned toward its heading — or,
/// sliding round a target, toward the way it goes, so it runs round rather
/// than sideways; the heading takes the turn back as the slide ends.
pub fn update(
    mut query: Query<(&Heading, &mut Transform, &VisualPosition, Option<&Displacing>, Option<&Status>), (With<Loc>, Without<DeathMarker>)>,
    time: Res<Time>,
) {
    for (&heading, mut transform0, vis, displacing, status) in &mut query {
        // The visual, not the position: the same value the camera reads,
        // whichever runs first
        let final_pos = vis.current();

        let circling = displacing.filter(|d| d.around.is_some())
            .and_then(|_| Heading::toward(vis.from, vis.to));
        let turned = face(transform0.rotation, circling.unwrap_or(heading), Status::turn_pace_of(status), time.delta_secs());
        // Written only when it moves: the same value written again marks
        // the whole rig changed and the skin is re-extracted for nothing.
        if transform0.translation != final_pos || transform0.rotation != turned {
            transform0.translation = final_pos;
            transform0.rotation = turned;
        }
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
    origin: Res<crate::resources::RenderOrigin>,
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

                let attrs_val = attrs.unwrap_or_default();
                let reaction_queue = ReactionQueue::default();

                // Handle entities that may have been evicted - spawn if needed
                let mut entity_cmd = if let Ok(e) = commands.get_entity(ent) {
                    e
                } else {
                    commands.spawn_empty()
                };

                // Where it is drawn, about the render origin.
                let spawn_world: Vec3 = origin.render_tile(&map, qrz);

                entity_cmd
                    .insert((
                        loc,
                        typ,
                        (
                            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(get_asset(EntityType::Actor(desc))))),
                            Rig(asset_server.load(get_asset(EntityType::Actor(desc)))),
                        ),
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
                        // New position and visual interpolation components
                        Position::at_tile(qrz),
                        VisualPosition::at(spawn_world),
                    ))
                    .insert((
                        attrs_val,
                        reaction_queue,
                        common_bevy::components::target::Target::default(), // For targeting system
                        common_bevy::components::AttackRange::default(), // Auto-attack range (melee default)
                    ))
                    .observe(ready);

                let actor_entity = entity_cmd.id();

                if diagnostics.grid_visible {
                    spawn_debug_sphere(&mut commands, &mut meshes, &mut materials, actor_entity);
                }

                // Health/Endurance/CombatState will be inserted by Incremental events from server
                // (do_incremental handles inserting missing components)
            }
            _ => continue,
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
            ActorIdentity::Npc(archetype) => match archetype {
                EnemyArchetype::Berserker => "dog",
                EnemyArchetype::Flanker => "sprite",
                EnemyArchetype::Juggernaut => "juggernaut",
                EnemyArchetype::Skirmisher => "grasshopper",
                EnemyArchetype::Ambusher => "crocodile",
                EnemyArchetype::Defender => "player",
            }
        },
        _ => panic!("couldn't find asset for entity type {:?}", typ)
    }
}

pub(crate) fn get_asset(typ: EntityType) -> String {
    format!("actors/{}-basic.glb", actor_name(typ))
}

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

#[cfg(test)]
mod facing_tests {
    use super::*;
    use common_bevy::systems::movement::turn_interval_ms;

    /// A turn eases in and then lands: the facing becomes the heading's
    /// rotation bit for bit and never moves again.
    #[test]
    fn the_facing_lands_on_the_heading_and_stays() {
        let heading = Heading::from_degrees(105.0);
        let target: Quat = heading.into();
        let mut facing = Quat::IDENTITY;
        for _ in 0..600 {
            facing = face(facing, heading, 1.0, 1.0 / 60.0);
        }
        assert_eq!(facing, target);
        for _ in 0..600 {
            let next = face(facing, heading, 1.0, 1.0 / 60.0);
            assert_eq!(next, facing);
            facing = next;
        }
    }

    /// A slower turn eases as much slower: when its next step is due, the
    /// body is as far through the last as it is at whole pace, so it never
    /// lands and rests between steps.
    #[test]
    fn the_facing_keeps_pace_with_the_turn() {
        let heading = Heading::from_degrees(15.0);
        let target: Quat = heading.into();
        let left = |pace: f32| {
            let interval = turn_interval_ms(pace).unwrap() as f32 / 1000.0;
            face(Quat::IDENTITY, heading, pace, interval).angle_between(target)
        };
        let whole = left(1.0);
        for pace in [0.6, 0.2, 2.0] {
            assert!((left(pace) - whole).abs() < 1e-3, "pace {pace}: {} left against {whole}", left(pace));
        }
    }
}
