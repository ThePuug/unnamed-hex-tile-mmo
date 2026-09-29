pub mod auto_attack;
pub mod counter;
pub mod deflect;
pub mod disengage;
pub mod flank;
pub mod rattle;
pub mod kick;
pub mod lunge;
pub mod overpower;
pub mod volley;

use bevy::prelude::*;
use common_bevy::{
    components::{
        heading::Heading,
        prepared::Prepared,
        recovery::{get_ability_recovery_duration, Combo, GlobalRecovery, SynergyUnlock},
        resources::Stamina,
        ActorAttributes, Loc,
    },
    message::{AbilityFailReason, AbilityType, Component, Do, Event as GameEvent, Try},
    systems::{combat::synergies, targeting},
};

/// Whether a striker facing `heading` with `attrs` may strike from `from`
/// at `to`: within the arc its Grace opens (`ActorAttributes::arc`).
pub fn in_arc(heading: Option<&Heading>, attrs: Option<&ActorAttributes>, from: &Loc, to: &Loc) -> bool {
    targeting::faces(heading, attrs.map_or(targeting::STRIDE_ARC, ActorAttributes::arc), from, to)
}

/// Prepares `ability`, a reaction `ent` used with nothing to answer: it pays
/// its stamina and lockout now, as any use of it does, and is held in
/// `prepared` to fire free later. Errs where Preparation holds no more, or
/// the stamina falls short.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    ent: Entity,
    ability: AbilityType,
    attrs: &ActorAttributes,
    stamina: &mut Stamina,
    prepared: &mut Prepared,
    prior: Option<GlobalRecovery>,
    offer: Option<SynergyUnlock>,
    combo: Option<&Combo>,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
) -> Result<(), AbilityFailReason> {
    let room = attrs.preparation().index();
    if !Prepared::is_reaction(ability) || prepared.count() >= room {
        return Err(AbilityFailReason::NoTargets);
    }
    let cost = common_bevy::tuning::tuning().cost(ability);
    if stamina.state < cost {
        return Err(AbilityFailReason::InsufficientStamina);
    }
    stamina.state -= cost;
    stamina.step = stamina.state;
    prepared.hold(ability, room);
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Stamina(*stamina) } });
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Prepared(*prepared) } });
    writer.write(Do { event: GameEvent::UseAbility { ent, ability, target: None } });

    let early = synergies::is_early(ability, prior.as_ref(), offer.as_ref());
    let recovery = synergies::lockout(ability, prior.as_ref(), offer.as_ref());
    commands.entity(ent).insert(recovery);
    synergies::apply_synergies(ent, ability, &recovery, attrs, attrs, commands);
    synergies::settle_combo(ent, ability, early, get_ability_recovery_duration(ability), attrs, combo, commands);
    Ok(())
}

/// Spends a prepared `ability` of `ent`'s, telling its client: true where
/// one was held, and the reaction fires free, lockout or none.
pub fn fire_prepared(ent: Entity, ability: AbilityType, prepared: &mut Prepared, writer: &mut MessageWriter<Do>) -> bool {
    if !prepared.take(ability) {
        return false;
    }
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Prepared(*prepared) } });
    true
}

/// Breaks `ent`'s stride where the strike it just made from `from` at `to`
/// crossed its line (`targeting::across`).
pub fn stride(ent: Entity, heading: Option<&Heading>, from: &Loc, to: &Loc, commands: &mut Commands) {
    if targeting::across(heading, from, to) {
        commands.trigger(Try { event: GameEvent::Stumble { ent } });
    }
}
