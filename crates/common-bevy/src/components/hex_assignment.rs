//! Where each melee NPC of an engagement stands round its target.

use bevy::prelude::*;
use qrz::Qrz;

/// What an engagement's places were last assigned for: the target its NPCs
/// chase and the tile it stood on. The assignment runs afresh when either
/// changes.
#[derive(Component, Debug, Clone, Default)]
pub struct HexAssignment {
    pub last_player_tile: Option<Qrz>,
    pub target_player: Option<Entity>,
}

/// The hex an NPC is assigned to stand on, the one record of it: the
/// assignment and a Flank write it, chase walks to it.
#[derive(Clone, Component, Copy, Debug)]
pub struct AssignedHex(pub Qrz);
