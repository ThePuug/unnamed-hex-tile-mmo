use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component indicating an NPC is returning to its spawn point
/// after its leash broke. This prevents re-acquiring targets during the
/// return journey, even if the NPC moves back within leash range.
///
/// When present, NPCs regenerate health at 100 HP/sec (server) with client-side prediction.
#[derive(Component, Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Returning;
