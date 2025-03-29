use bevy::{
    prelude::*, 
    render::render_resource::*,
};

static SHADER_PATH: &str = "shaders/grass.wgsl";

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct Grass {}

impl Material for Grass {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}
