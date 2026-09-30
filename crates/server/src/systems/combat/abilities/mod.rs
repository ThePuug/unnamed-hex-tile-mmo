pub mod auto_attack;
pub mod counter;
pub mod disengage;
pub mod flank;
pub mod rattle;
pub mod kick;
pub mod lunge;
pub mod overpower;
pub mod volley;

use bevy::prelude::*;
use common_bevy::{
    components::{heading::Heading, ActorAttributes, Loc},
    message::{Event as GameEvent, Try},
    systems::targeting,
};

/// Whether a striker facing `heading` with `attrs` may strike from `from`
/// at `to`: within the arc its Grace opens (`ActorAttributes::arc`).
pub fn in_arc(heading: Option<&Heading>, attrs: Option<&ActorAttributes>, from: &Loc, to: &Loc) -> bool {
    targeting::faces(heading, attrs.map_or(targeting::STRIDE_ARC, ActorAttributes::arc), from, to)
}

/// Breaks `ent`'s stride where the strike it just made from `from` at `to`
/// crossed its line (`targeting::across`).
pub fn stride(ent: Entity, heading: Option<&Heading>, from: &Loc, to: &Loc, commands: &mut Commands) {
    if targeting::across(heading, from, to) {
        commands.trigger(Try { event: GameEvent::Stumble { ent } });
    }
}
