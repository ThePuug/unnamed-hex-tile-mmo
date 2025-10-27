pub mod behaviour;
pub mod entity_type;
pub mod heading;
pub mod keybits;
pub mod offset;
pub mod spawner;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }
}

/// Destination for pathfinding - the Qrz location an entity is trying to reach
#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dest(pub Qrz);

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)]
pub struct Actor;

/// Attributes for actor entities that affect gameplay mechanics
#[derive(Clone, Component, Copy, Debug)]
pub struct ActorAttributes {
    /// Movement speed in world units per millisecond
    /// Default: 0.005 (matches physics MOVEMENT_SPEED constant)
    pub movement_speed: f32,
    // Future attributes can be added here:
    // pub jump_height: f32,
    // pub max_health: f32,
    // pub stamina: f32,
}

impl Default for ActorAttributes {
    fn default() -> Self {
        Self {
            movement_speed: 0.005, // Default physics movement speed
        }
    }
}

#[derive(Clone, Component, Copy, Default)]
pub struct Physics;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();