//! Where each melee NPC of an engagement stands round its target.

use bevy::prelude::*;
use qrz::Qrz;

/// What an engagement's places were last assigned for: the target its NPCs
/// chase, the tile it stood on, and how many of them lived. The assignment
/// runs afresh when any of the three changes, and not otherwise, so a place
/// taken between assignments stands until then.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct HexAssignment {
    pub last_player_tile: Option<Qrz>,
    pub target_player: Option<Entity>,
    pub living: usize,
}

/// The hex an NPC is assigned to stand on, the one record of it: the
/// assignment and a Flank write it, chase walks to it.
#[derive(Clone, Component, Copy, Debug)]
pub struct AssignedHex(pub Qrz);
