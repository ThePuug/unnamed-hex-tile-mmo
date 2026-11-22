//! Ground telegraphs: while an actor activates a skill the gate checks a
//! reach for, aimed at the viewed actor, the tiles it can land on are
//! outlined on the ground, the striker's reach inside its arc. In each a
//! second outline grows from the tile's middle and meets the first as the
//! skill lands. The viewed actor's own tile is spoked while the skill would
//! land on it, so standing in one reads apart from standing beside one.
//! Drawn afresh each frame from the activations as they stand, with the
//! target indicators' gizmos, so a telegraph goes as its activation does.

use std::ops::RangeInclusive;

use bevy::prelude::*;
use qrz::Qrz;

use common_bevy::{
    components::{activation::Activating, heading::Heading, resources::Health, ActorAttributes, AttackRange, Loc},
    resources::map::Map,
    systems::targeting,
    tuning::Tuning,
};

use crate::{
    components::Viewed,
    resources::RenderOrigin,
    systems::target_indicator::{drawn_vertices, TargetGizmos},
};

/// A telegraphed tile's colour, and the viewed actor's own while inside
const DANGER: Color = Color::srgb(1.0, 0.4, 0.1);
const STOOD_IN: Color = Color::srgb(1.0, 0.95, 0.45);
/// How faint a telegraph is as its activation begins; it is whole as the
/// skill lands
const FAINT: f32 = 0.35;

/// Whether a skill reaching `within` lands, from `from` facing `heading`,
/// on one standing at `at`: the reach and the arc the server judges it by
/// as its activation ends (`combat::abilities`).
pub fn lands(within: &RangeInclusive<i32>, from: Loc, heading: Option<&Heading>, at: Loc) -> bool {
    within.contains(&from.distance(&at)) && targeting::faces(heading, &from, &at)
}

/// The tiles a skill reaching `within` can land on from `from` facing
/// `heading`, each as `ground` has it at its column (None where there is
/// no tile): the striker's own first, then ring by ring outward.
pub fn covered(within: &RangeInclusive<i32>, from: Loc, heading: Option<&Heading>, ground: impl Fn(i32, i32) -> Option<Qrz>) -> Vec<Qrz> {
    let rings = (1..=(*within.end()).max(0) as u32)
        .flat_map(|radius| from.ring(radius))
        .filter_map(|at| ground(at.q, at.r));
    std::iter::once(*from).chain(rings).filter(|tile| lands(within, from, heading, Loc::new(*tile))).collect()
}

/// Draws the telegraph of every skill under way that names the viewed
/// actor as its target. Nothing is drawn while the viewed actor is dead.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut gizmos: Gizmos<TargetGizmos>,
    viewed: Query<(Entity, &Loc, &Health), With<Viewed>>,
    strikers: Query<(&Activating, &Loc, Option<&Heading>, Option<&AttackRange>, Option<&ActorAttributes>)>,
    tuning: Res<Tuning>,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    let Ok((aimed_at, stood, health)) = viewed.single() else {
        return;
    };
    if health.state <= 0.0 {
        return;
    }
    let now = server.now(time.elapsed().as_millis());
    for (activating, from, heading, own_reach, attrs) in &strikers {
        let Some(activation) = activating.0.filter(|activation| activation.target == Some(aimed_at)) else { continue };
        let reach = activation.ability.reach(&tuning, &attrs.copied().unwrap_or_default(), own_reach.copied().unwrap_or_default().0);
        let Some(within) = reach else { continue };
        let progress = activation.progress(now);
        let inside = lands(&within, *from, heading, *stood);

        let mut tiles = covered(&within, *from, heading, |q, r| map.get_by_qr(q, r).map(|(tile, _)| tile));
        // The tile stood on is drawn once, spoked, after the rest
        tiles.retain(|tile| !inside || (tile.q, tile.r) != (stood.q, stood.r));
        let stood_on = inside.then(|| map.get_by_qr(stood.q, stood.r)).flatten().map(|(tile, _)| tile);

        for (tile, color) in tiles.into_iter().map(|tile| (tile, DANGER)).chain(stood_on.map(|tile| (tile, STOOD_IN))) {
            let vertices = drawn_vertices(&map, &origin, tile);
            let (corners, middle) = (&vertices[..6], vertices[6]);
            let color = color.with_alpha(FAINT + (1.0 - FAINT) * progress);
            gizmos.linestrip((0..=6).map(|corner| corners[corner % 6]), color);
            gizmos.linestrip((0..=6).map(|corner| middle + (corners[corner % 6] - middle) * progress), color);
            if Some(tile) == stood_on {
                for corner in corners {
                    gizmos.line(middle, *corner, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{components::heading::HEADING_SLOTS, message::AbilityType};

    fn flat(q: i32, r: i32) -> Option<Qrz> {
        Some(Qrz { q, r, z: 0 })
    }

    fn reach_of(ability: AbilityType, own: i32) -> RangeInclusive<i32> {
        ability.reach(&Tuning::default(), &ActorAttributes::default(), own).expect("a skill with a reach")
    }

    /// Every tile out to `radius` of `from`, on `ground`
    fn around(from: Loc, radius: u32, ground: impl Fn(i32, i32) -> Option<Qrz>) -> Vec<Qrz> {
        (0..=radius).flat_map(|ring| from.ring(ring)).filter_map(|at| ground(at.q, at.r)).collect()
    }

    #[test]
    fn a_telegraph_covers_exactly_the_tiles_the_gates_reach_and_arc_admit() {
        let from = Loc::new(Qrz { q: 3, r: -7, z: 0 });
        for own in 1..=4 {
            let within = reach_of(AbilityType::Overpower, own);
            for slot in 0..HEADING_SLOTS {
                let heading = Heading::from_slot(slot);
                let drawn = covered(&within, from, Some(&heading), flat);
                for tile in around(from, own as u32 + 2, flat) {
                    let at = Loc::new(tile);
                    let admitted = within.contains(&from.distance(&at)) && targeting::is_in_facing_cone(heading, from, at);
                    assert_eq!(drawn.contains(&tile), admitted, "{tile:?} at reach {own}, heading {slot}");
                }
                assert_eq!(drawn.len(), drawn.iter().collect::<std::collections::HashSet<_>>().len(), "each once");
            }
        }
    }

    #[test]
    fn it_lies_ahead_of_the_striker_and_widens_with_its_reach() {
        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let near = covered(&reach_of(AbilityType::Frenzy, 1), from, Some(&heading), flat);
        let far = covered(&reach_of(AbilityType::Frenzy, 3), from, Some(&heading), flat);
        assert!(near.contains(&Qrz { q: 1, r: 0, z: 0 }), "the tile dead ahead");
        assert!(near.contains(&*from), "and its own, where another may stand");
        assert!(!far.contains(&Qrz { q: -1, r: 0, z: 0 }), "never the one behind");
        assert!(near.iter().all(|tile| far.contains(tile)) && far.len() > near.len());
        assert_eq!(covered(&reach_of(AbilityType::Frenzy, 1), from, None, flat).len(), 7, "one with no heading faces every way");
    }

    #[test]
    fn a_cliff_too_tall_to_strike_up_is_left_out_and_a_gap_in_the_ground_draws_nothing() {
        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let within = reach_of(AbilityType::Frenzy, 1);
        let cliff = |q, r| Some(Qrz { q, r, z: if (q, r) == (1, 0) { 4 } else { 0 } });
        assert!(!covered(&within, from, Some(&heading), cliff).iter().any(|tile| (tile.q, tile.r) == (1, 0)));
        let gap = |q, r| ((q, r) != (1, 0)).then_some(Qrz { q, r, z: 0 });
        assert!(!covered(&within, from, Some(&heading), gap).iter().any(|tile| (tile.q, tile.r) == (1, 0)));
    }

    #[test]
    fn a_yank_reaches_past_a_strike() {
        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let strike = covered(&reach_of(AbilityType::Overpower, 1), from, None, flat);
        let yank = covered(&reach_of(AbilityType::Yank, 1), from, None, flat);
        assert!(yank.len() > strike.len());
    }
}
