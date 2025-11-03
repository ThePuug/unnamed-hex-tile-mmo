use bevy::prelude::*;

/// Marker component indicating an NPC is returning to its spawn point
/// after its leash broke. This prevents re-acquiring targets during the
/// return journey, even if the NPC moves back within leash range.
#[derive(Component, Clone, Copy, Debug)]
pub struct Returning;
