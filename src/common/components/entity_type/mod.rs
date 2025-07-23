pub mod actor;
pub mod decorator;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::entity_type::{
    actor::*,
    decorator::*,
};

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EntityType {
    Actor(ActorImpl),
    Decorator(DecoratorImpl),
}