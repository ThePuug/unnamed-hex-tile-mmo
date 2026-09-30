//! Client-specific targeting systems

//! This module contains targeting system implementations that are specific to the client.
//! TierLock is replicated from server, so both client and server have it.

use bevy::prelude::*;

use common_bevy::{
    components::{
        ally_target::AllyTarget,
        behaviour::Side,
        heading::Heading,
        Loc,
        target::Target,
        tier_lock::TierLock,
    },
    plugins::nntree::NNTree,
    systems::targeting::{select_target, update_targets_impl},
};

/// Update hostile targets every frame for responsive targeting (CLIENT VERSION)

/// Runs unconditionally to detect when target entities move out of range/cone.
/// This ensures targets update immediately when NPCs/players move, not just when
/// the local player changes heading/location.

/// # Performance

/// Uses spatial index (NNTree) for fast proximity queries. Designed to run at 60fps
/// alongside target indicator. If performance becomes an issue, can be changed to
/// run on a timer (e.g., every 100ms).
pub fn update_targets(
    mut query: Query<(Entity, &Loc, &Heading, &mut Target, Option<&TierLock>)>,
    sides: Query<&Side>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut target, tier_lock) in &mut query {
        update_targets_impl(
            ent,
            *loc,
            *heading,
            &mut target,
            tier_lock,
            &nntree,
            |e| sides.get(e).ok().copied(),
        );
    }
}

/// Update ally targets every frame for responsive targeting (CLIENT VERSION)

/// Runs unconditionally to detect when ally entities move out of range/cone.
/// This ensures targets update immediately when allies move, not just when
/// the local player changes heading/location.

/// # Architecture

/// This system mirrors update_targets() but for ally targeting: the same
/// selection, wanting the actors on the entity's own side. UI systems read
/// the AllyTarget component and never select for themselves.

/// # Performance

/// Uses spatial index (NNTree) for fast proximity queries. Designed to run at 60fps
/// alongside target indicator. If performance becomes an issue, can be changed to
/// run on a timer (e.g., every 100ms).
pub fn update_ally_targets(
    mut query: Query<(Entity, &Loc, &Heading, &mut AllyTarget, Option<&TierLock>)>,
    sides: Query<&Side>,
    nntree: Res<NNTree>,
) {
    for (ent, loc, heading, mut ally_target, tier_lock) in &mut query {
        // The ally this entity faces, within its tier lock if it holds one
        let new_ally_target = sides.get(ent).ok().and_then(|own| {
            select_target(ent, *loc, *heading, tier_lock.and_then(|tl| tl.get()), &nntree, |other| {
                sides.get(other).is_ok_and(|side| side == own)
            })
        });

        // Update AllyTarget fields directly
        match new_ally_target {
            Some(target_ent) => {
                // Ally found - update both entity and last_target
                ally_target.entity = Some(target_ent);
                ally_target.last_target = Some(target_ent);
            }
            None => {
                // No ally found - clear entity but leave last_target intact for sticky UI
                ally_target.entity = None;
            }
        }
    }
}
