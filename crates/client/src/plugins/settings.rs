//! The player's video and audio settings and the one panel that changes
//! them, opened from the character screen and from the in-game menu alike.
//!
//! The panel is driven by keys alone: Up and Down pick a row, Left and
//! Right change it, Enter or Esc closes it. Whoever opens it routes the keys
//! to `navigate` while it is open, so one system decides what they close.

use std::time::{Duration, Instant};

use bevy::{
    light::ShadowFilteringMethod,
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};

use crate::components::Sun;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VideoSettings>();
        app.init_resource::<AudioSettings>();
        app.init_resource::<SettingsPanel>();
        app.add_systems(Startup, setup);
        app.add_systems(Update, (apply, draw));
        app.add_systems(Last, pace);
    }
}

/// How the frame is drawn. Every field applies the moment it changes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoSettings {
    pub display: Display,
    pub vsync: Vsync,
    pub rate: Rate,
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
    const ALL: [Display; 2] = [Display::Windowed, Display::Borderless];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
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
    const ALL: [Vsync; 2] = [Vsync::On, Vsync::Off];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
    }

    pub fn label(self) -> &'static str {
        match self {
            Vsync::On => "On",
            Vsync::Off => "Off",
        }
    }
}

/// The most frames a second the client draws. A frame drawn faster than
/// the cap waits out the rest of its interval, so the GPU idles instead
/// of drawing frames no one needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rate {
    #[default]
    Sixty,
    Ninety,
    OneTwenty,
    Unlimited,
}

impl Rate {
    const ALL: [Rate; 4] = [Rate::Sixty, Rate::Ninety, Rate::OneTwenty, Rate::Unlimited];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
    }

    pub fn label(self) -> &'static str {
        match self {
            Rate::Sixty => "60",
            Rate::Ninety => "90",
            Rate::OneTwenty => "120",
            Rate::Unlimited => "Unlimited",
        }
    }

    /// The shortest a frame may take, or `None` when uncapped.
    fn interval(self) -> Option<Duration> {
        let fps = match self {
            Rate::Sixty => 60.0,
            Rate::Ninety => 90.0,
            Rate::OneTwenty => 120.0,
            Rate::Unlimited => return None,
        };
        Some(Duration::from_secs_f64(1.0 / fps))
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

/// What is heard. The music plugin reads it as it plays, so a change is
/// heard at once.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioSettings {
    pub music: Level,
}

/// A volume in tenths, from silent to the level a piece was rendered at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level(u8);

impl Default for Level {
    fn default() -> Self {
        Level(10)
    }
}

impl Level {
    /// Stops at either end: a volume wrapping from silent to full would
    /// jump out at the listener.
    fn step(self, by: i32) -> Self {
        Level((self.0 as i32 + by).clamp(0, 10) as u8)
    }

    pub fn label(self) -> String {
        format!("{}%", self.0 * 10)
    }

    /// The linear gain to play at. Squared, so each step moves evenly to
    /// the ear rather than crowding the change near silent.
    pub fn gain(self) -> f32 {
        (self.0 as f32 / 10.0).powi(2)
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
    Rate,
    Samples,
    Shadows,
    Music,
}

impl Row {
    const ALL: [Row; 6] = [Row::Display, Row::Vsync, Row::Rate, Row::Samples, Row::Shadows, Row::Music];

    /// The heading it is listed under; rows of one section are adjacent.
    fn section(self) -> &'static str {
        match self {
            Row::Display | Row::Vsync | Row::Rate | Row::Samples | Row::Shadows => "Video",
            Row::Music => "Audio",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Row::Display => "Display",
            Row::Vsync => "VSync",
            Row::Rate => "Frame rate",
            Row::Samples => "Anti-aliasing",
            Row::Shadows => "Shadows",
            Row::Music => "Music",
        }
    }

    fn value(self, video: &VideoSettings, audio: &AudioSettings) -> String {
        match self {
            Row::Display => video.display.label().into(),
            Row::Vsync => video.vsync.label().into(),
            Row::Rate => video.rate.label().into(),
            Row::Samples => video.samples.label().into(),
            Row::Shadows => video.shadows.label().into(),
            Row::Music => audio.music.label(),
        }
    }

    fn change(self, video: &VideoSettings, audio: &AudioSettings, by: i32) -> (VideoSettings, AudioSettings) {
        let (mut video, mut audio) = (*video, *audio);
        match self {
            Row::Display => video.display = video.display.step(by),
            Row::Vsync => video.vsync = video.vsync.step(by),
            Row::Rate => video.rate = video.rate.step(by),
            Row::Samples => video.samples = video.samples.step(by),
            Row::Shadows => video.shadows = video.shadows.step(by),
            Row::Music => audio.music = audio.music.step(by),
        }
        (video, audio)
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
    audio: &mut ResMut<AudioSettings>,
    keys: &mut crate::systems::help::Keys,
) {
    const BACK: &str = "Close the settings";
    if keys.pressed(KeyCode::Escape, BACK) | keys.pressed(KeyCode::Enter, BACK) | keys.pressed(KeyCode::NumpadEnter, BACK) {
        panel.open = false;
        return;
    }
    let rows = Row::ALL.len();
    if keys.pressed(KeyCode::ArrowUp, "Choose the setting above") {
        panel.row = (panel.row + rows - 1) % rows;
    }
    if keys.pressed(KeyCode::ArrowDown, "Choose the setting below") {
        panel.row = (panel.row + 1) % rows;
    }
    let by = if keys.pressed(KeyCode::ArrowLeft, "Change the setting back") {
        -1
    } else if keys.pressed(KeyCode::ArrowRight, "Change the setting on") {
        1
    } else {
        0
    };
    if by != 0 {
        let row = Row::ALL[panel.row];
        let (next_video, next_audio) = row.change(video, audio, by);
        video.set_if_neq(next_video);
        audio.set_if_neq(next_audio);
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

/// Holds each frame to the cap by sleeping until an interval has passed
/// since the last one ended. The deadlines step by the interval, so the
/// rate holds without drifting; a frame that overruns starts the count
/// again from its own end rather than rushing the next to catch up.
/// Pipelined rendering keeps the render thread one frame behind the main
/// one, so it is held to the same rate.
fn pace(video: Res<VideoSettings>, mut last: Local<Option<Instant>>) {
    let Some(interval) = video.rate.interval() else {
        *last = None;
        return;
    };
    let now = Instant::now();
    let due = last.map(|at| at + interval).filter(|&due| due > now);
    if let Some(due) = due {
        std::thread::sleep(due - now);
    }
    *last = Some(due.unwrap_or(now));
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
            use crate::systems::keycap::{hint_row, Hint};
            hint_row(
                parent,
                None,
                &[
                    Hint::either(&[KeyCode::ArrowUp, KeyCode::ArrowDown], "choose"),
                    Hint::either(&[KeyCode::ArrowLeft, KeyCode::ArrowRight], "change"),
                    Hint::either(&[KeyCode::Enter, KeyCode::Escape], "back"),
                ],
            );
        });
}

const PANEL_WIDTH: f32 = 420.0;
const TITLE: Color = Color::srgb(0.85, 0.75, 0.55);
const PICKED: Color = Color::srgb(1.0, 0.9, 0.6);
const PLAIN: Color = Color::srgb(0.75, 0.75, 0.75);

/// Shows the panel while it is open and redraws its rows when a setting or
/// the picked row changes.
fn draw(
    mut commands: Commands,
    panel: Res<SettingsPanel>,
    video: Res<VideoSettings>,
    audio: Res<AudioSettings>,
    mut root: Query<&mut Visibility, With<PanelRoot>>,
    rows: Query<Entity, With<PanelRows>>,
) {
    if !panel.is_changed() && !video.is_changed() && !audio.is_changed() {
        return;
    }
    if let Ok(mut visibility) = root.single_mut() {
        visibility.set_if_neq(if panel.open { Visibility::Visible } else { Visibility::Hidden });
    }
    let Ok(rows) = rows.single() else { return };
    commands.entity(rows).despawn_related::<Children>().with_children(|parent| {
        for (i, row) in Row::ALL.iter().enumerate() {
            if i == 0 || Row::ALL[i - 1].section() != row.section() {
                parent.spawn((
                    Text::new(row.section()),
                    TextFont { font_size: FontSize::Px(15.0), ..default() },
                    TextColor(TITLE),
                ));
            }
            let picked = i == panel.row;
            let text = if picked {
                format!("> {:<16}< {} >", row.name(), row.value(&video, &audio))
            } else {
                format!("  {:<16}  {}", row.name(), row.value(&video, &audio))
            };
            parent.spawn((
                Text::new(text),
                TextFont { font_size: FontSize::Px(17.0), ..default() },
                TextColor(if picked { PICKED } else { PLAIN }),
            ));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_stops_at_either_end() {
        assert_eq!(Level::default().step(1), Level::default());
        assert_eq!(Level(0).step(-1), Level(0));
    }

    #[test]
    fn a_level_runs_from_silent_to_full_rising() {
        let gains: Vec<f32> = (0..=10).map(|n| Level(n).gain()).collect();
        assert_eq!(gains[0], 0.0);
        assert_eq!(gains[10], 1.0);
        assert!(gains.windows(2).all(|w| w[0] < w[1]));
    }
}
