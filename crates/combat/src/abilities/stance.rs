use bevy::prelude::*;
use common_bevy::{
    components::{status::{Stance, Status, Timed}, ActorAttributes, Loc},
    message::AbilityType,
    tuning::Tuning,
};
use qrz::Qrz;

use super::{Abilities, Cast};
use crate::landing;

/// What a Surge's user has moved since it began: the tiles, and where it
/// last stood. The server's alone; the status says a Surge holds.
#[derive(Clone, Component, Copy, Debug)]
pub struct Surged {
    pub tiles: u8,
    pub last: Qrz,
}

/// A stance: a while of something on its user ([`timed`]), taken at once
/// and ending the stance it held (`Status::take`).
pub fn take(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Some(stance) = Stance::of(cast.ability) else { return };
    let timed = timed(&tuning, &cast.attrs, stance);
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.take(stance, timed);
    });
    if stance == Stance::Surge {
        abilities.commands.entity(cast.ent).try_insert(Surged { tiles: 0, last: *cast.loc });
    }
}

/// `stance` as an actor with `attrs` takes it, what it gives and how long
/// it lasts, each as its line has it (`ActorAttributes::line_power`):
///
/// - Perfect Stride, Agility's: runs `Tuning::stride_speed` faster.
/// - Bloodlust, Might's: `Tuning::bloodlust_share` of the damage its
///   attacks land comes back as health (`Abilities::land`).
/// - Brace, Physique's: no hold, slow, root or pin takes and no drag moves
///   it (`Status::is_braced`); it lifts none already on it.
/// - Surge, Discipline's: each tile it moves, walking or carried by a
///   skill, adds `Tuning::surge_per_tile` to its next skill strike, which
///   spends it (`Abilities::attack`).
/// - Vigil, Resolve's: every attack from outside its arc is blocked
///   (`Abilities::attack`).
/// - Lurk, Instinct's: no foe beyond its reach sees it
///   (`targeting::unseen`); its first attack ends it, a skill landing
///   `Tuning::lurk_ambush` harder (`Abilities::attack`).
pub fn timed(tuning: &Tuning, attrs: &ActorAttributes, stance: Stance) -> Timed {
    let line = |ability| attrs.line_power(tuning, ability);
    match stance {
        Stance::PerfectStride => Timed { pace: 1.0 + tuning.stride_speed * line(AbilityType::PerfectStride), remaining: tuning.stride_secs },
        Stance::Bloodlust => Timed { pace: tuning.bloodlust_share * line(AbilityType::Bloodlust), remaining: tuning.bloodlust_secs },
        Stance::Brace => Timed { pace: 1.0, remaining: tuning.brace_secs * line(AbilityType::Brace) },
        Stance::Surge => Timed { pace: tuning.surge_per_tile * line(AbilityType::Surge), remaining: tuning.surge_secs },
        Stance::Vigil => Timed { pace: 1.0, remaining: tuning.vigil_secs * line(AbilityType::Vigil) },
        Stance::Lurk => Timed { pace: tuning.lurk_ambush * line(AbilityType::Lurk), remaining: tuning.lurk_secs * line(AbilityType::Lurk) },
    }
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
