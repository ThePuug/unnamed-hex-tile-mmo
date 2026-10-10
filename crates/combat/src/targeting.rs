//! Who each actor on the server targets.

use bevy::prelude::*;

use common_bevy::{
    components::{behaviour::Side, heading::Heading, ActorAttributes, Loc, target::Target},
    plugins::nntree::NNTree,
    systems::targeting::{arc_of, update_targets_impl},
};
use crate::behaviour::chase::Chase;
use common_bevy::tuning::Tuning;

/// Points every actor that picks its target by facing at the hostile it
/// faces, each frame, so a target moving out of its arc is let go at once.
/// An NPC does not pick by facing: its `Chase` takes a target and keeps it,
/// and is left alone here.
pub fn update_targets(
    tuning: Res<Tuning>,
    mut query: Query<
        (Entity, &Loc, &Heading, &mut Target, Option<&ActorAttributes>),
        Without<Chase>
    >,
    sides: Query<&Side>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target, attrs) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            arc_of(&tuning, attrs),
            &mut *target,
            &nntree,
            |e| sides.get(e).ok().copied(),
            Side::is_hostile_to,
        );
    }
}
