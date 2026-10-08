use bevy::prelude::*;
use crate::systems::closeup::CloseupCamera;
use bevy_camera::Viewport;
use bevy_egui::{egui, EguiContexts};

use super::config::{DiagnosticsState, MetricsTab};
use super::feed::Feed;


// ── Layout constants (match server console) ──

const SEG_WIDTH: usize = 15;
const SEG_GAP: usize = 1;
const PANEL_CHARS: usize = 3 * SEG_WIDTH + 2 * SEG_GAP;
/// Two full segments and the gap between them: a name that needs the room.
const WIDE_WIDTH: usize = 2 * SEG_WIDTH + SEG_GAP;
const SPARKLINE_CHARS: usize = 15;
const MIN_BAR_WIDTH_PX: f32 = 2.0;

const FONT_SIZE: f32 = 12.0;
const OUTER_MARGIN: f32 = 8.0;
const SECTION_INNER_MARGIN: f32 = 4.0;

use common::glyphs::*;

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

/// A draw call costs the thread that encodes it, every frame, and a wood
/// that stands in thousands of them pays for every one.
const ALARM_DRAWS: Alarm = Alarm {
    bands: &[
        (500.0, COLOR_NORMAL),
        (2000.0, COLOR_WARN),
        (f64::INFINITY, COLOR_CRITICAL),
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

// ── Tile-width segment primitives ──

// Each function takes a pre-built string and asserts its char count matches the slot.
// Nerd Font glyphs use {:<2} for 2-cell width.

/// Segment row builder — wraps `ui.horizontal`, auto-inserts 1-char gaps between segments.
struct Seg<'a> {
    ui: &'a mut egui::Ui,
    cw: f32,
    count: u32,
}

impl<'a> Seg<'a> {
    fn emit(&mut self, s: &str, expected_chars: usize, color: egui::Color32) {
        debug_assert_eq!(s.chars().count(), expected_chars,
            "segment: {expected_chars} chars expected, got {} for {:?}", s.chars().count(), s);
        if self.count > 0 { self.ui.add_space(self.cw); }
        self.ui.label(colored_mono(s, color));
        self.count += 1;
    }

    #[allow(dead_code)]
    fn full(&mut self, s: &str, color: egui::Color32) { self.emit(s, SEG_WIDTH, color); }
    fn wide(&mut self, s: &str, color: egui::Color32) { self.emit(s, WIDE_WIDTH, color); }
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

/// The section a tab shows, drawn only when that tab is the one up.
fn tab_section<F: FnOnce(&mut egui::Ui)>(
    ui: &mut egui::Ui,
    up: MetricsTab,
    tab: MetricsTab,
    content_width: f32,
    content: F,
) {
    if up != tab {
        return;
    }
    draw_section(ui, tab.label(), content_width, content);
}

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

pub fn setup_overlay_camera(mut commands: Commands, mut egui: ResMut<bevy_egui::EguiGlobalSettings>) {
    // The overlay's camera is the one Egui draws on, so it is the primary
    // context and no other is made: two contexts sharing the pass panic.
    egui.auto_create_primary_context = false;
    let entity = commands
        .spawn((
            OverlayCamera,
            Camera2d,
            Camera {
                order: 100,
                // Drawn over the world's camera on the same window. Left to
                // clear, it wipes the world whenever MSAA is off: with it on,
                // the writeback marks the target cleared before this pass.
                clear_color: ClearColorConfig::None,
                ..default()
            },
            bevy_egui::PrimaryEguiContext,
        ))
        .id();
    commands.insert_resource(OverlayCameraEntity(entity));
}

// ── Font setup ──

pub fn setup_overlay_font(mut contexts: EguiContexts, overlay: Res<OverlayCameraEntity>) {
    let font_bytes = include_bytes!("../../../../../assets/fonts/IosevkaNerdFont-Regular.ttf");
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
    mut state: ResMut<DiagnosticsState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    overlay: Res<OverlayCameraEntity>,
    mut camera_q: Query<&mut Camera, (With<Camera3d>, Without<OverlayCamera>, Without<CloseupCamera>)>,
    windows: Query<&Window>,
    feed: Res<Feed>,
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

    // The brackets step the tab either way, for a hand already on the
    // keyboard; the strip under the view takes a click for one directly.
    let step = i32::from(keys.just_pressed(KeyCode::BracketRight)) - i32::from(keys.just_pressed(KeyCode::BracketLeft));
    if step != 0 {
        state.metrics_tab = state.metrics_tab.step(step);
    }
    let tab = state.metrics_tab;

    // ── Sparkline bar count (matches visible window for stats) ──
    let spark_width = SPARKLINE_CHARS as f32 * cw;
    let bar_count = (spark_width / MIN_BAR_WIDTH_PX).floor() as usize;

    // ── Collect data ──

    let frame_p95 = feed.latest("frame/p95_ms");
    let fps_p95 = feed.latest("frame/fps");

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
                    // ── FRAME ──
                    // Above every tab: the one number the rest explain.
                    draw_section(ui, "FRAME", content_width, |ui| {
                        const FRAME_MS: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
                        const FPS: NumFmt = NumFmt { width: 3, precision: Precision::Integer, overflow: Overflow::Clamp };
                        let frame_peak = feed.peak("frame/p95_ms", bar_count);
                        let peak_v = FRAME_MS.fmt(frame_peak);
                        let fps_v = FPS.fmt(fps_p95);
                        let fps_color = ALARM_FPS.color(fps_p95);
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "FRAME"), COLOR_DIM);
                            s.half(&format!("{:>5}{:<2}", FRAME_MS.fmt(frame_p95), "ms"), COLOR_DIM);
                            s.spark(feed.history("frame/p95_ms"), SparkScale::Fixed(33.0), &ALARM_FRAME, rh);
                            s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, peak_v), COLOR_DIM);
                            s.half(&format!("ƒ{:<6}", fps_v), fps_color);
                        });
                    });

                    ui.add_space(4.0);

                    // ── TERRAIN ──
                    tab_section(ui, tab, MetricsTab::Terrain, content_width, |ui| {
                        let [q, r, z, wx, wy] = ["q", "r", "z", "wx", "wy"].map(|f| feed.latest(&format!("world/{f}")));
                        const COORD: NumFmt = NumFmt { width: 7, precision: Precision::Integer, overflow: Overflow::Clamp };
                        seg_row(ui, cw, |s| {
                            s.quarter(&format!(" {:<2}", GLYPH_HEX), COLOR_DIM);
                            s.half(&format!("{:<7}", COORD.fmt(q)), COLOR_DIM);
                            s.half(&format!("{:<7}", COORD.fmt(r)), COLOR_DIM);
                            s.half(&format!("{:<7}", COORD.fmt(z)), COLOR_DIM);
                            s.quarter(&format!(" {:<2}", GLYPH_GRID), COLOR_DIM);
                            s.half(&format!("{:<7}", COORD.fmt(wx)), COLOR_DIM);
                            s.half(&format!("{:<7}", COORD.fmt(wy)), COLOR_DIM);
                        });
                        // Async task counts
                        const ASYNC_CT: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "async"), COLOR_DIM);
                            s.half(&format!("{:>4}msh", ASYNC_CT.fmt(feed.latest("terrain/async_mesh"))), COLOR_DIM);
                        });
                        // Hex-native LoD stats: per-tier breakdown
                        const LOD_TRIS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        const LOD_CT: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
                        // Total row
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "LoD"), COLOR_DIM);
                            s.half(&format!("{:<2}{:<5}", GLYPH_TRIANGLES, LOD_TRIS.fmt(feed.latest("terrain/tris"))), COLOR_DIM);
                            s.half(&format!("{:>4}chk", LOD_CT.fmt(feed.latest("terrain/chunks"))), COLOR_DIM);
                        });
                        // Per-band rows
                        let mut bands: Vec<u32> = feed.under("terrain/band/").filter_map(|(_, rest)| rest.strip_suffix("/tris")?.parse().ok()).collect();
                        bands.sort();
                        for r in bands {
                            let band_tris = feed.latest(&format!("terrain/band/{r}/tris"));
                            let band_count = feed.latest(&format!("terrain/band/{r}/chunks"));
                            seg_row(ui, cw, |s| {
                                s.half(&format!("  r={:<3}", r), COLOR_DIM);
                                s.half(&format!("{:<2}{:<5}", GLYPH_TRIANGLES, LOD_TRIS.fmt(band_tris)), COLOR_DIM);
                                s.half(&format!("{:>4}chk", LOD_CT.fmt(band_count)), COLOR_DIM);
                            });
                        }
                    });


                    // ── RENDER ──
                    tab_section(ui, tab, MetricsTab::Render, content_width, |ui| {
                        const ENTS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        const TILES: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "ENTS"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", ENTS.fmt(feed.latest("diag/entity_count"))), COLOR_DIM);
                            s.half(&format!("{:>7}", "TILES"), COLOR_DIM);
                            s.half(&format!("{:>5}  ", TILES.fmt(feed.latest("world/tiles"))), COLOR_DIM);
                        });
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:>7}", "MEM"), COLOR_DIM);
                            s.half(&format!("{:>5}MB", TILES.fmt(feed.latest("process/memory_mb"))), COLOR_DIM);
                        });
                        // The cover: what it draws, and what it draws in one
                        // go. A draw costs the thread that encodes it; an
                        // instance in it costs nothing more.
                        for (what, draws, instances) in [
                            ("MODELS", feed.latest("cover/model_draws"), feed.latest("cover/models")),
                            ("CARDS", feed.latest("cover/card_draws"), feed.latest("cover/cards")),
                        ] {
                            seg_row(ui, cw, |s| {
                                s.half(&format!("{what:>7}"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ENTS.fmt(instances)), COLOR_DIM);
                                s.half(&format!("{:>7}", "DRAWS"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ENTS.fmt(draws)), ALARM_DRAWS.color(draws));
                            });
                        }
                        // What each group hands the rasteriser. The
                        // triangles are what a pass is paid in; the draws
                        // beside them say whether they came in one go.
                        for (what, group) in [
                            ("GROUND", "terrain"),
                            ("COVER", "cover"),
                            ("ACTORS", "actors"),
                            ("OTHER", "other"),
                        ] {
                            seg_row(ui, cw, |s| {
                                s.half(&format!("{what:>7}"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ENTS.fmt(feed.latest(&format!("census/{group}/triangles")))), COLOR_DIM);
                                s.half(&format!("{:>7}", "DRAWS"), COLOR_DIM);
                                s.half(&format!("{:>5}  ", ENTS.fmt(feed.latest(&format!("census/{group}/draws")))), ALARM_DRAWS.color(feed.latest(&format!("census/{group}/draws"))));
                            });
                        }
                    });


                    // ── PASSES ──
                    // Every pass Bevy times, heaviest first: what the GPU
                    // spent on it, and what the thread encoding it spent.
                    // The frame is GPU-bound when the gpu sum nears the
                    // frame time; when it falls well short, the cpu column
                    // says whether the encoding took the rest.
                    tab_section(ui, tab, MetricsTab::Passes, content_width, |ui| {
                        const PASS_MS: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
                        let mut timed: std::collections::HashMap<&str, (f64, f64)> = Default::default();
                        for (name, rest) in feed.under("diag/render/") {
                            let ms = feed.latest(name);
                            if let Some(name) = rest.strip_suffix("/elapsed_cpu") {
                                timed.entry(name).or_default().0 = ms;
                            } else if let Some(name) = rest.strip_suffix("/elapsed_gpu") {
                                timed.entry(name).or_default().1 = ms;
                            }
                        }
                        let mut passes: Vec<(&str, (f64, f64))> = timed.into_iter().collect();
                        passes.sort_by(|a, b| (b.1.0 + b.1.1).total_cmp(&(a.1.0 + a.1.1)));
                        let cpu_sum: f64 = passes.iter().map(|p| p.1.0).sum();
                        let gpu_sum: f64 = passes.iter().map(|p| p.1.1).sum();
                        seg_row(ui, cw, |s| {
                            s.wide(&format!("{:<WIDE_WIDTH$}", "all passes"), COLOR_DIM);
                            s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(cpu_sum), "c"), ALARM_FRAME.color(cpu_sum));
                            s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(gpu_sum), "g"), ALARM_FRAME.color(gpu_sum));
                        });
                        for (name, (cpu, gpu)) in passes.iter().take(10) {
                            // The last path component names the pass; the
                            // tail of it is the telling part.
                            let leaf = name.rsplit('/').next().unwrap_or(name);
                            let shown: String = leaf.chars().rev().take(WIDE_WIDTH).collect::<Vec<_>>().into_iter().rev().collect();
                            seg_row(ui, cw, |s| {
                                s.wide(&format!("{shown:<WIDE_WIDTH$}"), COLOR_DIM);
                                s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(*cpu), "c"), COLOR_DIM);
                                s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(*gpu), "g"), COLOR_DIM);
                            });
                        }
                    });


                    // ── NETWORK ──
                    tab_section(ui, tab, MetricsTab::Network, content_width, |ui| {
                        const NET_BPS: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:<2}NET  ", GLYPH_NET_DOWN), COLOR_DIM);
                            s.half(&format!("{:>4}{:<3}", NET_BPS.fmt(feed.latest("network/bytes_per_sec")), "B/s"), COLOR_DIM);
                            s.spark(feed.history("network/bytes_per_sec"), SparkScale::Fixed(40960.0), &ALARM_BW, rh);
                            let pv = NET_BPS.fmt(feed.peak("network/bytes_per_sec", bar_count));
                            s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                        const NET_MPS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        seg_row(ui, cw, |s| {
                            s.half(&format!("{:<2}MSG  ", GLYPH_NET_DOWN), COLOR_DIM);
                            s.half(&format!("{:>5}{:<2}", NET_MPS.fmt(feed.latest("network/messages_per_sec")), "/s"), COLOR_DIM);
                            s.spark(feed.history("network/messages_per_sec"), SparkScale::Fixed(40.0), &ALARM_MSG, rh);
                            let pv = NET_MPS.fmt(feed.peak("network/messages_per_sec", bar_count));
                            s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                    });


                    // ── TIMINGS ──
                    let mut timed: Vec<&str> = feed.under("timings/").filter_map(|(_, rest)| rest.strip_suffix(".p95")).collect();
                    timed.sort();
                    if tab == MetricsTab::Timings && !timed.is_empty() {
                        const ALARM_TIMING: Alarm = Alarm { bands: &[
                            (16.667, COLOR_NORMAL),
                            (33.333, COLOR_WARN),
                            (f64::INFINITY, COLOR_CRITICAL),
                        ]};
                        const ALARM_OVERRUN: Alarm = Alarm { bands: &[
                            (0.5, COLOR_DIM),
                            (f64::INFINITY, COLOR_CRITICAL),
                        ]};
                        const TIME5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
                        const OVERRUN: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Clamp };

                        draw_section(ui, "TIMINGS", content_width, |ui| {
                            for &name in &timed {
                                let history = feed.history(&format!("timings/{name}.p95"));
                                seg_row(ui, cw, |s| {
                                    // Label (7ch)
                                    let display = if name.len() > 7 { &name[..7] } else { name };
                                    s.half(&format!("{:>7}", display), COLOR_DIM);
                                    // Value (7ch): p95 + unit
                                    let p95_val = history.last().copied().unwrap_or(0.0) as f64;
                                    s.half(&format!("{:>5}{:<2}", TIME5.fmt(p95_val), "ms"), COLOR_DIM);
                                    // Sparkline (15ch)
                                    s.spark(history, SparkScale::Fixed(33.0), &ALARM_TIMING, rh);
                                    // Peak (7ch)
                                    let peak = feed.peak(&format!("timings/{name}.p95"), bar_count);
                                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(peak)), COLOR_DIM);
                                    // Overruns: count of p95 > 16.667ms in visible window (7ch)
                                    let start = history.len().saturating_sub(bar_count);
                                    let ov_val = history[start..].iter().filter(|&&v| v > 16.667).count() as f64;
                                    s.half(&format!("{:>5}{:<2}", OVERRUN.fmt(ov_val), GLYPH_OVERRUN), ALARM_OVERRUN.color(ov_val));
                                });
                            }
                        });
                    }
                });
        });

    // ── The strip under the view ──
    // The camera is letterboxed to 16:9 in what the panel leaves, so the
    // band below it is the window's own dead space: the tabs live there
    // rather than take rows from the panel.
    let strip_top = cam_h as f32 / scale_factor;
    let strip_height = window_logical_height - strip_top;
    if strip_height < rh * 2.0 {
        return;
    }
    let mut picked = None;
    egui::Area::new(egui::Id::new("metrics_tabs"))
        .fixed_pos(egui::pos2(0.0, strip_top))
        .show(ctx, |ui| {
            let strip = egui::Rect::from_min_size(
                egui::pos2(0.0, strip_top),
                egui::vec2(cam_w as f32 / scale_factor, strip_height),
            );
            ui.painter().rect_filled(strip, 0.0, COLOR_BG);
            egui::Frame::NONE.inner_margin(OUTER_MARGIN).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = cw * 2.0;
                    for entry in MetricsTab::ALL {
                        let lit = entry == tab;
                        let text = if lit { format!("[{}]", entry.label()) } else { format!(" {} ", entry.label()) };
                        let color = if lit { COLOR_NORMAL } else { COLOR_DIM };
                        let label = egui::Label::new(colored_mono(&text, color)).sense(egui::Sense::click());
                        if ui.add(label).clicked() {
                            picked = Some(entry);
                        }
                    }
                });
            });
        });
    if let Some(entry) = picked {
        state.metrics_tab = entry;
    }
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
}
