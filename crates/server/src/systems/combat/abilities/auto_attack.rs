use bevy::prelude::*;
use common_bevy::{
    components::status::Status,
    message::AbilityType,
};

use super::{disengage::Poised, Abilities, AbilityFailReason, Cast};

/// An auto-attack: a blow of the caster's auto damage on its target, free
/// and outside the lockout, due on its own cadence (the gate's to check).
/// The swings that came due since the last while it could not strike, up to
/// its Patience, land with this one. A caster poised by a Disengage strikes
/// harder by what it was poised with, behind a feint: a damage-free ability
/// threat, which queues ahead of the blow and draws the reaction.
pub fn swing(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let (target, _) = cast.struck()?;
    let poised = abilities.poised.get(cast.ent).map_or(0.0, |poised| poised.0);
    if poised > 0.0 {
        abilities.commands.entity(cast.ent).remove::<Poised>();
        abilities.deal(cast.ent, target, 0.0, AbilityType::Disengage, 0.0);
    }
    let now = abilities.time.elapsed();
    let status = abilities.statuses.get(cast.ent).ok().copied();
    let interval = Status::cadence(cast.attrs.cadence_interval(), status.as_ref());
    let banked = abilities.swings.get_mut(cast.ent).map_or(0, |mut swing| {
        let banked = swing.at.map_or(0, |at| cast.attrs.banked(now.saturating_sub(at), interval));
        swing.at = Some(now);
        banked
    });
    abilities.deal(cast.ent, target, cast.attrs.auto_damage() * (1 + banked) as f32 + poised, AbilityType::AutoAttack, 0.0);
    Ok(Some(target))
}
