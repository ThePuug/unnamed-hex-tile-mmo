use std::collections::VecDeque;

use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_camera::Viewport;
use bevy_egui::{egui, EguiContext, EguiContexts};

use super::config::DiagnosticsState;
use super::network_ui::NetworkMetrics;
use common_bevy::{
    chunk::{elevation_chunk_radius_raw, terrain_chunk_radius, CHUNK_TILES},
    components::{behaviour::PlayerControlled, Actor, Loc},
    resources::map::Map,
};
use qrz::Convert;

use crate::components::{ChunkMesh, SummaryChunk};
use crate::resources::{LoadedChunks, PendingChunkMeshes, PendingSummaryMeshes};

// ── Layout constants (match server console) ──

const SEG_WIDTH: usize = 14;
const SEG_GAP: usize = 1;
const PANEL_CHARS: usize = 3 * SEG_WIDTH + 2 * SEG_GAP;
const SPARKLINE_CHARS: usize = 14;
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

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

// ── Pure formatting (duplicated from crates/console/src/main.rs) ──

/// Always returns exactly 5 characters (fractional with scale suffix).
fn format_value(v: f64) -> String {
    if v == 0.0 {
        "    0".into()
    } else if v.abs() < 0.001 {
        format!("{:>5.0}", v)
    } else if v < 10.0 {
        format!("{:>5.2}", v)
    } else if v < 100.0 {
        format!("{:>5.1}", v)
    } else if v < 1000.0 {
        format!("{:>5.0}", v)
    } else if v < 10_000.0 {
        format!("{:>4.1}K", v / 1_000.0)
    } else if v < 1_000_000.0 {
        format!("{:>4.0}K", v / 1_000.0)
    } else if v < 10_000_000.0 {
        format!("{:>4.1}M", v / 1_000_000.0)
    } else if v < 1_000_000_000.0 {
        format!("{:>4.0}M", v / 1_000_000.0)
    } else if v < 10_000_000_000.0 {
        format!("{:>4.1}B", v / 1_000_000_000.0)
    } else if v < 1_000_000_000_000.0 {
        format!("{:>4.0}B", v / 1_000_000_000.0)
    } else {
        format!("{:>4.1}T", v / 1_000_000_000_000.0)
    }
}

/// Always returns exactly 5 characters (integer, no decimals).
fn format_int(v: f64) -> String {
    let v = v.round();
    if v == 0.0 {
        "    0".into()
    } else if v.abs() < 100_000.0 {
        format!("{:>5.0}", v)
    } else if v.abs() < 10_000_000.0 {
        format!("{:>4.0}K", v / 1_000.0)
    } else if v.abs() < 10_000_000_000.0 {
        format!("{:>4.0}M", v / 1_000_000.0)
    } else if v.abs() < 10_000_000_000_000.0 {
        format!("{:>4.0}B", v / 1_000_000_000.0)
    } else {
        format!("{:>4.0}T", v / 1_000_000_000_000.0)
    }
}

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

/// Accumulated metric histories, sampled at SAMPLE_INTERVAL.
#[derive(Resource)]
pub struct MetricsHistory {
    timer: f32,
    fps: History,
    frame_ms: History,
    bw: History,
    msg: History,
}

impl Default for MetricsHistory {
    fn default() -> Self {
        Self {
            timer: 0.0,
            fps: History::new(),
            frame_ms: History::new(),
            bw: History::new(),
            msg: History::new(),
        }
    }
}

/// Samples current metric values into rolling histories.
pub fn sample_metrics(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    network: Res<NetworkMetrics>,
    mut history: ResMut<MetricsHistory>,
) {
    history.timer += time.delta_secs();
    if history.timer < SAMPLE_INTERVAL {
        return;
    }
    history.timer -= SAMPLE_INTERVAL;

    if let Some(v) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    {
        history.fps.push(v);
    }
    if let Some(v) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
    {
        history.frame_ms.push(v);
    }
    history.bw.push(network.displayed_bytes_per_sec() as f64);
    history.msg.push(network.displayed_messages_per_sec() as f64);
}

// ── Alarm bands (match server console pattern) ──

struct Alarm {
    bands: &'static [(f64, egui::Color32)],
}

impl Alarm {
    const NONE: Self = Self {
        bands: &[(f64::INFINITY, COLOR_NORMAL)],
    };

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
        (30.0, COLOR_CRITICAL),
        (55.0, COLOR_WARN),
        (f64::INFINITY, COLOR_NORMAL),
    ],
};

const ALARM_FRAME: Alarm = Alarm {
    bands: &[
        (17.0, COLOR_NORMAL),
        (33.0, COLOR_WARN),
        (f64::INFINITY, COLOR_CRITICAL),
    ],
};

// ── Sparkline scale ──

enum SparkScale {
    Fixed(f32),
    Auto,
}

// ── Sym (stat symbol width) ──

#[derive(Clone, Copy)]
enum Sym {
    Narrow(char),
    Wide(char),
}

impl Sym {
    fn cells(self) -> usize {
        match self {
            Sym::Narrow(_) => 1,
            Sym::Wide(_) => 2,
        }
    }
    fn as_str(self) -> String {
        match self {
            Sym::Narrow(c) | Sym::Wide(c) => c.to_string(),
        }
    }
}

const SYM_UP: Sym = Sym::Wide('↑');

// ── Composable segments (match server console) ──

fn seg_label(ui: &mut egui::Ui, label: &str, value: &str, unit: &str) {
    ui.label(colored_mono(
        &format!("{:>5} {} {:<2}", label, value, unit),
        COLOR_DIM,
    ));
}

fn seg_gap(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(char_width);
}

fn draw_sparkline(
    ui: &mut egui::Ui,
    history: &[f32],
    scale: SparkScale,
    fixed_color: Option<egui::Color32>,
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
        SparkScale::Auto => samples.iter().cloned().fold(0.0_f32, f32::max),
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
        let color = fixed_color.unwrap_or_else(|| lerp_color(COLOR_DIM, COLOR_CRITICAL, t));
        painter.rect_filled(bar_rect, 0.0, color);
    }
}

fn seg_spark(
    ui: &mut egui::Ui,
    history: &[f32],
    scale: SparkScale,
    fixed_color: Option<egui::Color32>,
    char_width: f32,
    row_height: f32,
) {
    draw_sparkline(ui, history, scale, fixed_color, char_width, row_height);
}

/// Single stat: symbol + 5-char value, with alarm coloring.
/// Pads to 14 chars total when only one stat is shown.
fn seg_stats(ui: &mut egui::Ui, a: Option<(Sym, f64, &Alarm)>) {
    let (text, color) = match a {
        Some((sym, val, alarm)) => {
            let formatted = format!("{:<5}", format_value(val).trim_start());
            (format!("{}{}", sym.as_str(), formatted), alarm.color(val))
        }
        None => (String::new(), COLOR_DIM),
    };
    let width = a.map_or(0, |(s, _, _)| s.cells() + 5);
    let pad = 14usize.saturating_sub(width);
    let padded = format!("{}{}", text, " ".repeat(pad));
    ui.label(colored_mono(&padded, color));
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

/// Label-only row: 1–3 seg_labels with gaps.
fn metric_row(ui: &mut egui::Ui, cw: f32, segments: &[(&str, &str, &str)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, &(label, value, unit)) in segments.iter().enumerate() {
            if i > 0 {
                seg_gap(ui, cw);
            }
            seg_label(ui, label, value, unit);
        }
    });
}

/// Full sparkline row: seg_label + seg_gap + seg_spark + seg_gap + seg_stats.
fn spark_row(
    ui: &mut egui::Ui,
    cw: f32,
    rh: f32,
    label: &str,
    value: &str,
    unit: &str,
    history: &[f32],
    scale: SparkScale,
    peak: f64,
    alarm: &Alarm,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        seg_label(ui, label, value, unit);
        seg_gap(ui, cw);
        seg_spark(ui, history, scale, None, cw, rh);
        seg_gap(ui, cw);
        seg_stats(ui, Some((SYM_UP, peak, alarm)));
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
    pending_meshes: Res<PendingChunkMeshes>,
    pending_summary: Res<PendingSummaryMeshes>,
    chunk_mesh_q: Query<(&ChunkMesh, Option<&SummaryChunk>)>,
    #[cfg(feature = "admin")] flyover: Res<crate::systems::admin::FlyoverState>,
    #[cfg(feature = "admin")] admin_terrain: Res<crate::systems::admin::AdminTerrain>,
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

    #[cfg(feature = "admin")]
    let raw_elevation =
        tile_data.map(|(qrz, _, _, _)| admin_terrain.0.get_raw_elevation(qrz.q, qrz.r));

    let mut full_count = 0usize;
    let mut summary_count = 0usize;
    let mut orphan_count = 0usize;
    for (cm, summary) in chunk_mesh_q.iter() {
        if summary.is_some() {
            summary_count += 1;
        } else {
            full_count += 1;
            #[allow(unused_mut)]
            let mut tracked = loaded_chunks.chunks.contains(&cm.chunk_id);
            #[cfg(feature = "admin")]
            {
                if !tracked && flyover.active {
                    tracked = flyover.admin_chunks.contains(&cm.chunk_id);
                }
            }
            if !tracked {
                orphan_count += 1;
            }
        }
    }

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

    let vis_data = player_loc.map(|loc| {
        let base = terrain_chunk_radius(loc.z);
        let max = elevation_chunk_radius_raw(loc.z);
        let chunk_wu = (CHUNK_TILES as f32).sqrt() * 1.5;
        (base, max, max as f32 * chunk_wu)
    });

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let entities = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let bps = network.displayed_bytes_per_sec() as f64;
    let mps = network.displayed_messages_per_sec() as f64;

    // Pre-compute sparkline data
    let hist_fps = history.fps.as_f32();
    let hist_frame = history.frame_ms.as_f32();
    let hist_bw = history.bw.as_f32();
    let hist_msg = history.msg.as_f32();

    // ── Render egui panel ──
    let window_logical_width = window_width as f32 / scale_factor;
    let window_logical_height = window_height as f32 / scale_factor;
    let panel_x = window_logical_width - panel_pixel_width;

    let fv = format_value;
    let fi = format_int;

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
                        if let Some((qrz, z, wx, wy)) = tile_data {
                            metric_row(ui, cw, &[
                                ("q", &fi(qrz.q as f64), "n"),
                                ("r", &fi(qrz.r as f64), "n"),
                                ("z", &fi(z as f64), "n"),
                            ]);
                            #[cfg(feature = "admin")]
                            if let Some(wz) = raw_elevation {
                                metric_row(ui, cw, &[
                                    ("wx", &fv(wx), "n"),
                                    ("wy", &fv(wy), "n"),
                                    ("wz", &fv(wz), "n"),
                                ]);
                            }
                        } else {
                            metric_row(ui, cw, &[
                                ("q", &fi(0.0), "n"),
                                ("r", &fi(0.0), "n"),
                                ("z", &fi(0.0), "n"),
                            ]);
                        }
                        metric_row(ui, cw, &[
                            ("mesh", &fi(full_count as f64), "n"),
                            ("sum", &fi(summary_count as f64), "n"),
                        ]);
                        metric_row(ui, cw, &[
                            ("pend", &fi(pending_meshes.tasks.len() as f64), "n"),
                            ("psum", &fi(pending_summary.tasks.len() as f64), "n"),
                        ]);
                        metric_row(ui, cw, &[
                            ("trck", &fi(loaded_chunks.chunks.len() as f64), "n"),
                            ("orph", &fi(orphan_count as f64), "n"),
                        ]);
                        if admin_count > 0 || admin_sum_count > 0 {
                            metric_row(ui, cw, &[
                                ("aChk", &fi(admin_count as f64), "n"),
                                ("aSum", &fi(admin_sum_count as f64), "n"),
                            ]);
                        }
                        if let Some((base, max, wu)) = vis_data {
                            metric_row(ui, cw, &[
                                ("vBas", &fi(base as f64), "ch"),
                                ("vMax", &fi(max as f64), "ch"),
                                ("vWu", &fv(wu as f64), "wu"),
                            ]);
                        }
                    });

                    ui.add_space(4.0);

                    // ── RENDER ──
                    draw_section(ui, "RENDER", content_width, |ui| {
                        spark_row(
                            ui, cw, rh, "FPS", &fv(fps), "hz",
                            &hist_fps, SparkScale::Auto,
                            history.fps.visible_max(bar_count), &ALARM_FPS,
                        );
                        spark_row(
                            ui, cw, rh, "FRAME", &fv(frame_ms), "ms",
                            &hist_frame, SparkScale::Fixed(33.0),
                            history.frame_ms.visible_max(bar_count), &ALARM_FRAME,
                        );
                        metric_row(ui, cw, &[
                            ("ENTS", &fi(entities), "n"),
                            ("TILES", &fi(map.len() as f64), "n"),
                        ]);
                    });

                    ui.add_space(4.0);

                    // ── NETWORK ──
                    draw_section(ui, "NETWORK", content_width, |ui| {
                        spark_row(
                            ui, cw, rh, "BW", &fv(bps), "Bs",
                            &hist_bw, SparkScale::Auto,
                            history.bw.visible_max(bar_count), &Alarm::NONE,
                        );
                        spark_row(
                            ui, cw, rh, "MSG", &fv(mps), "/s",
                            &hist_msg, SparkScale::Auto,
                            history.msg.visible_max(bar_count), &Alarm::NONE,
                        );
                    });
                });
        });
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_value_five_chars() {
        let cases: &[f64] = &[
            0.0, 0.043, 0.5, 1.23, 9.99, 10.0, 92.6, 99.9, 100.0, 714.0, 999.0, 1234.0,
            9999.0, 25148.0, 999999.0, 1_234_567.0,
        ];
        for &v in cases {
            let s = format_value(v);
            assert_eq!(s.len(), 5, "format_value({}) = {:?} (len {})", v, s, s.len());
        }
    }

    #[test]
    fn format_int_five_chars() {
        let cases: &[f64] = &[
            0.0, 1.0, 42.0, 999.0, 12345.0, 99999.0, 100_000.0, 999_999.0, 1_000_000.0,
        ];
        for &v in cases {
            let s = format_int(v);
            assert_eq!(s.len(), 5, "format_int({}) = {:?} (len {})", v, s, s.len());
        }
    }

    #[test]
    fn format_value_known_outputs() {
        assert_eq!(format_value(0.0), "    0");
        assert_eq!(format_value(92.6), " 92.6");
        assert_eq!(format_value(714.0), "  714");
    }

    #[test]
    fn panel_chars_matches_three_segments() {
        assert_eq!(PANEL_CHARS, 3 * 14 + 2 * 1);
        assert_eq!(PANEL_CHARS, 44);
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

    #[test]
    fn alarm_band_evaluation() {
        assert_eq!(ALARM_FPS.color(60.0), COLOR_NORMAL);
        assert_eq!(ALARM_FPS.color(40.0), COLOR_WARN);
        assert_eq!(ALARM_FPS.color(20.0), COLOR_CRITICAL);
        assert_eq!(ALARM_FRAME.color(10.0), COLOR_NORMAL);
        assert_eq!(ALARM_FRAME.color(25.0), COLOR_WARN);
        assert_eq!(ALARM_FRAME.color(50.0), COLOR_CRITICAL);
    }
}
