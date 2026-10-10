//! Client-specific targeting systems

//! This module contains targeting system implementations that are specific to the client.

use bevy::prelude::*;

use common_bevy::{
    components::{
        ally_target::AllyTarget,
        behaviour::Side,
        heading::Heading,
        Loc,
        target::Target,
        ActorAttributes,
    },
    plugins::nntree::NNTree,
    systems::targeting::{arc_of, update_targets_impl},
};
use common_bevy::tuning::Tuning;

/// Selects every actor's hostile `Target` each frame, so a target that
/// moves out of reach or arc is dropped at once, not only when the actor
/// itself turns or moves.
pub fn update_targets(
    tuning: Res<Tuning>,
    mut query: Query<(Entity, &Loc, &Heading, &mut Target, Option<&ActorAttributes>)>,
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

/// Selects every actor's `AllyTarget` each frame as `update_targets` does
/// its hostile one, wanting the actors on its own side. UI systems read
/// the component and never select for themselves.
pub fn update_ally_targets(
    tuning: Res<Tuning>,
    mut query: Query<(Entity, &Loc, &Heading, &mut AllyTarget, Option<&ActorAttributes>)>,
    sides: Query<&Side>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut ally_target, attrs) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            arc_of(&tuning, attrs),
            &mut *ally_target,
            &nntree,
            |e| sides.get(e).ok().copied(),
            |own, side| side == own,
        );
    }
}
