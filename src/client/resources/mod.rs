use bevy::prelude::*;
use bimap::BiMap;

use crate::common::components::keybits::*;

#[derive(Debug, Default, Resource)]
pub struct EntityMap(pub BiMap<Entity,Entity>);

#[derive(Resource)]
pub struct TextureHandles {
    pub actor: (Handle<Image>, Handle<TextureAtlasLayout>),
    pub decorator: (Handle<Image>, Handle<TextureAtlasLayout>),
}

#[derive(Debug, Default)]
pub struct InputAccumulator {
    pub key_bits: KeyBits,
    pub dt: u16,
}

#[derive(Debug, Default, Resource)]
pub struct InputQueue(pub Vec<InputAccumulator>);