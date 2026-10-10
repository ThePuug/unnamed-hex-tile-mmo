//! Targeting by facing: whether a target lies within an arc of an actor's
//! heading, and which of the actors in reach it targets.
//!
//! A `Heading` is one of [`HEADING_SLOTS`] bearings clockwise from north;
//! the bearing to a target is measured the same way, and the two are
//! compared the short way round. An actor strikes within [`STRIDE_ARC`]
//! either side of its heading without turning from its line: the three
//! forward hex faces, whose tile-to-tile bearings lie 60° apart.

use bevy::prelude::*;

use crate::{
    components::{behaviour::Side, heading::*, *},
    plugins::nntree::*,
};
use crate::tuning::Tuning;

/// Whether `target_loc` lies within the forward faces of `caster_heading`
/// from `caster_loc`: [`STRIDE_ARC`] either side of it.
pub fn is_in_facing_cone(
    caster_heading: Heading,
    caster_loc: Loc,
    target_loc: Loc,
) -> bool {
    within_arc(caster_heading, STRIDE_ARC, caster_loc, target_loc)
}

/// The half-angle either side of its heading an actor strikes within
/// without turning from its line: the three forward hex faces.
pub const STRIDE_ARC: f32 = 60.0;

/// Whether `target_loc` lies within `arc` degrees either side of
/// `caster_heading` from `caster_loc`. A target on the caster's own tile
/// always does.
pub fn within_arc(
    caster_heading: Heading,
    arc: f32,
    caster_loc: Loc,
    target_loc: Loc,
) -> bool {
    // Targets on the same tile are always in the facing cone
    // (e.g., multiple enemies standing on the same hex)
    if *caster_loc == *target_loc {
        return true;
    }

    off_heading(caster_heading, caster_loc, target_loc) <= arc
}

/// Degrees between `heading` and the bearing from `from` to `to`, the
/// shorter way round: 0 dead ahead to 180 dead behind.
fn off_heading(heading: Heading, from: Loc, to: Loc) -> f32 {
    let delta = (angle_between_locs(from, to) - heading.degrees()).abs();
    if delta > 180.0 { 360.0 - delta } else { delta }
}

/// Whether an attacker at `from` facing `heading`, striking within `arc`
/// degrees either side of it, may strike `to`: every attack, auto-attack
/// or ability, lands only on a target in its arc. An entity with no heading
/// faces every way.
pub fn faces(heading: Option<&Heading>, arc: f32, from: &Loc, to: &Loc) -> bool {
    heading.is_none_or(|heading| within_arc(*heading, arc, *from, *to))
}

/// Whether a strike from `from` facing `heading` at `to` crosses the
/// striker's line: past [`STRIDE_ARC`], where only a Grace arc reaches.
pub fn across(heading: Option<&Heading>, from: &Loc, to: &Loc) -> bool {
    heading.is_some_and(|heading| !within_arc(*heading, STRIDE_ARC, *from, *to))
}

/// Whether a strike from `from` lands on a target at `at` facing `heading`
/// from past its forward faces, a flank. A target with no heading is never
/// flanked.
pub fn flanked(heading: Option<&Heading>, at: &Loc, from: &Loc) -> bool {
    heading.is_some_and(|heading| !is_in_facing_cone(*heading, *at, *from))
}

/// The bearing from `from` to `to` in degrees clockwise from north, in
/// [0, 360): the six tile-to-tile bearings of the flat-top grid fall 60°
/// apart from north at 0.
fn angle_between_locs(from: Loc, to: Loc) -> f32 {
    // Calculate the difference vector in Qrz coordinates
    let dq = (to.q - from.q) as f32;
    let dr = (to.r - from.r) as f32;

    // Convert to Cartesian coordinates for flat-top hexes
    // Using standard flat-top hex conversion:
    // x = 3/2 * q
    // y = sqrt(3) * (r + q/2)
    let x = 1.5 * dq;
    let y = 1.732050808 * (dr + dq / 2.0); // sqrt(3) ≈ 1.732

    // Calculate angle using atan2 (returns radians, -π to π)
    // atan2(y, x) gives 0° for positive X axis
    let angle_rad = y.atan2(x);

    // Convert to degrees
    let mut angle_deg = angle_rad.to_degrees();

    // Flat-top hex: (q=1, r=0) = SE direction gives atan2 ≈ 30° in Cartesian.
    // We want SE to be 120° in our compass system, so add 90°.
    angle_deg += 90.0;

    // Normalize to [0, 360) range
    if angle_deg < 0.0 {
        angle_deg += 360.0;
    } else if angle_deg >= 360.0 {
        angle_deg -= 360.0;
    }

    angle_deg
}

/// How far a caster looks for a target, in tiles.
pub const TARGET_RADIUS: u32 = 30;

/// The entity a caster at `caster_loc` facing `caster_heading` targets: of
/// those `wanted` within [`TARGET_RADIUS`] and `arc` degrees either side of
/// its heading, one inside the forward faces before any across its line,
/// then the nearest, and among the equally near the one nearest dead
/// ahead. Never the caster itself.
///
/// `arc` is the arc the caster strikes within ([`arc_of`]), so whatever it
/// may strike it may target, and a wider arc still takes what stands in
/// front first. `wanted` says who may be targeted at all: an actor on a
/// hostile side for an attack, one on the caster's own side for an ally.
/// Hostile and ally targets are picked by this one rule, so both move with
/// the heading alike.
pub fn select_target(
    caster_ent: Entity,
    caster_loc: Loc,
    caster_heading: Heading,
    arc: f32,
    nntree: &NNTree,
    wanted: impl Fn(Entity) -> bool,
) -> Option<Entity> {
    nntree.within_tiles(caster_loc, TARGET_RADIUS as i64)
        // By entity, not location: several may stand on one tile
        .filter(|nn| nn.ent != caster_ent && wanted(nn.ent))
        .filter(|nn| within_arc(caster_heading, arc, caster_loc, nn.loc))
        .map(|nn| (
            nn.ent,
            !is_in_facing_cone(caster_heading, caster_loc, nn.loc),
            caster_loc.flat_distance(&nn.loc) as u32,
            off_heading(caster_heading, caster_loc, nn.loc),
        ))
        .min_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)).then(a.3.total_cmp(&b.3)))
        .map(|(ent, ..)| ent)
}

/// The half-angle an actor with `attrs` strikes and targets within: the
/// arc its Grace opens, the forward faces for one with no attributes.
pub fn arc_of(tuning: &Tuning, attrs: Option<&ActorAttributes>) -> f32 {
    attrs.map_or(STRIDE_ARC, |attrs| attrs.arc(tuning))
}

/// Points `target` at the actor `ent` faces from `loc` along `heading`
/// within its `arc` whose side it `wants` beside its own: a hostile side
/// for a `Target`, its own for an `AllyTarget`. With none in its arc the
/// current target clears and the last one stays, for a frame that keeps
/// showing it. An entity with no side targets nothing.
#[allow(clippy::too_many_arguments)]
pub fn update_targets_impl(
    ent: Entity,
    loc: Loc,
    heading: Heading,
    arc: f32,
    target: &mut impl crate::components::target::Selection,
    nntree: &NNTree,
    side_of: impl Fn(Entity) -> Option<Side>,
    wants: impl Fn(Side, Side) -> bool,
) {
    let picked = side_of(ent).and_then(|own| {
        select_target(ent, loc, heading, arc, nntree, |other| {
            side_of(other).is_some_and(|side| wants(own, side))
        })
    });
    target.select(picked);
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;
    use crate::components::behaviour::PlayerControlled;
    use crate::components::entity_type::*;

    // ===== FACING CONE TESTS =====

    #[test]
    fn test_facing_cone_target_outside_cone() {
        // Heading SE (120°), target to the NW should be outside
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // SE
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: -1, r: 0, z: 0 }); // West

        assert!(
            !is_in_facing_cone(heading, caster, target),
            "Target behind should not be in facing cone"
        );
    }

    #[test]
    fn test_facing_cone_target_at_same_tile() {
        // Target at same location as caster should return true
        // This handles cases where multiple entities occupy the same hex
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        assert!(
            is_in_facing_cone(heading, caster, target),
            "Should include targets on same tile (multiple entities on same hex)"
        );
    }

    #[test]
    fn test_facing_cone_all_six_headings() {
        // Test that each heading correctly identifies targets in front
        let test_cases = vec![
            (Qrz { q: 1, r: -1, z: 0 }, Qrz { q: 1, r: -1, z: 0 }, "Northeast"),
            (Qrz { q: 1, r: 0, z: 0 }, Qrz { q: 1, r: 0, z: 0 }, "East"),
            (Qrz { q: 0, r: 1, z: 0 }, Qrz { q: 0, r: 1, z: 0 }, "Southeast"),
            (Qrz { q: -1, r: 1, z: 0 }, Qrz { q: -1, r: 1, z: 0 }, "Southwest"),
            (Qrz { q: -1, r: 0, z: 0 }, Qrz { q: -1, r: 0, z: 0 }, "West"),
            (Qrz { q: 0, r: -1, z: 0 }, Qrz { q: 0, r: -1, z: 0 }, "Northwest"),
        ];

        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        for (heading_qrz, target_offset, direction_name) in test_cases {
            let heading = Heading::from_hex(heading_qrz);
            let target = Loc::new(*caster + target_offset);

            assert!(
                is_in_facing_cone(heading, caster, target),
                "{}: Target directly ahead should be in cone",
                direction_name
            );
        }
    }

    #[test]
    fn test_facing_cone_boundary_precision() {
        // Test the 120° cone (±60° from heading)
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // SE = 120°
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // NE at 60° is 60° from SE heading (120°) - at edge of cone
        let target_ne = Loc::new(Qrz { q: 1, r: -1, z: 0 }); // Northeast at 60°
        assert!(
            is_in_facing_cone(heading, caster, target_ne),
            "Northeast (60° delta) should be at edge of 120° cone"
        );

        // Target (1,1) at ~150° is ~30° from SE (120°) - well within cone
        let target_150 = Loc::new(Qrz { q: 1, r: 1, z: 0 });
        assert!(
            is_in_facing_cone(heading, caster, target_150),
            "Target at ~30° delta should be well within 120° cone"
        );

        // SW at 240° is 120° from SE (120°) - outside cone
        let target_sw = Loc::new(Qrz { q: -1, r: 1, z: 0 }); // Southwest at 240°
        assert!(
            !is_in_facing_cone(heading, caster, target_sw),
            "Southwest (120° delta) should be outside 120° cone"
        );
    }

    // ===== ANGLE CALCULATION TESTS =====

    #[test]
    fn test_angle_between_locs_cardinal_directions() {
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Flat-top hex compass bearings
        let test_cases = vec![
            (Qrz { q: 0, r: -1, z: 0 }, 0.0, "North"),
            (Qrz { q: 1, r: -1, z: 0 }, 60.0, "Northeast"),
            (Qrz { q: 1, r: 0, z: 0 }, 120.0, "Southeast"),
            (Qrz { q: 0, r: 1, z: 0 }, 180.0, "South"),
            (Qrz { q: -1, r: 1, z: 0 }, 240.0, "Southwest"),
            (Qrz { q: -1, r: 0, z: 0 }, 300.0, "Northwest"),
        ];

        for (target_qrz, expected_angle, direction_name) in test_cases {
            let target = Loc::new(target_qrz);
            let angle = angle_between_locs(origin, target);

            // Allow small floating point error
            let diff = (angle - expected_angle).abs();
            assert!(
                diff < 5.0,
                "{}: Expected angle ~{}, got {} (diff: {})",
                direction_name, expected_angle, angle, diff
            );
        }
    }

    // ===== TARGET SELECTION TESTS =====

    // Helper function to create a test world with entities
    fn setup_test_world() -> (World, NNTree) {
        let world = World::new();
        let nntree = NNTree::new_for_test();
        (world, nntree)
    }

    /// An actor's side: players on their own, every other actor wild
    fn side_of(world: &World, ent: Entity) -> Option<Side> {
        matches!(world.get::<EntityType>(ent), Some(EntityType::Actor(_)))
            .then(|| if world.get::<PlayerControlled>(ent).is_some() { Side::PLAYERS } else { Side::WILD })
    }

    /// The actors hostile to `caster`: players against everyone else
    fn hostile(world: &World, caster: Entity) -> impl Fn(Entity) -> bool + '_ {
        move |ent| side_of(world, ent).zip(side_of(world, caster)).is_some_and(|(side, own)| side.is_hostile_to(own))
    }

    /// The actors on `caster`'s side
    fn ally(world: &World, caster: Entity) -> impl Fn(Entity) -> bool + '_ {
        move |ent| side_of(world, ent).zip(side_of(world, caster)).is_some_and(|(side, own)| side == own)
    }

    // Helper to spawn an actor at a location
    fn spawn_actor(world: &mut World, nntree: &mut NNTree, loc: Loc) -> Entity {
        use crate::components::entity_type::actor::*;

        let entity = world.spawn((
            EntityType::Actor(ActorImpl {
                origin: Origin::Evolved,
                approach: Approach::Direct,
                resilience: Resilience::Vital,
                identity: ActorIdentity::Npc(crate::archetype::EnemyArchetype::Berserker), // Test helper - generic NPC
            }),
            loc,
        )).id();

        nntree.insert(NearestNeighbor::new(entity, loc));
        entity
    }

    #[test]
    fn test_select_target_single_target_ahead() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn caster (player)
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        // Spawn target directly ahead (east) - NPC
        let target = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 }));

        let result = select_target(caster, caster_loc, heading, STRIDE_ARC, &nntree, hostile(&world, caster));

        assert_eq!(result, Some(target), "Should select the target directly ahead");
    }

    #[test]
    fn test_select_target_no_targets() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        let result = select_target(caster, caster_loc, heading, STRIDE_ARC, &nntree, hostile(&world, caster));

        assert_eq!(result, None, "Should return None when no targets exist");
    }

    #[test]
    fn test_select_target_behind_caster() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn target behind (west) - NPC
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: -1, r: 0, z: 0 }));

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        let result = select_target(caster, caster_loc, heading, STRIDE_ARC, &nntree, hostile(&world, caster));

        assert_eq!(result, None, "Should not select target behind caster");
    }

    #[test]
    fn test_select_target_nearest_wins() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn targets at different distances, all in front - NPCs
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 3, r: 0, z: 0 })); // Far
        let nearest = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Near
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 2, r: 0, z: 0 })); // Mid

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        let result = select_target(caster, caster_loc, heading, STRIDE_ARC, &nntree, hostile(&world, caster));

        assert_eq!(result, Some(nearest), "Should select the nearest target");
    }

    #[test]
    fn test_select_target_geometric_tiebreaker() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East = 90°

        // Spawn two targets at same distance (1 hex away) - NPCs
        // One directly ahead (east), one at an angle (northeast)
        let directly_ahead = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // East
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: -1, z: 0 })); // Northeast

        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        let result = select_target(caster, caster_loc, heading, STRIDE_ARC, &nntree, hostile(&world, caster));

        assert_eq!(
            result, Some(directly_ahead),
            "Should select target closest to exact heading angle"
        );
    }

    #[test]
    fn a_wider_arc_targets_what_stands_across_the_line() {
        let (mut world, mut nntree) = setup_test_world();
        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);
        let behind = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: -1, r: 0, z: 0 }));

        let pick = |arc| select_target(caster, caster_loc, heading, arc, &nntree, hostile(&world, caster));
        assert_eq!(pick(STRIDE_ARC), None, "the forward faces do not reach behind");
        assert_eq!(pick(180.0), Some(behind), "an arc every way does");
    }

    #[test]
    fn a_target_in_front_is_taken_before_a_nearer_one_across_the_line() {
        let (mut world, mut nntree) = setup_test_world();
        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);
        spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: -1, r: 0, z: 0 }));
        let ahead = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 3, r: 0, z: 0 }));

        let result = select_target(caster, caster_loc, heading, 180.0, &nntree, hostile(&world, caster));
        assert_eq!(result, Some(ahead));
    }

    #[test]
    fn test_select_ally_target_automatic_selects_nearest() {
        let (mut world, mut nntree) = setup_test_world();

        let caster_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }); // East

        // Spawn caster (player)
        let caster = spawn_actor(&mut world, &mut nntree, caster_loc);
        world.entity_mut(caster).insert(PlayerControlled);

        // Spawn allies at different distances
        let close_ally = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 1, r: 0, z: 0 })); // Distance 1 (Close)
        world.entity_mut(close_ally).insert(PlayerControlled);

        let mid_ally = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 5, r: 0, z: 0 })); // Distance 5 (Mid)
        world.entity_mut(mid_ally).insert(PlayerControlled);

        let far_ally = spawn_actor(&mut world, &mut nntree, Loc::new(Qrz { q: 11, r: 0, z: 0 }));
        world.entity_mut(far_ally).insert(PlayerControlled);

        let result = select_target(
            caster,
            caster_loc,
            heading,
            STRIDE_ARC,
            &nntree,
            ally(&world, caster),
        );

        assert_eq!(
            result, Some(close_ally),
            "the nearest ally"
        );
    }
}
