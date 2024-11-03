use bevy::prelude::*;
use bimap::BiMap;

#[derive(Debug, Default, Resource)]
pub struct EntityMap(pub BiMap<Entity,Entity>);

#[derive(Resource)]
pub struct TextureHandles {
    pub actor: (Handle<Image>, Handle<TextureAtlasLayout>),
    pub decorator: (Handle<Image>, Handle<TextureAtlasLayout>),
}
