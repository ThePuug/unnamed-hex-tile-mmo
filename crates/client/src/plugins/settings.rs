//! The player's video settings and the one panel that changes them, opened
//! from the character screen and from the in-game menu alike.
//!
//! The panel is driven by keys alone: Up and Down pick a row, Left and
//! Right change it, Enter or Esc closes it. Whoever opens it routes the keys
//! to `navigate` while it is open, so one system decides what they close.

use bevy::{
    light::ShadowFilteringMethod,
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};

use common_bevy::components::Sun;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VideoSettings>();
        app.init_resource::<SettingsPanel>();
        app.add_systems(Startup, setup);
        app.add_systems(Update, (apply, draw));
    }
}

/// How the frame is drawn. Every field applies the moment it changes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoSettings {
    pub display: Display,
    pub vsync: Vsync,
    pub samples: Samples,
    pub shadows: Shadows,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Windowed,
    Borderless,
}

impl Display {
    fn step(self, _: i32) -> Self {
        match self {
            Display::Windowed => Display::Borderless,
            Display::Borderless => Display::Windowed,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Display::Windowed => "Windowed",
            Display::Borderless => "Borderless",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vsync {
    #[default]
    On,
    Off,
}

impl Vsync {
    fn step(self, _: i32) -> Self {
        match self {
            Vsync::On => Vsync::Off,
            Vsync::Off => Vsync::On,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Vsync::On => "On",
            Vsync::Off => "Off",
        }
    }
}

/// How many samples a camera takes per pixel. Coverage work — every
/// silhouette the depth prepass rasterises — scales with this, and the
/// wood is nothing but silhouette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Samples {
    #[default]
    Four,
    Two,
    Off,
}

impl Samples {
    const ALL: [Samples; 3] = [Samples::Four, Samples::Two, Samples::Off];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
    }

    pub fn label(self) -> &'static str {
        match self {
            Samples::Four => "4x",
            Samples::Two => "2x",
            Samples::Off => "Off",
        }
    }

    fn msaa(self) -> Msaa {
        match self {
            Samples::Four => Msaa::Sample4,
            Samples::Two => Msaa::Sample2,
            Samples::Off => Msaa::Off,
        }
    }
}

/// The sun's shadows: filtered by the Gaussian, by the hardware's 2×2
/// tap, or not cast at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shadows {
    #[default]
    Gaussian,
    Hard,
    Off,
}

impl Shadows {
    const ALL: [Shadows; 3] = [Shadows::Gaussian, Shadows::Hard, Shadows::Off];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
    }

    pub fn label(self) -> &'static str {
        match self {
            Shadows::Gaussian => "Gaussian",
            Shadows::Hard => "2x2",
            Shadows::Off => "Off",
        }
    }
}

/// The value `by` steps along `all` from `at`, wrapping at either end.
fn step<T: Copy + PartialEq>(all: &[T], at: T, by: i32) -> T {
    let i = all.iter().position(|&v| v == at).unwrap_or(0) as i32;
    all[(i + by).rem_euclid(all.len() as i32) as usize]
}

/// A row of the panel, in the order it lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Display,
    Vsync,
    Samples,
    Shadows,
}

impl Row {
    const ALL: [Row; 4] = [Row::Display, Row::Vsync, Row::Samples, Row::Shadows];

    fn name(self) -> &'static str {
        match self {
            Row::Display => "Display",
            Row::Vsync => "VSync",
            Row::Samples => "Anti-aliasing",
            Row::Shadows => "Shadows",
        }
    }

    fn value(self, video: &VideoSettings) -> &'static str {
        match self {
            Row::Display => video.display.label(),
            Row::Vsync => video.vsync.label(),
            Row::Samples => video.samples.label(),
            Row::Shadows => video.shadows.label(),
        }
    }

    fn change(self, video: &VideoSettings, by: i32) -> VideoSettings {
        let mut next = *video;
        match self {
            Row::Display => next.display = video.display.step(by),
            Row::Vsync => next.vsync = video.vsync.step(by),
            Row::Samples => next.samples = video.samples.step(by),
            Row::Shadows => next.shadows = video.shadows.step(by),
        }
        next
    }
}

/// Whether the panel shows, and which row it has picked.
#[derive(Resource, Default)]
pub struct SettingsPanel {
    pub open: bool,
    row: usize,
}

impl SettingsPanel {
    pub fn open(&mut self) {
        self.open = true;
        self.row = 0;
    }
}

/// Acts on this frame's keys while the panel is open: picks a row, changes
/// its setting, or closes the panel on Enter or Esc.
pub fn navigate(
    panel: &mut ResMut<SettingsPanel>,
    video: &mut ResMut<VideoSettings>,
    keyboard: &ButtonInput<KeyCode>,
) {
    const KEYS: [KeyCode; 7] = [
        KeyCode::Escape, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft,
        KeyCode::ArrowRight, KeyCode::Enter, KeyCode::NumpadEnter,
    ];
    if !keyboard.any_just_pressed(KEYS) {
        return;
    }
    if keyboard.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter]) {
        panel.open = false;
        return;
    }
    let rows = Row::ALL.len();
    if keyboard.just_pressed(KeyCode::ArrowUp) {
        panel.row = (panel.row + rows - 1) % rows;
    }
    if keyboard.just_pressed(KeyCode::ArrowDown) {
        panel.row = (panel.row + 1) % rows;
    }
    let by = if keyboard.just_pressed(KeyCode::ArrowLeft) {
        -1
    } else if keyboard.just_pressed(KeyCode::ArrowRight) {
        1
    } else {
        0
    };
    if by != 0 {
        let row = Row::ALL[panel.row];
        video.set_if_neq(row.change(video, by));
    }
}

/// Puts the settings on the window, every camera and the sun. Runs on a
/// change and for each camera that appears, so a camera spawned later
/// draws as the rest do.
pub fn apply(
    mut commands: Commands,
    video: Res<VideoSettings>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut cameras: Query<(Entity, Ref<Camera>, &mut Msaa, Has<Camera3d>)>,
    mut sun: Query<&mut DirectionalLight, With<Sun>>,
) {
    let changed = video.is_changed();
    if let (true, Ok(mut window)) = (changed, window.single_mut()) {
        let mode = match video.display {
            Display::Windowed => WindowMode::Windowed,
            Display::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        };
        let present = match video.vsync {
            Vsync::On => PresentMode::Fifo,
            Vsync::Off => PresentMode::AutoNoVsync,
        };
        if window.mode != mode {
            window.mode = mode;
        }
        if window.present_mode != present {
            window.present_mode = present;
        }
    }
    let method = match video.shadows {
        Shadows::Hard => ShadowFilteringMethod::Hardware2x2,
        _ => ShadowFilteringMethod::Gaussian,
    };
    // Every camera on the window, or the ones left behind stop sharing its
    // main texture and draw the UI a second time.
    for (entity, camera, mut msaa, is_3d) in &mut cameras {
        if !changed && !camera.is_added() {
            continue;
        }
        msaa.set_if_neq(video.samples.msaa());
        if is_3d {
            commands.entity(entity).insert(method);
        }
    }
    if changed {
        for mut light in &mut sun {
            light.shadow_maps_enabled = video.shadows != Shadows::Off;
        }
    }
}

#[derive(Component)]
struct PanelRoot;

#[derive(Component)]
struct PanelRows;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            PanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(PANEL_WIDTH),
                margin: UiRect { left: Val::Px(-PANEL_WIDTH / 2.0), top: Val::Px(-140.0), ..default() },
                padding: UiRect::all(Val::Px(20.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.95)),
            Visibility::Hidden,
            GlobalZIndex(2000),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Settings"),
                TextFont { font_size: FontSize::Px(22.0), ..default() },
                TextColor(TITLE),
            ));
            parent.spawn((
                PanelRows,
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() },
            ));
            parent.spawn((
                Text::new("Up/Down choose   Left/Right change   Enter/Esc back"),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(HINT),
            ));
        });
}

const PANEL_WIDTH: f32 = 420.0;
const TITLE: Color = Color::srgb(0.85, 0.75, 0.55);
const PICKED: Color = Color::srgb(1.0, 0.9, 0.6);
const PLAIN: Color = Color::srgb(0.75, 0.75, 0.75);
const HINT: Color = Color::srgb(0.5, 0.5, 0.5);

/// Shows the panel while it is open and redraws its rows when a setting or
/// the picked row changes.
fn draw(
    mut commands: Commands,
    panel: Res<SettingsPanel>,
    video: Res<VideoSettings>,
    mut root: Query<&mut Visibility, With<PanelRoot>>,
    rows: Query<Entity, With<PanelRows>>,
) {
    if !panel.is_changed() && !video.is_changed() {
        return;
    }
    if let Ok(mut visibility) = root.single_mut() {
        visibility.set_if_neq(if panel.open { Visibility::Visible } else { Visibility::Hidden });
    }
    let Ok(rows) = rows.single() else { return };
    commands.entity(rows).despawn_related::<Children>().with_children(|parent| {
        for (i, row) in Row::ALL.iter().enumerate() {
            let picked = i == panel.row;
            let text = if picked {
                format!("> {:<16}< {} >", row.name(), row.value(&video))
            } else {
                format!("  {:<16}  {}", row.name(), row.value(&video))
            };
            parent.spawn((
                Text::new(text),
                TextFont { font_size: FontSize::Px(17.0), ..default() },
                TextColor(if picked { PICKED } else { PLAIN }),
            ));
        }
    });
}
