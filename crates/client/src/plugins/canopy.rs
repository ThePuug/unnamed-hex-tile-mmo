//! The canopy's parts on the GPU. Each level that wears a canopy has one
//! array texture, a layer to each of its regions standing, and a region's
//! layer holds its parts as `summary_mesh::parts_layer` lays them out. A
//! region wears its layer's index as its `MeshTag`, which the terrain
//! shaders read the layer by, so the terrain keeps drawing in the batches
//! it always did. A layer is written straight into the texture as its
//! region is built: the image asset never changes after it is made, so
//! nothing is uploaded whole and no material is prepared again.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureDimension, TextureFormat,
    TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use common_bevy::summary_mesh::{MeshRegionKey, PARTS_LAYER_SIDE};

/// Layers per level: more than a level keeps standing at once, its band
/// widened by the keep margin, and the regions held round an edge that
/// lags the player.
pub const LAYERS: u32 = 768;

/// The tag of a region with no layer; the shaders draw its canopy bare.
pub const NO_LAYER: u32 = u32::MAX;

pub struct CanopyPartsPlugin;

impl Plugin for CanopyPartsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CanopyParts>();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<PendingWrites>()
            .add_systems(ExtractSchedule, extract_writes)
            .add_systems(Render, write_layers.in_set(RenderSystems::PrepareResources));
    }
}

/// Every level's parts texture, and which region holds which layer.
#[derive(Resource)]
pub struct CanopyParts {
    levels: HashMap<u32, Level>,
    /// One bare layer, bound by every level that wears no canopy.
    bare: Handle<Image>,
    /// Layers written since the render world last took them.
    writes: Vec<LayerWrite>,
}

struct Level {
    image: Handle<Image>,
    free: Vec<u32>,
    held: HashMap<MeshRegionKey, u32>,
    /// Whether a region has already gone without a layer, said once.
    short: bool,
}

struct LayerWrite {
    image: AssetId<Image>,
    layer: u32,
    texels: Vec<u16>,
}

/// An array texture of `layers` bare layers, each a region's square of
/// parts, read by layer whatever the count.
fn parts_image(side: u32, layers: u32) -> Image {
    let size = Extent3d { width: side, height: side, depth_or_array_layers: layers };
    let mut image = Image::new_fill(size, TextureDimension::D2, &0u16.to_le_bytes(), TextureFormat::R16Uint, RenderAssetUsages::RENDER_WORLD);
    image.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
    image
}

impl FromWorld for CanopyParts {
    fn from_world(world: &mut World) -> Self {
        let bare = world.resource_mut::<Assets<Image>>().add(parts_image(1, 1));
        Self { levels: HashMap::new(), bare, writes: Vec::new() }
    }
}

impl CanopyParts {
    /// The parts texture level `r`'s material binds: its own where it
    /// wears a canopy, made on first asking, else the bare one.
    pub fn image(&mut self, r: u32, canopied: bool, images: &mut Assets<Image>) -> Handle<Image> {
        if !canopied {
            return self.bare.clone();
        }
        self.levels
            .entry(r)
            .or_insert_with(|| Level {
                image: images.add(parts_image(PARTS_LAYER_SIDE, LAYERS)),
                free: (0..LAYERS).rev().collect(),
                held: HashMap::new(),
                short: false,
            })
            .image
            .clone()
    }

    /// Writes `texels` as region `key`'s layer, claiming one where it holds
    /// none, and answers the tag the region wears: [`NO_LAYER`] where its
    /// level has no texture or every layer is held.
    pub fn write(&mut self, key: MeshRegionKey, texels: Vec<u16>) -> u32 {
        let Some(level) = self.levels.get_mut(&key.r) else { return NO_LAYER };
        let layer = match level.held.get(&key) {
            Some(&layer) => layer,
            None => match level.free.pop() {
                Some(layer) => *level.held.entry(key).or_insert(layer),
                None => {
                    if !level.short {
                        level.short = true;
                        warn!("canopy: level {} holds all {LAYERS} layers; a region draws its canopy bare", key.r);
                    }
                    return NO_LAYER;
                }
            },
        };
        self.writes.push(LayerWrite { image: level.image.id(), layer, texels });
        layer
    }

    /// Frees the layer of every region that no longer stands.
    pub fn release(&mut self, standing: impl Fn(&MeshRegionKey) -> bool) {
        for level in self.levels.values_mut() {
            level.held.retain(|key, &mut layer| {
                let keep = standing(key);
                if !keep {
                    level.free.push(layer);
                }
                keep
            });
        }
    }
}

/// Layers the render world has taken and not yet written: a texture is
/// only there once its image is prepared, a frame or more after it is made.
#[derive(Resource, Default)]
struct PendingWrites(Vec<LayerWrite>);

fn extract_writes(mut main: ResMut<MainWorld>, mut pending: ResMut<PendingWrites>) {
    let mut parts = main.resource_mut::<CanopyParts>();
    pending.0.append(&mut parts.writes);
}

/// Writes each taken layer into its texture, a queue write a region built,
/// in the order they were made, so a region built twice keeps the later.
fn write_layers(mut pending: ResMut<PendingWrites>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let mut waiting = Vec::new();
    for write in pending.0.drain(..) {
        let Some(image) = images.get(write.image) else {
            waiting.push(write);
            continue;
        };
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: Origin3d { x: 0, y: 0, z: write.layer },
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(&write.texels),
            TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(PARTS_LAYER_SIDE * 2), rows_per_image: Some(PARTS_LAYER_SIDE) },
            Extent3d { width: PARTS_LAYER_SIDE, height: PARTS_LAYER_SIDE, depth_or_array_layers: 1 },
        );
    }
    pending.0 = waiting;
}
