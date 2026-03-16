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

/// Segment width in monospace characters.
const SEG_WIDTH: usize = 14;
/// Gap between segments in characters.
const SEG_GAP: usize = 1;
/// Total content width in characters: 3 segments + 2 gaps.
const PANEL_CHARS: usize = 3 * SEG_WIDTH + 2 * SEG_GAP;

const FONT_SIZE: f32 = 12.0;
/// Outer margin around all content (matches console CentralPanel inner_margin).
const OUTER_MARGIN: f32 = 8.0;
/// Inner margin inside each section frame (matches console draw_section).
const SECTION_INNER_MARGIN: f32 = 4.0;

// ── Colors (match server console) ──

const COLOR_NORMAL: egui::Color32 = egui::Color32::from_rgb(180, 255, 180);
const COLOR_DIM: egui::Color32 = egui::Color32::from_rgb(120, 160, 120);
const COLOR_BORDER: egui::Color32 = egui::Color32::from_rgb(80, 120, 80);
const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(10, 15, 10);

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

// ── Composable segments (match server console) ──

/// "LABEL nnn.d uu" — 14 chars: 5 label + 1 sp + 5 value + 1 sp + 2 unit
fn seg_label(ui: &mut egui::Ui, label: &str, value: &str, unit: &str) {
    ui.label(colored_mono(
        &format!("{:>5} {} {:<2}", label, value, unit),
        COLOR_DIM,
    ));
}

/// 1-char gap between segments.
fn seg_gap(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(char_width);
}

/// Empty segment placeholder — SEG_WIDTH chars of space.
fn seg_empty(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(SEG_WIDTH as f32 * char_width);
}

// ── Section rendering (matches console draw_section) ──

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

/// Render a row of 1–3 segments. Unused segment slots are left empty.
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

// ── Overlay camera ──

/// Marker component for the dedicated egui overlay camera.
#[derive(Component)]
pub struct OverlayCamera;

/// Stores the entity id of the overlay camera.
#[derive(Resource)]
pub struct OverlayCameraEntity(pub Entity);

/// Spawns a Camera2d dedicated to egui overlay rendering.
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

// ── Main system ──

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
    // ── Toggle off: restore fullscreen viewport ──
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
    let cw = ctx.fonts_mut(|f| f.glyph_width(&mono_font(), '0'));

    // ── Panel pixel width ──
    // Content width + section frame overhead + outer margin on both sides
    // Section frame: stroke(1px) + inner_margin(4px) on each side = 10px
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

    // ── Render egui panel ──
    let window_logical_width = window_width as f32 / scale_factor;
    let window_logical_height = window_height as f32 / scale_factor;
    let panel_x = window_logical_width - panel_pixel_width;

    egui::Area::new(egui::Id::new("metrics_overlay"))
        .fixed_pos(egui::pos2(panel_x, 0.0))
        .show(ctx, |ui| {
            // Paint full-height background
            let bg_rect = egui::Rect::from_min_size(
                egui::pos2(panel_x, 0.0),
                egui::vec2(panel_pixel_width, window_logical_height),
            );
            ui.painter().rect_filled(bg_rect, 0.0, COLOR_BG);

            // Outer margin matching console's CentralPanel inner_margin(8.0)
            egui::Frame::NONE
                .inner_margin(OUTER_MARGIN)
                .show(ui, |ui| {
                    // ── TERRAIN section ──
                    let fv = format_value; // alias for brevity
                    let fi = format_int;
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

                    // ── RENDER section ──
                    draw_section(ui, "RENDER", content_width, |ui| {
                        metric_row(ui, cw, &[
                            ("FPS", &fv(fps), "hz"),
                            ("FRAME", &fv(frame_ms), "ms"),
                        ]);
                        metric_row(ui, cw, &[
                            ("ENTS", &fi(entities), "n"),
                            ("TILES", &fi(map.len() as f64), "n"),
                        ]);
                    });

                    ui.add_space(4.0);

                    // ── NETWORK section ──
                    draw_section(ui, "NETWORK", content_width, |ui| {
                        metric_row(ui, cw, &[
                            ("BW", &fv(bps), "Bs"),
                            ("MSG", &fv(mps), "/s"),
                        ]);
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
}
