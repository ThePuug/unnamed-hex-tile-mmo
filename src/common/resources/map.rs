use std::collections::HashMap;

use bevy::prelude::*;

use crate::common::hx::Hx;

#[derive(Default, Resource)]
pub struct Map(pub HashMap<Hx, Entity>);