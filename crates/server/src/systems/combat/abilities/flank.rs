use bevy::prelude::*;
use common_bevy::{
    components::{heading::Heading, hex_assignment::AssignedHex, AttackRange, Loc, Turn},
    message::AbilityType,
};

use super::{Abilities, AbilityFailReason, Cast};
use crate::systems::combat::leap;

/// How long the circle to the target's back takes, however far round it
/// goes: the Ambusher's clip scuttles for this long before it strikes.
pub const FLANK_CIRCLE_MS: u16 = 1500;

/// Flank, the Ambusher's signature: on a target within melee reach, a
/// circle round it on its ring to the tile at its back at reach, turned to
/// face it so its auto-attacks carry on from there, and a strike for
/// `flank_endurance` of the caster's Endurance that stuns for
/// `Tuning::flank_stun` seconds as it lands (`landing::land`).
/// The strike waits in the target's queue like any threat, so the target
/// has its window to answer it before the stun holds it: the hold
/// (`Status::hold`) stops its movement and auto-attacks, and a recovery as
/// long its abilities and reactions.
/// The back tile becomes the Ambusher's assigned tile, so it holds the flank;
/// an engagement member assigned there takes the tile the Ambusher left.
/// With its back tile taken or not standable, the Ambusher strikes from
/// where it stands.
pub fn strike(abilities: &mut Abilities, cast: &Cast) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = common_bevy::tuning::tuning();
    let (target, target_loc) = cast.struck()?;
    let ent = cast.ent;

    // The tile at the target's back at reach, a place on the ring the
    // assignment stands melee on, taken when its floor is there and no one
    // stands on it
    let target_heading = abilities.actors.get(target).ok().and_then(|(_, _, _, heading, ..)| heading.copied());
    let back = target_heading.map(|heading| *target_loc + heading.reversed().hex_dir() * AttackRange::default().0);
    let landing = back.and_then(|back| abilities.map.get_by_qr(back.q, back.r)).map(|(floor, _)| floor + qrz::Qrz::Z)
        .filter(|landing| *landing != *cast.loc && abilities.nntree.locate_all_at_point(&Loc::new(*landing)).next().is_none());
    if let Some(landing) = landing {
        // Round the target on its ring while the strike waits in its queue,
        // arriving turned to it so its auto-attacks carry on from its back
        leap::slide(ent, landing, FLANK_CIRCLE_MS, Some(*target_loc), &mut abilities.commands, &mut abilities.writer);
        if let Some(facing) = Heading::between(&abilities.map, landing, *target_loc) {
            abilities.commands.entity(ent).insert((facing, Turn { heading: facing, ..Turn::default() }));
        }
        let left = abilities.assigned.get(ent).ok().map(|assigned| assigned.0);
        let holder = abilities.members.get(ent).ok()
            .and_then(|member| abilities.engagements.get(member.0).ok())
            .and_then(|engagement| engagement.spawned_npcs.iter().copied()
                .find(|&npc| npc != ent && abilities.assigned.get(npc).is_ok_and(|assigned| assigned.0 == landing)));
        // The fallen keep their place until they are despawned, so the
        // holder may be gone by the time the command lands
        if let Some(holder) = holder {
            match left {
                Some(left) => abilities.commands.entity(holder).try_insert(AssignedHex(left)),
                None => abilities.commands.entity(holder).try_remove::<AssignedHex>(),
            };
        }
        abilities.commands.entity(ent).insert(AssignedHex(landing));
    }

    abilities.deal(ent, target, cast.attrs.endurance() * tuning.flank_endurance, AbilityType::Flank, 0.0);
    Ok(Some(target))
}
