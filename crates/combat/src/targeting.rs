//! Who each actor on the server targets.

use bevy::prelude::*;

use common_bevy::{
    components::{behaviour::Side, heading::Heading, status::Status, AttackRange, Loc, target::Target},
    plugins::nntree::NNTree,
    systems::targeting::{unseen, update_targets_impl},
};
use crate::behaviour::chase::Chase;

/// Points every actor that picks its target by facing at the hostile it
/// faces, each frame, so a target moving out of its arc is let go at once.
/// An NPC does not pick by facing: its `Chase` takes a target and keeps it,
/// and is left alone here.
pub fn update_targets(
    mut query: Query<(Entity, &Loc, &Heading, &mut Target), Without<Chase>>,
    sides: Query<&Side>,
    others: Query<(&Loc, Option<&Status>, Option<&AttackRange>)>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target) in &mut query {
        let seen = |other: Entity| others.get(other).is_ok_and(|(at, status, range)| {
            !unseen(status, range.copied().unwrap_or_default().0, loc.flat_distance(at))
        });
        update_targets_impl(
            ent,
            *loc,
            *heading,
            &mut *target,
            &nntree,
            |e| sides.get(e).ok().copied().filter(|_| e == ent || seen(e)),
            Side::is_hostile_to,
        );
    }
}
