pub mod engagement_budget;
pub mod terrain;

use bevy::prelude::*;
use renet::ClientId;
use bimap::BiMap;

#[derive(Default, Deref, DerefMut, Resource)]
pub struct Lobby(BiMap<ClientId, Entity>);

#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
}
