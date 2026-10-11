use bevy::prelude::*;
use common_bevy::{components::{status::{Status, Timed}, Loc}, message::AbilityType};
use qrz::Qrz;

use super::{Abilities, AbilityFailReason, Cast};
use crate::landing;

/// What a Surge's user has moved since it began: the tiles, and where it
/// last stood. The server's alone; the status says a Surge holds.
#[derive(Clone, Component, Copy, Debug)]
pub struct Surged {
    pub tiles: u8,
    pub last: Qrz,
}

/// Surge, Discipline's stance: for `Tuning::surge_secs`, each tile its user
/// moves, walking or carried by a skill, adds `Tuning::surge_per_tile` as its
/// Discipline line has it (`ActorAttributes::line_power`) to its next skill
/// strike, which spends it (`process_deal_damage`). Short, so what it
/// builds is the moves made just before the strike.
pub fn take(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let surge = Timed { pace: tuning.surge_per_tile * cast.attrs.line_power(&tuning, AbilityType::Surge), remaining: tuning.surge_secs };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.surge = Some(surge);
    });
    abilities.commands.entity(cast.ent).try_insert(Surged { tiles: 0, last: *cast.loc });
    Ok(None)
}

/// Counts the tiles each surging actor moves as it moves them.
pub fn count(mut query: Query<(&Loc, &Status, &mut Surged), Changed<Loc>>) {
    for (&loc, status, mut surged) in &mut query {
        if status.surging() <= 0.0 {
            continue;
        }
        let moved = surged.last.flat_distance(&loc).max(0) as u32;
        surged.tiles = (surged.tiles as u32 + moved).min(u8::MAX as u32) as u8;
        surged.last = *loc;
    }
}
