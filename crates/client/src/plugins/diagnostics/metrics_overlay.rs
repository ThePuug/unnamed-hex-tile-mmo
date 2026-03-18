use std::collections::VecDeque;

use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_camera::Viewport;
use bevy_egui::{egui, EguiContext, EguiContexts};

use super::config::DiagnosticsState;
use super::network_ui::NetworkMetrics;
use common_bevy::{
    chunk::terrain_chunk_radius,
    components::{behaviour::PlayerControlled, Actor, Loc},
    resources::map::Map,
};
use qrz::Convert;

use crate::components::ChunkMesh;
use crate::resources::{LoadedChunks, ChunkLodMeshes};

// ── Layout constants (match server console) ──

const SEG_WIDTH: usize = 15;
const SEG_GAP: usize = 1;
const PANEL_CHARS: usize = 3 * SEG_WIDTH + 2 * SEG_GAP;
const SPARKLINE_CHARS: usize = 15;
const MIN_BAR_WIDTH_PX: f32 = 2.0;

const FONT_SIZE: f32 = 12.0;
const OUTER_MARGIN: f32 = 8.0;
const SECTION_INNER_MARGIN: f32 = 4.0;

// ── Colors (match server console) ──

const COLOR_NORMAL: egui::Color32 = egui::Color32::from_rgb(180, 255, 180);
const COLOR_CRITICAL: egui::Color32 = egui::Color32::from_rgb(255, 80, 80);
const COLOR_WARN: egui::Color32 = egui::Color32::from_rgb(255, 200, 60);
const COLOR_DIM: egui::Color32 = egui::Color32::from_rgb(120, 160, 120);
const COLOR_BORDER: egui::Color32 = egui::Color32::from_rgb(80, 120, 80);
const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(10, 15, 10);
const COLOR_SPARK_BG: egui::Color32 = egui::Color32::from_rgb(30, 30, 35);

fn mono_font() -> egui::FontId {
    egui::FontId::monospace(FONT_SIZE)
}

fn colored_mono(text: &str, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: mono_font(),
            color,
            ..Default::default()
        },
    );
    job
}



use common::numfmt;

// ── History (rolling window — matches server console) ──

const HISTORY_LEN: usize = 120;
/// Seconds between samples pushed into history.
const SAMPLE_INTERVAL: f32 = 0.5;

struct History(VecDeque<f64>);

impl History {
    fn new() -> Self {
        Self(VecDeque::with_capacity(HISTORY_LEN))
    }
    fn push(&mut self, val: f64) {
        if self.0.len() >= HISTORY_LEN {
            self.0.pop_front();
        }
        self.0.push_back(val);
    }
    fn as_f32(&self) -> Vec<f32> {
        self.0.iter().map(|&v| v as f32).collect()
    }
    fn visible_max(&self, n: usize) -> f64 {
        let start = self.0.len().saturating_sub(n);
        self.0.range(start..).copied().fold(0.0_f64, f64::max)
    }
}

/// Rolling window of raw frame times for p95 computation.
/// Time-based eviction keeps exactly the last `window_secs` of observations.
struct FrameTimeWindow {
    timestamps: VecDeque<f64>,
    values: VecDeque<f64>,
    window_secs: f64,
}

impl FrameTimeWindow {
    fn new(window_secs: f64) -> Self {
        Self {
            timestamps: VecDeque::new(),
            values: VecDeque::new(),
            window_secs,
        }
    }

    fn push(&mut self, time_secs: f64, val: f64) {
        let cutoff = time_secs - self.window_secs;
        while self.timestamps.front().map_or(false, |&t| t < cutoff) {
            self.timestamps.pop_front();
            self.values.pop_front();
        }
        self.timestamps.push_back(time_secs);
        self.values.push_back(val);
    }

    fn p95(&self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.values.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = (sorted.len() as f64 * 0.95).ceil() as usize;
        sorted[rank.saturating_sub(1)]
    }
}

/// Accumulated metric histories, sampled at SAMPLE_INTERVAL.
#[derive(Resource)]
pub struct MetricsHistory {
    timer: f32,
    /// Raw frame times — pushed every frame, p95 computed over 2s window.
    frame_window: FrameTimeWindow,
    /// Sparkline history of p95 frame times (sampled at SAMPLE_INTERVAL).
    frame_ms: History,
    /// Cached p95 frame time (ms), updated at SAMPLE_INTERVAL to avoid flicker.
    frame_p95: f64,
    /// Cached FPS (inverse of frame_p95), updated at SAMPLE_INTERVAL.
    fps_p95: f64,
    bw: History,
    msg: History,
}

impl Default for MetricsHistory {
    fn default() -> Self {
        Self {
            timer: 0.0,
            frame_window: FrameTimeWindow::new(2.0),
            frame_ms: History::new(),
            frame_p95: 0.0,
            fps_p95: 0.0,
            bw: History::new(),
            msg: History::new(),
        }
    }
}

/// Samples current metric values into rolling histories.
/// Raw frame time is pushed every frame; sparkline histories at SAMPLE_INTERVAL.
pub fn sample_metrics(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    network: Res<NetworkMetrics>,
    mut history: ResMut<MetricsHistory>,
) {
    // Every frame: push raw frame time into the 2s p95 window
    let elapsed = time.elapsed_secs_f64();
    if let Some(ft) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.value())
    {
        history.frame_window.push(elapsed, ft);
    }

    // Periodic: push to sparkline histories
    history.timer += time.delta_secs();
    if history.timer < SAMPLE_INTERVAL {
        return;
    }
    history.timer -= SAMPLE_INTERVAL;

    let p95 = history.frame_window.p95();
    history.frame_p95 = p95;
    history.fps_p95 = if p95 > 0.0 { 1000.0 / p95 } else { 0.0 };
    history.frame_ms.push(p95);
    history.bw.push(network.displayed_bytes_per_sec() as f64);
    history.msg.push(network.displayed_messages_per_sec() as f64);
}

// ── Alarm bands (match server console pattern) ──

struct Alarm {
    bands: &'static [(f64, egui::Color32)],
}

impl Alarm {
    fn color(&self, val: f64) -> egui::Color32 {
        for &(threshold, color) in self.bands {
            if val < threshold {
                return color;
            }
        }
        self.bands.last().map_or(COLOR_NORMAL, |&(_, c)| c)
    }
}

const ALARM_FPS: Alarm = Alarm {
    bands: &[
        (60.0, COLOR_CRITICAL),  // <60fps red
        (144.0, COLOR_WARN),     // <144fps yellow
        (f64::INFINITY, COLOR_NORMAL), // ≥144fps green
    ],
};

const ALARM_FRAME: Alarm = Alarm {
    bands: &[
        (6.944, COLOR_NORMAL),   // <6.944ms = ≥144fps green
        (16.667, COLOR_WARN),    // <16.667ms = ≥60fps yellow
        (f64::INFINITY, COLOR_CRITICAL), // ≥16.667ms = <60fps red
    ],
};

const ALARM_BW: Alarm = Alarm {
    bands: &[
        (15360.0, COLOR_NORMAL),        // <15KB/s green
        (20480.0, COLOR_WARN),          // <20KB/s yellow
        (f64::INFINITY, COLOR_CRITICAL), // ≥20KB/s red
    ],
};

const ALARM_MSG: Alarm = Alarm {
    bands: &[
        (15.0, COLOR_NORMAL),           // <15/s green
        (20.0, COLOR_WARN),             // <20/s yellow
        (f64::INFINITY, COLOR_CRITICAL), // ≥20/s red
    ],
};

// ── Sparkline scale ──

enum SparkScale {
    Fixed(f32),
}

// ── Display width ──

/// Display cell count for a string, accounting for known wide chars in Iosevka.
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if matches!(c, '△' | '●' | '○' | '↑' | '↓' | '↔') { 2 } else { 1 })
        .sum()
}

// ── Tile-width segment primitives ──
//
// Each function takes a pre-built string and asserts its display width matches the slot.
// Callers use standard format!() for layout; wide chars (↑, ↔, △ etc.) are 2 display
// cells but 1 Rust char — account for them at the call site, not here.

/// Segment row builder — wraps `ui.horizontal`, auto-inserts 1-char gaps between segments.
struct Seg<'a> {
    ui: &'a mut egui::Ui,
    cw: f32,
    count: u32,
}

impl<'a> Seg<'a> {
    fn emit(&mut self, s: &str, expected_width: usize, color: egui::Color32) {
        debug_assert_eq!(display_width(s), expected_width,
            "segment: {expected_width} cells expected, got {} for {:?}", display_width(s), s);
        if self.count > 0 { self.ui.add_space(self.cw); }
        self.ui.label(colored_mono(s, color));
        self.count += 1;
    }

    #[allow(dead_code)]
    fn full(&mut self, s: &str, color: egui::Color32) { self.emit(s, SEG_WIDTH, color); }
    fn half(&mut self, s: &str, color: egui::Color32) { self.emit(s, 7, color); }
    #[allow(dead_code)]
    fn quarter(&mut self, s: &str, color: egui::Color32) { self.emit(s, 3, color); }

    fn spark(&mut self, history: &[f32], scale: SparkScale, alarm: &Alarm, rh: f32) {
        if self.count > 0 { self.ui.add_space(self.cw); }
        seg_spark(self.ui, history, scale, alarm, self.cw, rh);
        self.count += 1;
    }
}

fn seg_row(ui: &mut egui::Ui, cw: f32, f: impl FnOnce(&mut Seg)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut seg = Seg { ui, cw, count: 0 };
        f(&mut seg);
    });
}

fn draw_sparkline(
    ui: &mut egui::Ui,
    history: &[f32],
    scale: SparkScale,
    alarm: &Alarm,
    char_width: f32,
    row_height: f32,
) {
    let width = SPARKLINE_CHARS as f32 * char_width;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COLOR_SPARK_BG);

    if history.is_empty() {
        return;
    }
    let bar_count = (width / MIN_BAR_WIDTH_PX).floor() as usize;
    if bar_count == 0 {
        return;
    }

    let n = history.len();
    let start = n.saturating_sub(bar_count);
    let samples = &history[start..];

    let max_val = match scale {
        SparkScale::Fixed(v) => v,
    };
    if max_val <= 0.0 {
        return;
    }
    let bar_width = width / bar_count as f32;
    let offset = bar_count - samples.len();

    for (i, &val) in samples.iter().enumerate() {
        let t = (val / max_val).clamp(0.0, 1.0);
        let bar_height = t * rect.height();
        if bar_height < 0.5 {
            continue;
        }
        let x = rect.left() + (offset + i) as f32 * bar_width;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - bar_height),
            egui::vec2(bar_width, bar_height),
        );
        let color = alarm.color(val as f64);
        painter.rect_filled(bar_rect, 0.0, color);
    }
}

fn seg_spark(
    ui: &mut egui::Ui,
    history: &[f32],
    scale: SparkScale,
    alarm: &Alarm,
    char_width: f32,
    row_height: f32,
) {
    draw_sparkline(ui, history, scale, alarm, char_width, row_height);
}


// ── Section rendering ──

fn draw_section<F: FnOnce(&mut egui::Ui)>(
    ui: &mut egui::Ui,
    label: &str,
    content_width: f32,
    content: F,
) {
    ui.label(colored_mono(label, COLOR_BORDER));
    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, COLOR_BORDER))
        .inner_margin(SECTION_INNER_MARGIN)
        .show(ui, |ui| {
            ui.set_min_width(content_width);
            content(ui);
        });
}


// ── Overlay camera ──

#[derive(Component)]
pub struct OverlayCamera;

#[derive(Resource)]
pub struct OverlayCameraEntity(pub Entity);

pub fn setup_overlay_camera(mut commands: Commands) {
    let entity = commands
        .spawn((
            OverlayCamera,
            Camera2d,
            Camera {
                order: 100,
                ..default()
            },
            EguiContext::default(),
        ))
        .id();
    commands.insert_resource(OverlayCameraEntity(entity));
}

// ── Font setup ──

pub fn setup_overlay_font(mut contexts: EguiContexts, overlay: Res<OverlayCameraEntity>) {
    let font_bytes = include_bytes!("../../../../../assets/fonts/Iosevka-Regular.ttc");
    let Ok(ctx) = contexts.ctx_for_entity_mut(overlay.0) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Iosevka".to_owned(),
        egui::FontData::from_static(font_bytes).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "Iosevka".to_owned());
    ctx.set_fonts(fonts);
}

// ── Main render system ──

#[allow(clippy::too_many_arguments)]
pub fn update_metrics_overlay(
    state: Res<DiagnosticsState>,
    mut contexts: EguiContexts,
    overlay: Res<OverlayCameraEntity>,
    mut camera_q: Query<&mut Camera, (With<Camera3d>, Without<OverlayCamera>)>,
    windows: Query<&Window>,
    diagnostics: Res<DiagnosticsStore>,
    map: Res<Map>,
    network: Res<NetworkMetrics>,
    history: Res<MetricsHistory>,
    player_q: Query<
        (&Transform, Option<&Loc>),
        (With<Actor>, With<PlayerControlled>, Without<Camera3d>),
    >,
    loaded_chunks: Res<LoadedChunks>,
    lod_meshes: Res<ChunkLodMeshes>,
    tri_stats: Res<crate::resources::LodTriangleStats>,
    chunk_mesh_q: Query<&ChunkMesh>,
    #[cfg(feature = "admin")] flyover: Res<crate::systems::admin::FlyoverState>,
) {
    if !state.metrics_overlay_visible {
        if let Ok(mut camera) = camera_q.single_mut() {
            if camera.viewport.is_some() {
                camera.viewport = None;
            }
        }
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok(ctx) = contexts.ctx_for_entity_mut(overlay.0) else {
        return;
    };

    // ── Measure font metrics ──
    let font = mono_font();
    let cw = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
    let rh = ctx.fonts_mut(|f| f.row_height(&font));

    // ── Panel pixel width ──
    let content_width = PANEL_CHARS as f32 * cw;
    let section_frame_overhead = (1.0 + SECTION_INNER_MARGIN) * 2.0;
    let panel_pixel_width = content_width + section_frame_overhead + OUTER_MARGIN * 2.0;

    // ── Camera viewport (16:9 in remaining space) ──
    let window_width = window.resolution.physical_width();
    let window_height = window.resolution.physical_height();
    let scale_factor = window.resolution.scale_factor();

    let panel_physical_width = (panel_pixel_width * scale_factor) as u32;
    let camera_available_width = window_width.saturating_sub(panel_physical_width);

    let mut cam_w = camera_available_width;
    let mut cam_h = (cam_w * 9) / 16;
    if cam_h > window_height {
        cam_h = window_height;
        cam_w = (cam_h * 16) / 9;
    }

    if let Ok(mut camera) = camera_q.single_mut() {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::new(cam_w, cam_h),
            ..default()
        });
    }

    // ── Sparkline bar count (matches visible window for stats) ──
    let spark_width = SPARKLINE_CHARS as f32 * cw;
    let bar_count = (spark_width / MIN_BAR_WIDTH_PX).floor() as usize;

    // ── Collect data ──

    let world_pos: Option<Vec3> = {
        #[cfg(feature = "admin")]
        {
            if flyover.active {
                Some(flyover.world_position)
            } else {
                player_q.single().ok().map(|(t, _)| t.translation)
            }
        }
        #[cfg(not(feature = "admin"))]
        {
            player_q.single().ok().map(|(t, _)| t.translation)
        }
    };

    let player_loc = player_q.single().ok().and_then(|(_, loc)| loc.copied());

    let tile_data = world_pos.map(|pos| {
        let qrz: qrz::Qrz = map.convert(pos);
        let z = map
            .get_by_qr(qrz.q, qrz.r)
            .map(|(real, _)| real.z)
            .unwrap_or(qrz.z);
        let qf = qrz.q as f64;
        let rf = qrz.r as f64;
        let wx = qf + rf * 0.5;
        let wy = rf * 1.7320508075688772 / 2.0;
        (qrz, z, wx, wy)
    });


    let full_count = chunk_mesh_q.iter().count();
    let pending_lod = lod_meshes.states.values()
        .filter(|s| s.lod1_task.is_some() || s.lod2_task.is_some()).count();

    // Triangle stats (ratio, max_error) + per-level totals
    let (lod1_ratio, lod1_err) = tri_stats.lod1_stats();
    let (lod2_ratio, lod2_err) = tri_stats.lod2_stats();
    let inner_r = common_bevy::chunk::detail_boundary_radius(0, common_bevy::chunk::MAX_FOV);
    let outer_r = terrain_chunk_radius(player_loc.map(|l| l.z).unwrap_or(0));
    let (lod1_tris, lod2_tris) = lod_meshes.states.values()
        .filter(|s| s.entity.is_some())
        .fold((0u64, 0u64), |(l1, l2), s| match s.active_lod {
            crate::resources::LodLevel::Lod1 => (l1 + s.lod1_tris as u64, l2),
            crate::resources::LodLevel::Lod2 => (l1, l2 + s.lod2_tris as u64),
        });

    #[cfg(feature = "admin")]
    let (admin_count, admin_sum_count) = if flyover.active {
        (
            flyover.admin_chunks.len(),
            flyover.admin_summary_chunks.len(),
        )
    } else {
        (0, 0)
    };
    #[cfg(not(feature = "admin"))]
    let (admin_count, admin_sum_count) = (0usize, 0usize);


    let frame_p95 = history.frame_p95;
    let fps_p95 = history.fps_p95;
    let entities = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let bps = network.displayed_bytes_per_sec() as f64;
    let mps = network.displayed_messages_per_sec() as f64;

    // Pre-compute sparkline data
    let hist_frame = history.frame_ms.as_f32();
    let hist_bw = history.bw.as_f32();
    let hist_msg = history.msg.as_f32();

    use numfmt::{NumFmt, Precision, Overflow};

    // ── Render egui panel ──
    let window_logical_width = window_width as f32 / scale_factor;
    let window_logical_height = window_height as f32 / scale_factor;
    let panel_x = window_logical_width - panel_pixel_width;


    egui::Area::new(egui::Id::new("metrics_overlay"))
        .fixed_pos(egui::pos2(panel_x, 0.0))
        .show(ctx, |ui| {
            let bg_rect = egui::Rect::from_min_size(
                egui::pos2(panel_x, 0.0),
                egui::vec2(panel_pixel_width, window_logical_height),
            );
            ui.painter().rect_filled(bg_rect, 0.0, COLOR_BG);

            egui::Frame::NONE
                .inner_margin(OUTER_MARGIN)
                .show(ui, |ui| {
                    // ── TERRAIN ──
                    draw_section(ui, "TERRAIN", content_width, |ui| {
                        let (q, r, z, wx, wy) = tile_data
                            .map(|(qrz, z, wx, wy)| (qrz.q as f64, qrz.r as f64, z as f64, wx, wy))
                            .unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));
                        // qrz/xy — two 23ch super-segments (23 + 1 gap + 23 = 47)
                        const COORD: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Clamp };
                        let fc = |v: f64| COORD.fmt(v);
                        let qrz = format!("qrz:{},{},{}", fc(q), fc(r), fc(z));
                        let xy = format!("xy:{},{}", fc(wx), fc(wy));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.label(colored_mono(&format!("{:^23}", qrz), COLOR_DIM));
                            ui.add_space(cw);
                            ui.label(colored_mono(&format!("{:^23}", xy), COLOR_DIM));
                        });
                        // mesh/load/pending
                        const CHUNK_CT: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "mesh"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", CHUNK_CT.fmt(full_count as f64)), COLOR_DIM);
                            s.half(&format!("{:>7}", "load"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", CHUNK_CT.fmt(loaded_chunks.chunks.len() as f64)), COLOR_DIM);
                            s.half(&format!("{:>7}", "pend"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", CHUNK_CT.fmt(pending_lod as f64)), COLOR_DIM);
                        });
                        // LoD lines: 5 half-segments each
                        const LOD_TRIS: NumFmt = NumFmt { width: 5, precision: Precision::Fixed(2), overflow: Overflow::Suffix };
                        const LOD_RADIUS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        const LOD_RATIO: NumFmt = NumFmt { width: 6, precision: Precision::Fixed(2), overflow: Overflow::Clamp };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "LoD1"), COLOR_DIM);
                            s.half(&format!("△{:<5}", LOD_TRIS.fmt(lod1_tris as f64)), COLOR_DIM);
                            s.half(&format!("↔{:<5}", LOD_RADIUS.fmt(inner_r as f64)), COLOR_DIM);
                            s.half(&format!("×{:<6}", LOD_RATIO.fmt(lod1_ratio)), COLOR_DIM);
                            s.half(&format!("ε{:<6}", LOD_RATIO.fmt(lod1_err)), COLOR_DIM);
                        });
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "LoD2"), COLOR_DIM);
                            s.half(&format!("△{:<5}", LOD_TRIS.fmt(lod2_tris as f64)), COLOR_DIM);
                            s.half(&format!("↔{:<5}", LOD_RADIUS.fmt(outer_r as f64)), COLOR_DIM);
                            s.half(&format!("×{:<6}", LOD_RATIO.fmt(lod2_ratio)), COLOR_DIM);
                            s.half(&format!("ε{:<6}", LOD_RATIO.fmt(lod2_err)), COLOR_DIM);
                        });
                        if admin_count > 0 || admin_sum_count > 0 {
                            const ADMIN_CT: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                            seg_row(ui, cw, |s| {
                                s.half(&format!("{:>7}", "aChk"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ADMIN_CT.fmt(admin_count as f64)), COLOR_DIM);
                                s.half(&format!("{:>7}", "aSum"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ADMIN_CT.fmt(admin_sum_count as f64)), COLOR_DIM);
                            });
                        }
                    });

                    ui.add_space(4.0);

                    // ── RENDER ──
                    draw_section(ui, "RENDER", content_width, |ui| {
                        const FRAME_MS: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
                        const FPS: NumFmt = NumFmt { width: 3, precision: Precision::Integer, overflow: Overflow::Clamp };
                        let frame_peak = history.frame_ms.visible_max(bar_count);
                        let peak_v = FRAME_MS.fmt(frame_peak);
                        let fps_v = FPS.fmt(fps_p95);
                        let fps_color = ALARM_FPS.color(fps_p95);
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "FRAME"), COLOR_DIM);
                            s.half(&format!("{:>5}{:<2}", FRAME_MS.fmt(frame_p95), "ms"), COLOR_DIM);
                            s.spark(&hist_frame, SparkScale::Fixed(33.0), &ALARM_FRAME, rh);
                            s.half(&format!("↑{:<5}", peak_v), COLOR_DIM);
                            s.half(&format!("ƒ{:<6}", fps_v), fps_color);
                        });
                        const ENTS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        const TILES: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "ENTS"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", ENTS.fmt(entities)), COLOR_DIM);
                            s.half(&format!("{:>7}", "TILES"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", TILES.fmt(map.len() as f64)), COLOR_DIM);
                        });
                    });

                    ui.add_space(4.0);

                    // ── NETWORK ──
                    draw_section(ui, "NETWORK", content_width, |ui| {
                        const NET_BPS: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("↓NET  "), COLOR_DIM);
                            s.half(&format!("{:>4}{:<3}", NET_BPS.fmt(bps), "B/s"), COLOR_DIM);
                            s.spark(&hist_bw, SparkScale::Fixed(40960.0), &ALARM_BW, rh);
                            let pv = NET_BPS.fmt(history.bw.visible_max(bar_count));
                            s.half(&format!("↑{:<5}", pv), COLOR_DIM);
                        });
                        const NET_MPS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("↓MSG  "), COLOR_DIM);
                            s.half(&format!("{:>5}{:<2}", NET_MPS.fmt(mps), "/s"), COLOR_DIM);
                            s.spark(&hist_msg, SparkScale::Fixed(40.0), &ALARM_MSG, rh);
                            let pv = NET_MPS.fmt(history.msg.visible_max(bar_count));
                            s.half(&format!("↑{:<5}", pv), COLOR_DIM);
                        });
                    });
                });
        });
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use numfmt::{NumFmt, Precision, Overflow};

    const DEC5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
    const INT5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };

    #[test]
    fn format_value_max_width() {
        let cases: &[f64] = &[
            0.0, 0.043, 0.5, 1.23, 9.99, 10.0, 92.6, 99.9, 100.0, 714.0, 999.0, 1234.0,
            9999.0, 25148.0, 999999.0, 1_234_567.0,
            -0.5, -1.23, -9.99, -92.6, -714.0, -5678.0, -99999.0,
        ];
        for &v in cases {
            let s = DEC5.fmt(v);
            assert!(s.len() <= 5, "DEC5.fmt({}) = {:?} (len {})", v, s, s.len());
        }
    }

    #[test]
    fn format_int_max_width() {
        let cases: &[f64] = &[
            0.0, 1.0, 42.0, 999.0, 12345.0, 99999.0, 100_000.0, 999_999.0, 1_000_000.0,
            -1.0, -42.0, -999.0, -5678.0, -99999.0,
        ];
        for &v in cases {
            let s = INT5.fmt(v);
            assert!(s.len() <= 5, "INT5.fmt({}) = {:?} (len {})", v, s, s.len());
        }
    }

    #[test]
    fn format_value_known_outputs() {
        assert_eq!(DEC5.fmt(0.0), "0.00");
        assert_eq!(DEC5.fmt(0.5), "0.50");
        assert_eq!(DEC5.fmt(92.6), "92.60");
        assert_eq!(DEC5.fmt(714.0), "714.0");
        assert_eq!(DEC5.fmt(1234.0), "1234");
        assert_eq!(DEC5.fmt(9999.0), "9999");
        assert_eq!(DEC5.fmt(10_000.0), "10.0K");
        assert_eq!(DEC5.fmt(25148.0), "25.1K");
        assert_eq!(DEC5.fmt(100_000.0), "100K");
        assert_eq!(DEC5.fmt(1_234_567.0), "1.23M");
        assert_eq!(DEC5.fmt(-50.3), "-50.3");
        assert_eq!(DEC5.fmt(-714.0), "-714");
        assert_eq!(DEC5.fmt(-5678.0), "-5678");
        assert_eq!(DEC5.fmt(-56789.0), "-57K");
    }

    #[test]
    fn format_int_known_outputs() {
        assert_eq!(INT5.fmt(0.0), "0");
        assert_eq!(INT5.fmt(42.0), "42");
        assert_eq!(INT5.fmt(999.0), "999");
        assert_eq!(INT5.fmt(1000.0), "1000");
        assert_eq!(INT5.fmt(1500.0), "1500");
        assert_eq!(INT5.fmt(9999.0), "9999");
        assert_eq!(INT5.fmt(10_000.0), "10.0K");
        assert_eq!(INT5.fmt(25000.0), "25.0K");
        assert_eq!(INT5.fmt(100_000.0), "100K");
        assert_eq!(INT5.fmt(-999.0), "-999");
        assert_eq!(INT5.fmt(-5678.0), "-5678");
        assert_eq!(INT5.fmt(-10_000.0), "-10K");
    }

    #[test]
    fn panel_chars_matches_three_segments() {
        assert_eq!(PANEL_CHARS, 3 * 15 + 2 * 1);
        assert_eq!(PANEL_CHARS, 47);
    }

    #[test]
    fn history_rolling_window() {
        let mut h = History::new();
        for i in 0..150 {
            h.push(i as f64);
        }
        assert_eq!(h.0.len(), HISTORY_LEN);
        assert_eq!(*h.0.back().unwrap(), 149.0);
        assert_eq!(*h.0.front().unwrap(), 30.0);
    }

    #[test]
    fn history_visible_max() {
        let mut h = History::new();
        for v in [1.0, 5.0, 3.0, 8.0, 2.0] {
            h.push(v);
        }
        assert_eq!(h.visible_max(3), 8.0); // last 3: 3.0, 8.0, 2.0
        assert_eq!(h.visible_max(5), 8.0);
        assert_eq!(h.visible_max(1), 2.0);
    }


}
