//! The closeup: the player as the world draws it, dressed as the server
//! says, idling on a stage of its own, drawn into the equipment tab.
//!
//! The stage is a second scene of the player's model on a render layer of
//! its own, far under the world, with its own light and a camera drawing
//! into a texture the panel shows. The figure carries the player's
//! Equipment, so the one dressing pipeline dresses it.

use bevy::{
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::TextureFormat,
};
use std::f32::consts::PI;

use common_bevy::{
    components::{entity_type::EntityType, equipment::Equipment, Actor},
    resources::map::Map,
};

use crate::systems::{
    actor,
    character_panel::{CharacterPanelState, PanelTab},
    equipment_panel::CloseupView,
};

/// The stage's layer, seen by its own camera and lit by its own light.
const LAYER: usize = 1;
/// Where the stage stands: far under the world, out of every view.
const STAGE: Vec3 = Vec3::new(0.0, -2000.0, 0.0);
/// The texture's size, twice the view's so the figure stays crisp.
const WIDTH: u32 = 560;
const HEIGHT: u32 = 720;
/// The figure's height in its own units, which the frame is fitted to.
const FIGURE_HEIGHT: f32 = 2.0;
/// One turn of the closeup, the world camera's own step.
const TURN: f32 = PI / 3.0;
const TURN_SPEED: f32 = 12.0;
const FOV: f32 = 25.0 * PI / 180.0;

/// The stage's figure: a copy of the player's model.
#[derive(Component)]
pub struct Figure;

#[derive(Component)]
pub struct CloseupCamera;

/// The figure's facing: the stop it turns toward and where it is.
#[derive(Default, Resource)]
pub struct Turn {
    target: f32,
    current: f32,
}

/// Spawns the stage's camera and light and gives the panel's view the
/// texture the camera draws into, once the view exists.
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    map: Res<Map>,
    view: Query<Entity, Added<CloseupView>>,
) {
    let Ok(view) = view.single() else { return };
    let target = images.add(Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None));
    commands.entity(view).insert(ImageNode::new(target.clone()));

    // The figure fills the frame's height with a little over it and under.
    let r = map.radius();
    let height = FIGURE_HEIGHT * r;
    let distance = 0.6 * height / (FOV / 2.0).tan();
    let eye = STAGE + Vec3::new(0.0, height * 0.52, distance);
    let at = STAGE + Vec3::new(0.0, height * 0.5, 0.0);
    commands.spawn((
        CloseupCamera,
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgba(0.05, 0.05, 0.05, 1.0)),
            is_active: false,
            ..default()
        },
        RenderTarget::from(target),
        Projection::from(PerspectiveProjection { fov: FOV, near: 0.1, far: 100.0 * r, ..default() }),
        Transform::from_translation(eye).looking_at(at, Vec3::Y),
        RenderLayers::layer(LAYER),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, shadows_enabled: false, ..default() },
        Transform::from_translation(STAGE + Vec3::new(2.0, 4.0, 3.0) * r).looking_at(at, Vec3::Y),
        RenderLayers::layer(LAYER),
    ));
}

/// Puts the player's figure on the stage once the player exists, wearing
/// what the player wears.
pub fn spawn_figure(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    map: Res<Map>,
    player: Query<(&EntityType, &Equipment), With<Actor>>,
    figure: Query<(), With<Figure>>,
) {
    if !figure.is_empty() {
        return;
    }
    let Ok((&typ, &equipment)) = player.single() else { return };
    commands
        .spawn((
            Figure,
            typ,
            equipment,
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(actor::get_asset(typ)))),
            Transform {
                translation: STAGE,
                scale: Vec3::splat(map.radius()),
                ..default()
            },
            RenderLayers::layer(LAYER),
        ))
        .observe(actor::ready);
}

/// Keeps the figure wearing what the player wears.
pub fn sync_figure(
    player: Query<&Equipment, (With<Actor>, Changed<Equipment>)>,
    mut figure: Query<&mut Equipment, (With<Figure>, Without<Actor>)>,
) {
    let (Ok(worn), Ok(mut figure)) = (player.single(), figure.single_mut()) else { return };
    if *figure != *worn {
        *figure = *worn;
    }
}

/// Puts every mesh that appears under the figure on the stage's layer:
/// the layer is not inherited, and pieces arrive as they are worn.
pub fn stage_layers(
    mut commands: Commands,
    meshes: Query<Entity, Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    figures: Query<(), With<Figure>>,
) {
    for mesh in &meshes {
        let on_stage = parents.iter_ancestors(mesh).any(|a| figures.contains(a));
        if on_stage {
            commands.entity(mesh).insert(RenderLayers::layer(LAYER));
        }
    }
}

/// The camera draws only while the equipment tab shows.
pub fn activate(
    state: Res<CharacterPanelState>,
    mut camera: Query<&mut Camera, With<CloseupCamera>>,
) {
    if !state.is_changed() {
        return;
    }
    let Ok(mut camera) = camera.single_mut() else { return };
    camera.is_active = state.visible && state.tab == PanelTab::Equipment;
}

/// Left and right turn the figure a stop at a time, as they orbit the
/// camera in the world, and it settles toward the stop.
pub fn turn(
    keyboard: Res<ButtonInput<KeyCode>>,
    state: Res<CharacterPanelState>,
    time: Res<Time>,
    mut turn: ResMut<Turn>,
    mut figure: Query<&mut Transform, With<Figure>>,
) {
    if state.visible && state.tab == PanelTab::Equipment {
        if keyboard.just_pressed(KeyCode::ArrowLeft) {
            turn.target += TURN;
        }
        if keyboard.just_pressed(KeyCode::ArrowRight) {
            turn.target -= TURN;
        }
    }
    let diff = turn.target - turn.current;
    if diff.abs() < 1e-4 {
        return;
    }
    turn.current += diff * (1.0 - (-TURN_SPEED * time.delta_secs()).exp());
    if let Ok(mut transform) = figure.single_mut() {
        transform.rotation = Quat::from_rotation_y(turn.current);
    }
}
