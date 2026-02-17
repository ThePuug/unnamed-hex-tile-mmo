use bevy::prelude::*;
use std::collections::HashSet;

/// Tracks which players have this entity loaded on their client.
///
/// Managed by the AOI (Area of Interest) system. When a player enters/exits
/// range, they are added/removed from this set and sent Spawn/Despawn events.
/// Used by send_do to route all gameplay events to the correct clients.
#[derive(Clone, Component, Debug, Default)]
pub struct LoadedBy {
    pub players: HashSet<Entity>,
}
