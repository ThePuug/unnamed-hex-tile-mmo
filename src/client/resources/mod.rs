use std::collections::HashMap;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use bimap::BiMap;

use crate::common::systems::gcd::GcdType;

#[derive(Debug, Default, Resource)]
pub struct EntityMap(pub BiMap<Entity,Entity>);

#[derive(Resource)]
pub struct TextureHandles {
    pub actor: (Handle<Image>, Handle<TextureAtlasLayout>),
    pub decorator: (Handle<Image>, Handle<TextureAtlasLayout>),
}

#[derive(Debug, Default, Resource)]
pub struct EffectMap(pub HashMap<GcdType, Handle<EffectAsset>>);
