pub mod event_registry;
pub mod summary_cache;

use bevy::prelude::*;
use renet::ClientId;
use bimap::BiMap;

#[derive(Default, Deref, DerefMut, Resource)]
pub struct Lobby(BiMap<ClientId, Entity>);

#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
}

/// Every roll combat makes — damage, crits, the foe an NPC takes up, its
/// stray — drawn from one stream, so a world seeded alike plays a fight
/// alike. The live server seeds it from the OS.
#[derive(Resource, Deref, DerefMut)]
pub struct Dice(rand::rngs::StdRng);

impl Default for Dice {
    fn default() -> Self {
        Self(rand::SeedableRng::from_os_rng())
    }
}

impl Dice {
    pub fn seeded(seed: u64) -> Self {
        Self(rand::SeedableRng::seed_from_u64(seed))
    }
}
