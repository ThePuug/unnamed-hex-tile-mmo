use common_bevy::{components::ActorAttributes, message::AbilityType, tuning::Tuning};

use super::{Abilities, Blow, Cast};

/// A skill that is one strike on a target within the caster's reach, for
/// its [`share`] of base potency, with what is its own:
///
/// - Frenzy, Might's: quick to return.
/// - Feint, of no line: a light blow that interrupts.
/// - Overpower, Physique's: one heavy blow, slow to land.
/// - Backstab, Agility's: a heavy blow that lands only from past its
///   target's forward faces (`Abilities::resolve`).
/// - Kick, Discipline's: knocks its target back as it lands, up to
///   `Tuning::kick_tiles` along the line from the caster as its line has
///   it, over the ground and inside the target's leash
///   (`Abilities::drag`).
/// - Punish, Instinct's: harder on a target mid-activation.
/// - Defy, Resolve's: harder the more of its user's health is lost.
pub fn strike(abilities: &mut Abilities, cast: &Cast) {
    let tuning = *abilities.tuning;
    let Ok((target, _)) = cast.struck() else { return };
    let caught = abilities.activation_of(target).is_some();
    let lost = abilities.health.get(cast.ent).map_or(0.0, |health| 1.0 - health.state / health.max.max(f32::EPSILON));
    let drag = match cast.ability {
        AbilityType::Kick => -(cast.attrs.line_tiles(&tuning, AbilityType::Kick, tuning.kick_tiles).min(i8::MAX as usize) as i8),
        _ => 0,
    };
    let blow = Blow {
        damage: cast.attrs.base_potency(&tuning) * share(&tuning, &cast.attrs, cast.ability, caught, lost),
        drag,
        interrupts: cast.ability.interrupts(),
        ..Blow::default()
    };
    abilities.attack(cast.ent, target, cast.ability, blow);
}

/// The share of base potency a strike of `ability` by `attrs` lands for:
/// its own share (`Tuning::damage`) as its line has it
/// (`ActorAttributes::line_power`), whole for a Feint and a Kick, which
/// strike alike in any build. A Punish strikes `Tuning::punish_base` in any
/// build and `Tuning::punish_caught` more, as its line has it, on a target
/// `caught` mid-activation; a Defy `Tuning::defy_damage` in any build and
/// up to `Tuning::defy_low` more, as its line has it, by the share of its
/// user's health `lost`.
pub fn share(tuning: &Tuning, attrs: &ActorAttributes, ability: AbilityType, caught: bool, lost: f32) -> f32 {
    let line = attrs.line_power(tuning, ability);
    match ability {
        AbilityType::Punish => tuning.punish_base + if caught { tuning.punish_caught * line } else { 0.0 },
        AbilityType::Defy => tuning.defy_damage + tuning.defy_low * line * lost.clamp(0.0, 1.0),
        AbilityType::Kick | AbilityType::Feint => tuning.damage(ability),
        ability => tuning.damage(ability) * line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_punish_strikes_its_base_in_any_build_and_its_line_raises_only_what_catching_adds() {
        let tuning = Tuning::DEFAULT;
        let instinctive = ActorAttributes::new(0, 0, 0, 0, 0, 0, -25, 0, 0);
        let plain = ActorAttributes::default();
        let punish = |attrs: &ActorAttributes, caught| share(&tuning, attrs, AbilityType::Punish, caught, 0.0);
        assert_eq!(punish(&instinctive, false), punish(&plain, false), "on a target at rest, the same in any build");
        assert!(punish(&plain, true) > punish(&plain, false), "catching one mid-activation adds something in any build");
        assert!(punish(&instinctive, true) > punish(&plain, true), "and Instinct raises what it adds");
    }

    #[test]
    fn a_defy_strikes_harder_the_more_health_its_user_has_lost() {
        let tuning = Tuning::DEFAULT;
        let resolute = ActorAttributes::new(0, 0, 0, 0, 0, 0, 25, 0, 0);
        let defy = |attrs: &ActorAttributes, lost| share(&tuning, attrs, AbilityType::Defy, false, lost);
        assert_eq!(defy(&resolute, 0.0), defy(&ActorAttributes::default(), 0.0), "at full health, the same in any build");
        assert!(defy(&resolute, 0.5) > defy(&resolute, 0.0) && defy(&resolute, 1.0) > defy(&resolute, 0.5), "more for more lost");
        assert!(defy(&resolute, 0.5) > defy(&ActorAttributes::default(), 0.5), "and Resolve raises what it adds");
    }
}
