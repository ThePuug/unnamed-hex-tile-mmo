use bevy::prelude::*;
use bimap::BiMap;

#[derive(Debug, Default, Resource)]
pub struct EntityMap(pub BiMap<Entity,Entity>);

// #[derive(Debug, Default, Resource)]
// pub struct EffectMap(pub HashMap<GcdType, Handle<EffectAsset>>);
