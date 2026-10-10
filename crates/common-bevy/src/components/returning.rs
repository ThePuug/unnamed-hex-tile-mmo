use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component indicating an NPC is returning to its spawn point
/// after its leash broke. This prevents re-acquiring targets during the
/// return journey, even if the NPC moves back within leash range.

/// While present the NPC regains health at
/// `systems::combat::resources::RETURNING_HEALTH_REGEN` a second
/// (`regenerate_resources`), on the server and on each client alike.
#[derive(Component, Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Returning;
