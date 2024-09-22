pub mod map;

use std::collections::HashMap;

use bevy::prelude::*;
use renet::ClientId;

#[derive(Default, Resource)]
pub struct Lobby(pub HashMap<ClientId, Entity>);
