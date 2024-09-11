use std::collections::HashMap;

use bevy::prelude::*;
use renet::ClientId;

#[derive(Default, Resource)]
pub struct Lobby {
    pub clients: HashMap<ClientId, Entity>,
}