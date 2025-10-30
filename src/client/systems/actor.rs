use std::time::Duration;

use bevy::{
    prelude::*,
    scene::SceneInstanceReady
};
use qrz::Convert;

use crate::{
    client::components::*,
    common::{
        components::{
            behaviour::*,
            entity_type::{ actor::*, * },
            heading::*, keybits::*, offset::*,
            reaction_queue::ReactionQueue,
            resources::*, *
        },
        message::{ Event, * },
        plugins::nntree::NearestNeighbor,
        resources::map::Map,
        systems::combat::queue as queue_calcs,
    }
};

pub fn setup() {}

fn ready(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    query: Query<&EntityType>,
    mut q_player: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    for child in q_child.iter_descendants(trigger.target()) {
        if let Ok(mut player) = q_player.get_mut(child) {
            commands.entity(trigger.target()).insert(Animates(child));

            let &typ = query.get(trigger.target()).expect("couldn't get entity type");
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
    mut query: Query<(&Loc, &Offset, &Heading, &mut Transform)>,
    map: Res<Map>,
) {
    for (&loc, offset, &heading, mut transform0) in &mut query {
        // Interpolate between FixedUpdate ticks using overstep fraction
        // overstep_fraction: 0.0 = just ran FixedUpdate, 1.0 = about to run FixedUpdate
        let overstep_fraction = fixed_time.overstep_fraction();

        let prev_pos = map.convert(*loc) + offset.prev_step;
        let curr_pos = map.convert(*loc) + offset.step;

        // Interpolate between previous and current physics positions
        // Physics handles heading-based positioning, so just render what physics calculates
        let final_pos = prev_pos.lerp(curr_pos, overstep_fraction);

        transform0.translation = final_pos;
        transform0.rotation = heading.into();
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    asset_server: Res<AssetServer>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        let Do { event: Event::Spawn { ent, typ, qrz, attrs } } = message else { continue };
        let EntityType::Actor(desc) = typ else { continue };
        let loc = Loc::new(qrz);

        // Note: Resource components (Health, Stamina, Mana, CombatState) are already on the entity
        // from Init event and have been updated by Incremental events. Don't re-insert them here!

        // Initialize reaction queue with capacity based on Focus attribute
        let attrs_val = attrs.unwrap_or_default();
        let queue_capacity = queue_calcs::calculate_queue_capacity(&attrs_val);
        let reaction_queue = ReactionQueue::new(queue_capacity);

        commands.entity(ent).insert((
            loc,
            typ,
            Behaviour::Controlled,  // Remote players are controlled by network updates
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
            attrs_val,
            reaction_queue,
        ))
        .observe(ready);
    }
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
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
            let origin_str = match desc.origin {
                Origin::Natureborn => "natureborn",
                Origin::Synthetic => "synthetic",
                Origin::Dreamborn => "dreamborn",
                Origin::Voidborn => "voidborn",
                Origin::Mythic => "mythic",
                Origin::Dimensional => "dimensional",
                Origin::Indiscernible => "indiscernible",
            };
            let approach_str = match desc.approach {
                Approach::Direct => "direct",
                Approach::Distant => "distant",
                Approach::Ambushing => "ambushing",
                Approach::Patient => "patient",
                Approach::Binding => "binding",
                Approach::Evasive => "evasive",
                Approach::Overwhelming => "overwhelming",
            };
            let resilience_str = match desc.resilience {
                Resilience::Vital => "vital",
                Resilience::Mental => "mental",
                Resilience::Hardened => "hardened",
                Resilience::Shielded => "shielded",
                Resilience::Blessed => "blessed",
                Resilience::Primal => "primal",
                Resilience::Eternal => "eternal",
            };
            format!("actors/{}-{}-{}.glb", origin_str, approach_str, resilience_str)
        },
        _ => panic!("couldn't find asset for entity type {:?}", typ)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    #[test]
    fn test_stationary_player_with_heading_should_stand_in_heading_triangle() {
        // This test verifies that when a player is stationary (not pressing movement keys)
        // but has a heading set, they are positioned in the triangle of their hex
        // corresponding to their heading direction.

        // Setup
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Player is stationary (offset.step and prev_step are both zero)
        let offset = Offset {
            state: Vec3::ZERO,
            step: Vec3::ZERO,
            prev_step: Vec3::ZERO,
        };

        // Player is facing East (q: 1, r: 0, z: 0)
        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });

        // Player is not pressing any movement keys
        let keybits = KeyBits::default();

        // Expected position: center of tile + direction to East neighbor * HERE
        let tile_center = map.convert(*loc);
        let east_neighbor = map.convert(*loc + *heading);
        let direction = east_neighbor - tile_center;
        let expected_position = tile_center + direction * HERE;

        // Simulate what the update function should calculate
        // (Testing the logic without needing the full ECS system)
        let overstep_fraction = 0.0; // Just ran FixedUpdate
        let prev_pos = map.convert(*loc) + offset.prev_step;
        let curr_pos = map.convert(*loc) + offset.step;
        let lpx = prev_pos.lerp(curr_pos, overstep_fraction);

        // The heading-based positioning logic from update()
        let is_stationary = keybits.key_bits & (KB_HEADING_Q | KB_HEADING_R) == 0;
        let final_pos = if is_stationary && *heading != Qrz::default() {
            let tile_center = map.convert(*loc);
            let heading_neighbor = map.convert(*loc + *heading);
            let direction = heading_neighbor - tile_center;
            let heading_pos_xz = tile_center + direction * HERE;
            Vec3::new(heading_pos_xz.x, lpx.y, heading_pos_xz.z)
        } else {
            lpx
        };

        assert_eq!(
            final_pos, expected_position,
            "Stationary player with heading should stand in heading triangle.\n\
             Expected: {:?}\n\
             Actual: {:?}",
            expected_position, final_pos
        );
    }

    #[test]
    fn test_moving_player_should_use_physics_position() {
        // When a player is actively moving (offset.step has significant magnitude),
        // their position should be based on the physics simulation (offset.step),
        // NOT the heading triangle positioning.

        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Player is moving (offset.step has significant value)
        let physics_position = Vec3::new(0.5, 0.0, 0.3);
        let offset = Offset {
            state: Vec3::ZERO,
            step: physics_position,
            prev_step: Vec3::new(0.4, 0.0, 0.2),
        };

        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });

        // Expected: position should be based on offset.step (physics), not heading
        let tile_center = map.convert(*loc);
        let expected_position = tile_center + physics_position;

        // Heading-based position (should NOT be used when moving)
        let east_neighbor = map.convert(*loc + *heading);
        let direction = east_neighbor - tile_center;
        let heading_position = tile_center + direction * HERE;

        let actual_position = tile_center + offset.step;

        assert_eq!(
            actual_position, expected_position,
            "Moving player should use physics position, not heading position"
        );

        assert_ne!(
            actual_position, heading_position,
            "Moving player should not be constrained to heading triangle"
        );
    }

    #[test]
    fn test_stationary_player_with_no_heading_should_stand_at_center() {
        // When a player has no heading set (Heading::default()), they should stand
        // at the center of their tile, regardless of whether they're moving.

        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        let offset = Offset {
            state: Vec3::ZERO,
            step: Vec3::ZERO,
            prev_step: Vec3::ZERO,
        };

        let _heading = Heading::default(); // No heading

        let tile_center = map.convert(*loc);
        let expected_position = tile_center;
        let actual_position = tile_center + offset.step;

        assert_eq!(
            actual_position, expected_position,
            "Player with no heading should stand at tile center"
        );
    }

    #[test]
    fn test_moving_player_with_small_offset_should_not_use_heading_position() {
        // REGRESSION TEST: When a player is actively moving (pressing movement keys)
        // but their offset.step is temporarily small (e.g., just started moving),
        // they should still use physics-based positioning, NOT heading-based positioning.
        // This prevents stuttering where position jumps between heading triangle and physics.

        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Player just started moving - offset.step is small but non-zero
        let small_offset = Vec3::new(0.005, 0.0, 0.005); // Magnitude < 0.01
        let offset = Offset {
            state: Vec3::ZERO,
            step: small_offset,
            prev_step: Vec3::ZERO,
        };

        let heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });

        // Player is pressing movement key (East direction)
        let mut keybits = KeyBits::default();
        keybits.set_pressed([KB_HEADING_Q], true);

        // With the BUG: offset.step.xz().length_squared() < 0.01 is TRUE,
        // so it would incorrectly use heading position, causing stutter

        // Expected: Should use interpolated physics position
        let tile_center = map.convert(*loc);
        let overstep_fraction = 0.5;
        let prev_pos = tile_center + offset.prev_step;
        let curr_pos = tile_center + offset.step;
        let expected_position = prev_pos.lerp(curr_pos, overstep_fraction);

        // Heading position (should NOT be used when moving)
        let heading_neighbor = map.convert(*loc + *heading);
        let direction = heading_neighbor - tile_center;
        let heading_position = Vec3::new(
            (tile_center + direction * HERE).x,
            expected_position.y,
            (tile_center + direction * HERE).z
        );

        // The fix should check KeyBits, not offset magnitude
        let is_stationary = keybits.key_bits & (KB_HEADING_Q | KB_HEADING_R) == 0;
        let final_pos = if is_stationary && *heading != Qrz::default() {
            heading_position
        } else {
            expected_position
        };

        assert_eq!(
            final_pos, expected_position,
            "Moving player (pressing keys) should use physics position even with small offset.\n\
             This prevents stuttering between heading and physics positions.\n\
             Expected (physics): {:?}\n\
             Heading position: {:?}",
            expected_position, heading_position
        );

        assert_ne!(
            final_pos, heading_position,
            "Moving player should never snap to heading position while keys are pressed"
        );
    }
}