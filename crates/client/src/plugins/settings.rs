//! The player's video and audio settings and the one panel that changes
//! them, opened from the character screen and from the in-game menu alike.
//!
//! The panel is driven by keys alone: Up and Down pick a row, Left and
//! Right change it, Enter or Esc closes it. Whoever opens it routes the keys
//! to `navigate` while it is open, so one system decides what they close.
//!
//! The settings are kept in the client's folder (`user_data`) as the
//! panel closes, and read back as the plugin is built, before the window
//! opens, so the window opens as they were left.

use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bevy::{
    light::ShadowFilteringMethod,
    prelude::*,
    window::{
        Monitor, MonitorSelection, OnMonitor, PresentMode, PrimaryMonitor, PrimaryWindow, VideoMode,
        VideoModeSelection, WindowMode, WindowResolution,
    },
};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::components::Sun;

/// Installs the kept settings and opens the primary window by them. Added
/// after `WindowPlugin`, which spawns that window as it is built; the OS
/// window is made from it only once the app runs.
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let saved = load();
        let world = app.world_mut();
        if let Ok(mut window) = world.query_filtered::<&mut Window, With<PrimaryWindow>>().single_mut(world) {
            saved.video.open(&mut window);
        }
        app.insert_resource(saved.video);
        app.insert_resource(saved.audio);
        app.init_resource::<SettingsPanel>();
        app.add_systems(Startup, setup);
        app.add_systems(Update, ((fit_to_monitor, apply).chain(), yield_screen, draw, save));
        app.add_systems(Last, pace);
    }
}

/// How the frame is drawn. Every field applies the moment it changes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    #[serde(deserialize_with = "lenient")]
    pub display: Display,
    /// `None` until the window's monitor is known, which fits it to a mode
    /// that monitor offers (`fit_to_monitor`).
    #[serde(deserialize_with = "lenient")]
    pub resolution: Option<Resolution>,
    #[serde(deserialize_with = "lenient")]
    pub vsync: Vsync,
    #[serde(deserialize_with = "lenient")]
    pub rate: Rate,
    #[serde(deserialize_with = "lenient")]
    pub samples: Samples,
    #[serde(deserialize_with = "lenient")]
    pub shadows: Shadows,
}

impl VideoSettings {
    /// Sets a window not yet made to open as these settings say, so a kept
    /// fullscreen opens without a windowed frame first. It names the primary
    /// monitor: before the window exists winit has no current one, and Bevy
    /// panics on it.
    fn open(&self, window: &mut Window) {
        window.resizable = false;
        window.enabled_buttons.maximize = false;
        window.present_mode = self.vsync.present_mode();
        window.mode = self.window_mode(MonitorSelection::Primary);
        // Logical at creation, before the window knows its scale factor;
        // `apply` puts it right in physical pixels.
        if let (Display::Windowed, Some(resolution)) = (self.display, self.resolution) {
            window.resolution = WindowResolution::new(resolution.width, resolution.height);
        }
    }

    fn window_mode(&self, monitor: MonitorSelection) -> WindowMode {
        match (self.display, self.resolution) {
            (Display::Windowed, _) => WindowMode::Windowed,
            (Display::Borderless, _) => WindowMode::BorderlessFullscreen(monitor),
            (Display::Fullscreen, Some(resolution)) => {
                WindowMode::Fullscreen(monitor, VideoModeSelection::Specific(resolution.video_mode()))
            }
            (Display::Fullscreen, None) => WindowMode::Fullscreen(monitor, VideoModeSelection::Current),
        }
    }
}

/// A window the size of the chosen resolution; a window covering the screen
/// at the desktop's mode, which takes no resolution; or the whole screen
/// switched to the chosen resolution, which the GPU or the monitor scales to
/// the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    #[default]
    Windowed,
    Borderless,
    Fullscreen,
}

impl Display {
    const ALL: [Display; 3] = [Display::Windowed, Display::Borderless, Display::Fullscreen];

    fn step(self, by: i32) -> Self {
        step(&Self::ALL, self, by)
    }

    pub fn label(self) -> &'static str {
        match self {
            Display::Windowed => "Windowed",
            Display::Borderless => "Borderless",
            Display::Fullscreen => "Fullscreen",
        }
    }

    /// Whether the window is the chosen resolution's size. Borderless keeps
    /// the chosen one for the next display that takes it.
    pub fn takes_resolution(self) -> bool {
        self != Display::Borderless
    }
}

/// A video mode the window's monitor offers: what a fullscreen window
/// switches the display to, and a windowed one's size in physical pixels.
/// Kept whole, refresh and depth with the size, because a fullscreen window
/// opens only on a mode the monitor lists exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: u32,
    pub bit_depth: u16,
}

impl Resolution {
    fn of(mode: &VideoMode) -> Self {
        Resolution {
            width: mode.physical_size.x,
            height: mode.physical_size.y,
            refresh_millihertz: mode.refresh_rate_millihertz,
            bit_depth: mode.bit_depth,
        }
    }

    /// A windowed size no monitor need offer, as the recorder frames its
    /// shots.
    pub fn sized(width: u32, height: u32) -> Self {
        Resolution { width, height, refresh_millihertz: 0, bit_depth: 0 }
    }

    fn video_mode(self) -> VideoMode {
        VideoMode {
            physical_size: UVec2::new(self.width, self.height),
            bit_depth: self.bit_depth,
            refresh_rate_millihertz: self.refresh_millihertz,
        }
    }

    fn size(self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }

    pub fn label(self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// The resolutions `modes` offer, one per size at its highest refresh and
/// depth, smallest first.
fn offered(modes: &[VideoMode]) -> Vec<Resolution> {
    let mut all: Vec<Resolution> = modes.iter().map(Resolution::of).collect();
    all.sort_by_key(|r| (r.width as u64 * r.height as u64, r.width, Reverse(r.refresh_millihertz), Reverse(r.bit_depth)));
    all.dedup_by_key(|r| r.size());
    all
}

/// The offered resolution nearest `wanted` in size: its own where offered.
fn nearest(offered: &[Resolution], wanted: UVec2) -> Option<Resolution> {
    offered.iter().copied().min_by_key(|r| r.width.abs_diff(wanted.x) + r.height.abs_diff(wanted.y))
}

/// `at` where a window that size fits on the desktop with its frame around
/// it, else the largest offered size smaller both ways than the desktop.
/// A window the desktop's size spills its title bar off the screen, and it
/// cannot be dragged smaller.
fn windowed(offered: &[Resolution], desktop: UVec2, at: Option<Resolution>) -> Option<Resolution> {
    let fits = |r: &Resolution| r.width < desktop.x && r.height < desktop.y;
    match at {
        Some(at) if fits(&at) => Some(at),
        _ => offered.iter().copied().filter(fits).last().or(at),
    }
}

/// The resolution `by` steps along `offered` from `at`. It stops at either
/// end: each step is a mode switch on a fullscreen display, and a wrap would
/// jump from the smallest to the largest.
fn step_resolution(offered: &[Resolution], at: Option<Resolution>, by: i32) -> Option<Resolution> {
    let Some(i) = offered.iter().position(|&r| Some(r) == at) else { return at };
    let last = offered.len() as i32 - 1;
    Some(offered[(i as i32 + by).clamp(0, last) as usize])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

    fn present_mode(self) -> PresentMode {
        match self {
            Vsync::On => PresentMode::Fifo,
            Vsync::Off => PresentMode::AutoNoVsync,
        }
    }
}

/// The most frames a second the client draws. A frame drawn faster than
/// the cap waits out the rest of its interval, so the GPU idles instead
/// of drawing frames no one needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    #[serde(deserialize_with = "lenient")]
    pub music: Level,
}

/// A volume in tenths, from silent to the level a piece was rendered at.
/// Kept as the count of tenths, and clamped as it is read back, so an
/// edited file cannot play louder than full.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct Level(u8);

impl Default for Level {
    fn default() -> Self {
        Level(10)
    }
}

impl From<u8> for Level {
    fn from(tenths: u8) -> Self {
        Level(tenths.min(10))
    }
}

impl From<Level> for u8 {
    fn from(level: Level) -> Self {
        level.0
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

/// What the client keeps of its settings between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    #[serde(deserialize_with = "lenient")]
    video: VideoSettings,
    #[serde(deserialize_with = "lenient")]
    audio: AudioSettings,
}

/// Reads a field as `T`, or its default where the value is not one, so one
/// bad value costs that setting alone and a setting added later keeps the
/// rest of an older file.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_else(|e| {
        warn!("settings: {e}; taking the default");
        T::default()
    }))
}

impl Saved {
    fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_else(|e| {
            warn!("settings: {e}; taking the defaults");
            Saved::default()
        })
    }
}

fn path() -> Option<PathBuf> {
    user_data::folder("client").map(|folder| folder.join("settings.json"))
}

/// The settings kept by the last run, or the defaults where none were.
fn load() -> Saved {
    let Some(path) = path() else { return Saved::default() };
    match fs::read_to_string(&path) {
        Ok(text) => Saved::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::default(),
        Err(e) => {
            warn!("settings: {}: {e}; taking the defaults", path.display());
            Saved::default()
        }
    }
}

/// Writes beside `path` and renames over it, so a run that stops mid-write
/// leaves the last whole file.
fn write(path: &Path, saved: &Saved) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let partial = path.with_extension("json.partial");
    let text = serde_json::to_string_pretty(saved).map_err(std::io::Error::other)?;
    fs::write(&partial, text)?;
    fs::rename(&partial, path)
}

/// Keeps the settings as the panel closes, once a visit however many rows
/// changed.
fn save(panel: Res<SettingsPanel>, video: Res<VideoSettings>, audio: Res<AudioSettings>, mut was_open: Local<bool>) {
    let closed = *was_open && !panel.open;
    *was_open = panel.open;
    if !closed {
        return;
    }
    let Some(path) = path() else { return };
    if let Err(e) = write(&path, &Saved { video: *video, audio: *audio }) {
        warn!("settings: {}: {e}", path.display());
    }
}

/// A row of the panel, in the order it lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Display,
    Resolution,
    Vsync,
    Rate,
    Samples,
    Shadows,
    Music,
}

impl Row {
    const ALL: [Row; 7] = [Row::Display, Row::Resolution, Row::Vsync, Row::Rate, Row::Samples, Row::Shadows, Row::Music];

    /// The heading it is listed under; rows of one section are adjacent.
    fn section(self) -> &'static str {
        match self {
            Row::Display | Row::Resolution | Row::Vsync | Row::Rate | Row::Samples | Row::Shadows => "Video",
            Row::Music => "Audio",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Row::Display => "Display",
            Row::Resolution => "Resolution",
            Row::Vsync => "VSync",
            Row::Rate => "Frame rate",
            Row::Samples => "Anti-aliasing",
            Row::Shadows => "Shadows",
            Row::Music => "Music",
        }
    }

    /// Whether Left and Right change it: the resolution waits while the
    /// display takes none.
    fn enabled(self, video: &VideoSettings) -> bool {
        self != Row::Resolution || video.display.takes_resolution()
    }

    fn value(self, video: &VideoSettings, audio: &AudioSettings) -> String {
        match self {
            Row::Display => video.display.label().into(),
            Row::Resolution if !self.enabled(video) => "Desktop".into(),
            Row::Resolution => video.resolution.map_or_else(|| "Desktop".into(), Resolution::label),
            Row::Vsync => video.vsync.label().into(),
            Row::Rate => video.rate.label().into(),
            Row::Samples => video.samples.label().into(),
            Row::Shadows => video.shadows.label().into(),
            Row::Music => audio.music.label(),
        }
    }

    fn change(
        self,
        video: &VideoSettings,
        audio: &AudioSettings,
        panel: &SettingsPanel,
        by: i32,
    ) -> (VideoSettings, AudioSettings) {
        let (mut video, mut audio) = (*video, *audio);
        if !self.enabled(&video) {
            return (video, audio);
        }
        match self {
            Row::Display => {
                video.display = video.display.step(by);
                if video.display == Display::Windowed {
                    video.resolution = windowed(&panel.resolutions, panel.desktop, video.resolution);
                }
            }
            Row::Resolution => video.resolution = step_resolution(&panel.resolutions, video.resolution, by),
            Row::Vsync => video.vsync = video.vsync.step(by),
            Row::Rate => video.rate = video.rate.step(by),
            Row::Samples => video.samples = video.samples.step(by),
            Row::Shadows => video.shadows = video.shadows.step(by),
            Row::Music => audio.music = audio.music.step(by),
        }
        (video, audio)
    }
}

/// Whether the panel shows, which row it has picked, and the resolutions
/// the window's monitor offers it, smallest first, beside the desktop's.
#[derive(Resource, Default)]
pub struct SettingsPanel {
    pub open: bool,
    row: usize,
    resolutions: Vec<Resolution>,
    desktop: UVec2,
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
        let (next_video, next_audio) = row.change(video, audio, panel, by);
        video.set_if_neq(next_video);
        audio.set_if_neq(next_audio);
    }
}

/// Reads the resolutions the window's monitor offers into the panel as the
/// window meets a monitor, and fits the chosen resolution to them: one the
/// monitor lacks becomes the nearest it has, and none chosen the desktop's,
/// or a window the size below it.
fn fit_to_monitor(
    window: Query<Option<&OnMonitor>, With<PrimaryWindow>>,
    monitors: Query<(Entity, &Monitor)>,
    primary: Query<Entity, With<PrimaryMonitor>>,
    mut panel: ResMut<SettingsPanel>,
    mut video: ResMut<VideoSettings>,
    mut met: Local<Option<Entity>>,
) {
    let Ok(on) = window.single() else { return };
    let Some(entity) = on.map(|on| on.0).or_else(|| primary.single().ok()) else { return };
    let Ok((entity, monitor)) = monitors.get(entity) else { return };
    if *met == Some(entity) {
        return;
    }
    *met = Some(entity);
    log_modes(monitor);
    let resolutions = offered(&monitor.video_modes);
    let desktop = monitor.physical_size();
    let fitted = match (video.resolution, video.display) {
        (Some(at), _) => nearest(&resolutions, at.size()),
        (None, Display::Windowed | Display::Borderless) => windowed(&resolutions, desktop, nearest(&resolutions, desktop)),
        (None, Display::Fullscreen) => nearest(&resolutions, desktop),
    };
    if fitted.is_some() && video.resolution != fitted {
        video.resolution = fitted;
    }
    panel.resolutions = resolutions;
    panel.desktop = desktop;
}

/// Gives the screen back while a fullscreen window is out of focus: winit
/// keeps an exclusive window on top and its mode on the display, so another
/// window taking focus stays hidden under it. Out of focus the window goes
/// windowed, which puts the desktop's mode back, and minimises; in focus
/// again it is fullscreen as the settings say.
fn yield_screen(
    mut focus: MessageReader<bevy::window::WindowFocused>,
    video: Res<VideoSettings>,
    mut window: Query<(Entity, &mut Window), With<PrimaryWindow>>,
) {
    let Ok((entity, mut window)) = window.single_mut() else { return };
    let Some(focused) = focus.read().filter(|f| f.window == entity).last().map(|f| f.focused) else { return };
    if video.display != Display::Fullscreen {
        return;
    }
    if focused {
        window.mode = video.window_mode(MonitorSelection::Current);
    } else {
        window.mode = WindowMode::Windowed;
        window.set_minimized(true);
    }
}

/// Logs every mode `monitor` lists, a line per size with each refresh and
/// depth it comes in, so a size missing from the panel can be told from a
/// size the monitor never offered.
fn log_modes(monitor: &Monitor) {
    let mut modes = monitor.video_modes.clone();
    modes.sort_by_key(|m| (m.physical_size.x as u64 * m.physical_size.y as u64, m.physical_size.x, m.refresh_rate_millihertz, m.bit_depth));
    info!(
        "settings: {} at {}x{} scale {} lists {} modes",
        monitor.name.as_deref().unwrap_or("monitor"),
        monitor.physical_width,
        monitor.physical_height,
        monitor.scale_factor,
        modes.len()
    );
    for size in modes.chunk_by(|a, b| a.physical_size == b.physical_size) {
        let rates: Vec<String> = size
            .iter()
            .map(|m| format!("{:.2}Hz/{}bit", m.refresh_rate_millihertz as f64 / 1000.0, m.bit_depth))
            .collect();
        info!("settings:   {}x{}: {}", size[0].physical_size.x, size[0].physical_size.y, rates.join(", "));
    }
}

/// Puts the settings on the window, every camera and the sun. Runs on a
/// change and for each camera that appears, so a camera spawned later
/// draws as the rest do.
///
/// A windowed window is held to the chosen size whenever it is not: leaving
/// fullscreen restores the size before it, and a move between monitors
/// rescales it. A minimised window, sized zero, is left alone.
pub fn apply(
    mut commands: Commands,
    video: Res<VideoSettings>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut cameras: Query<(Entity, Ref<Camera>, &mut Msaa, Has<Camera3d>)>,
    mut sun: Query<&mut DirectionalLight, With<Sun>>,
) {
    let changed = video.is_changed();
    if let Ok(mut window) = window.single_mut() {
        if changed {
            let mode = video.window_mode(MonitorSelection::Current);
            let present = video.vsync.present_mode();
            if window.mode != mode {
                window.mode = mode;
            }
            if window.present_mode != present {
                window.present_mode = present;
            }
        }
        let size = window.resolution.physical_size();
        if let (Display::Windowed, Some(resolution)) = (video.display, video.resolution) {
            if size != resolution.size() && size != UVec2::ZERO {
                window.resolution.set_physical_resolution(resolution.width, resolution.height);
            }
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
const DISABLED: Color = Color::srgb(0.4, 0.4, 0.4);

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
            let enabled = row.enabled(&video);
            let text = match (picked, enabled) {
                (true, true) => format!("> {:<16}< {} >", row.name(), row.value(&video, &audio)),
                (true, false) => format!("> {:<16}  {}", row.name(), row.value(&video, &audio)),
                (false, _) => format!("  {:<16}  {}", row.name(), row.value(&video, &audio)),
            };
            let color = match (picked, enabled) {
                (_, false) => DISABLED,
                (true, true) => PICKED,
                (false, true) => PLAIN,
            };
            parent.spawn((
                Text::new(text),
                TextFont { font_size: FontSize::Px(17.0), ..default() },
                TextColor(color),
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

    fn mode(width: u32, height: u32, hz: u32) -> VideoMode {
        VideoMode { physical_size: UVec2::new(width, height), bit_depth: 32, refresh_rate_millihertz: hz * 1000 }
    }

    #[test]
    fn a_monitor_offers_each_size_once_at_its_fastest_smallest_first() {
        let modes = [mode(2560, 1440, 60), mode(1280, 720, 60), mode(2560, 1440, 144), mode(1920, 1080, 120)];
        let sizes: Vec<_> = offered(&modes).iter().map(|r| (r.width, r.height, r.refresh_millihertz)).collect();
        assert_eq!(sizes, [(1280, 720, 60_000), (1920, 1080, 120_000), (2560, 1440, 144_000)]);
    }

    #[test]
    fn a_size_the_monitor_lacks_takes_the_nearest() {
        let offered = offered(&[mode(1280, 720, 60), mode(1920, 1080, 60), mode(2560, 1440, 60)]);
        assert_eq!(nearest(&offered, UVec2::new(1920, 1080)).map(Resolution::size), Some(UVec2::new(1920, 1080)));
        assert_eq!(nearest(&offered, UVec2::new(1920, 1200)).map(Resolution::size), Some(UVec2::new(1920, 1080)));
        assert_eq!(nearest(&offered, UVec2::new(3840, 2160)).map(Resolution::size), Some(UVec2::new(2560, 1440)));
        assert_eq!(nearest(&[], UVec2::new(1920, 1080)), None);
    }

    #[test]
    fn a_window_takes_the_size_below_the_desktop() {
        let offered = offered(&[mode(1280, 720, 60), mode(1920, 1080, 60), mode(2560, 1440, 60), mode(3840, 2160, 60)]);
        let desktop = UVec2::new(2560, 1440);
        let size = |r: Option<Resolution>| r.map(Resolution::size);
        assert_eq!(size(windowed(&offered, desktop, Some(offered[2]))), Some(UVec2::new(1920, 1080)), "the desktop's own");
        assert_eq!(size(windowed(&offered, desktop, Some(offered[3]))), Some(UVec2::new(1920, 1080)), "above it");
        assert_eq!(size(windowed(&offered, desktop, Some(offered[0]))), Some(UVec2::new(1280, 720)), "one that fits stays");
        assert_eq!(windowed(&offered[2..3], desktop, Some(offered[2])), Some(offered[2]), "nothing smaller offered");
    }

    #[test]
    fn borderless_holds_the_resolution_for_the_next_display() {
        let offered = offered(&[mode(1280, 720, 60), mode(1920, 1080, 60), mode(2560, 1440, 60)]);
        let panel = SettingsPanel { resolutions: offered.clone(), desktop: UVec2::new(2560, 1440), ..default() };
        let audio = AudioSettings::default();
        let borderless = VideoSettings { display: Display::Borderless, resolution: Some(offered[2]), ..default() };
        assert_eq!(Row::Resolution.value(&borderless, &audio), "Desktop");
        assert_eq!(Row::Resolution.change(&borderless, &audio, &panel, 1).0, borderless, "the row does not change");
        assert_eq!(Row::Resolution.change(&borderless, &audio, &panel, -1).0, borderless);

        let fullscreen = Row::Display.change(&borderless, &audio, &panel, 1).0;
        assert_eq!((fullscreen.display, fullscreen.resolution), (Display::Fullscreen, Some(offered[2])), "kept for fullscreen");
        let windowed = Row::Display.change(&borderless, &audio, &panel, -1).0;
        assert_eq!((windowed.display, windowed.resolution), (Display::Windowed, Some(offered[1])), "a window steps below the desktop");
    }

    #[test]
    fn resolutions_stop_at_either_end() {
        let offered = offered(&[mode(1280, 720, 60), mode(1920, 1080, 60)]);
        assert_eq!(step_resolution(&offered, Some(offered[0]), -1), Some(offered[0]));
        assert_eq!(step_resolution(&offered, Some(offered[0]), 1), Some(offered[1]));
        assert_eq!(step_resolution(&offered, Some(offered[1]), 1), Some(offered[1]));
    }

    #[test]
    fn settings_read_back_as_they_were_kept() {
        let kept = Saved {
            video: VideoSettings {
                display: Display::Fullscreen,
                resolution: Some(Resolution::of(&mode(1920, 1080, 144))),
                vsync: Vsync::Off,
                rate: Rate::Unlimited,
                samples: Samples::Two,
                shadows: Shadows::Hard,
            },
            audio: AudioSettings { music: Level(3) },
        };
        assert_eq!(Saved::parse(&serde_json::to_string(&kept).unwrap()), kept);
    }

    #[test]
    fn a_bad_value_costs_its_setting_alone() {
        let saved = Saved::parse(r#"{"video": {"display": "Fullscreen", "vsync": "Sometimes", "shadows": "Off"}, "audio": {"music": 99}}"#);
        assert_eq!(saved.video.display, Display::Fullscreen);
        assert_eq!(saved.video.vsync, Vsync::default());
        assert_eq!(saved.video.shadows, Shadows::Off);
        assert_eq!(saved.video.resolution, None);
        assert_eq!(saved.audio.music, Level(10));
    }

    #[test]
    fn a_file_that_is_not_settings_reads_as_the_defaults() {
        assert_eq!(Saved::parse("not json"), Saved::default());
        assert_eq!(Saved::parse("[1, 2]"), Saved::default());
    }
}
