use common_bevy::{components::status::Block, message::AbilityType};

use super::{Abilities, Cast};
use crate::landing;

/// A skill that blocks: for `Tuning::block_secs` the next attack on its
/// user, a strike or an auto-attack, is stopped whole (`Status::stop`,
/// `Abilities::attack`).
///
/// - Parry, of no line: stops it and no more.
/// - Clash, Might's: its striker is stunned `Tuning::clash_stun` seconds
///   as its line has it (`ActorAttributes::line_power`).
/// - Counter, Resolve's: its striker is struck at once for
///   `Tuning::counter_reflect` of the blow, as its line has it.
pub fn take(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let line = cast.attrs.line_power(&tuning, cast.ability);
    let power = match cast.ability {
        AbilityType::Clash => tuning.clash_stun * line,
        AbilityType::Counter => tuning.counter_reflect * line,
        _ => 0.0,
    };
    let block = Block { ability: cast.ability, attacks: 1, remaining: tuning.block_secs, power };
    landing::update(cast.ent, &mut abilities.statuses, &mut abilities.commands, &mut abilities.writer, |status| {
        status.block = Some(block);
    });
}
