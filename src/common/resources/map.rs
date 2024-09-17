use std::collections::HashMap;

use bevy::prelude::*;

use crate::common::hxpx::Hx;

#[derive(Default, Resource)]
pub struct Map(pub HashMap<Hx, Entity>);