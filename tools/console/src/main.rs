use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;
use serde::{Deserialize, Serialize};

// Mirror of common::metrics — keep in sync, version check catches drift.
const METRICS_MAGIC: [u8; 4] = *b"GMSV";
const METRICS_VERSION: u16 = 4;

#[derive(Clone, Default, Serialize, Deserialize)]
struct ServerMetrics {
    tick_count: u64,
    snapshot_time_secs: f64,
    loaded_hexes: u32,
    connected_players: u32,
    tick_duration_us: u64,
    tick_duration_max_us: u64,
    memory_bytes: u64,
    memory_map_bytes: u64,
    frame_duration_us: u64,
    frame_duration_max_us: u64,
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.1} MB", b / MB)
    }
}

fn main() -> eframe::Result {
    let (tx, rx) = mpsc::channel::<ServerMetrics>();

    std::thread::spawn(move || receiver_loop(tx));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 340.0])
            .with_min_inner_size([320.0, 280.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Server Console",
        options,
        Box::new(|_cc| Ok(Box::new(ConsoleApp::new(rx)))),
    )
}

fn receiver_loop(tx: mpsc::Sender<ServerMetrics>) {
    let socket = UdpSocket::bind("127.0.0.1:5100").expect("failed to bind to metrics port 5100");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .ok();

    let mut buf = [0u8; 4096];
    loop {
        let n = match socket.recv(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if n < 6 {
            continue;
        }

        if buf[..4] != METRICS_MAGIC {
            continue;
        }

        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != METRICS_VERSION {
            eprintln!(
                "metrics version mismatch: expected {}, got {}",
                METRICS_VERSION, version
            );
            continue;
        }

        let Ok((metrics, _)) =
            bincode::serde::decode_from_slice::<ServerMetrics, _>(&buf[6..n], bincode::config::legacy())
        else {
            continue;
        };

        if tx.send(metrics).is_err() {
            break; // GUI closed
        }
    }
}

struct ConsoleApp {
    rx: mpsc::Receiver<ServerMetrics>,
    current: Option<ServerMetrics>,
    previous: Option<ServerMetrics>,
    last_received: Option<Instant>,
    ticks_per_sec: f64,
}

impl ConsoleApp {
    fn new(rx: mpsc::Receiver<ServerMetrics>) -> Self {
        Self {
            rx,
            current: None,
            previous: None,
            last_received: None,
            ticks_per_sec: 0.0,
        }
    }

    fn poll(&mut self) {
        let mut latest = None;
        while let Ok(snapshot) = self.rx.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            self.previous = self.current.take();
            self.current = Some(snapshot);
            self.last_received = Some(Instant::now());

            if let (Some(cur), Some(prev)) = (&self.current, &self.previous) {
                let dt = cur.snapshot_time_secs - prev.snapshot_time_secs;
                if dt > 0.0 {
                    self.ticks_per_sec =
                        (cur.tick_count - prev.tick_count) as f64 / dt;
                }
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.last_received
            .map(|t| t.elapsed().as_secs_f64() < 5.0)
            .unwrap_or(false)
    }
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        egui::CentralPanel::default().show(ctx, |ui| {
            // --- Connection Status ---
            ui.horizontal(|ui| {
                if self.is_connected() {
                    ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "\u{2B24}");
                    ui.label("Connected");
                } else {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "\u{2B24}");
                    ui.label("Waiting for server...");
                }

                if let Some(t) = self.last_received {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{:.0}s ago", t.elapsed().as_secs_f64()));
                    });
                }
            });

            ui.separator();

            if let Some(m) = &self.current {
                // --- Server Load (frame budget) ---
                let frame_ms = m.frame_duration_us as f64 / 1000.0;
                let frame_max_ms = m.frame_duration_max_us as f64 / 1000.0;
                let load_pct = (frame_ms / 125.0 * 100.0).min(999.0);

                ui.heading(format!("Server Load: {:.0}%", load_pct));
                ui.add_space(4.0);

                // Frame budget bar
                let fraction = (frame_ms / 125.0).clamp(0.0, 1.0) as f32;
                let bar_color = if load_pct < 40.0 {
                    egui::Color32::from_rgb(80, 200, 80)
                } else if load_pct < 80.0 {
                    egui::Color32::from_rgb(230, 180, 40)
                } else {
                    egui::Color32::from_rgb(220, 60, 60)
                };

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
                let mut fill = rect;
                fill.set_right(rect.left() + rect.width() * fraction);
                painter.rect_filled(fill, 3.0, bar_color);

                ui.add_space(4.0);

                egui::Grid::new("frame_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Frame:");
                    ui.label(format!("{:.1} / 125 ms", frame_ms));
                    ui.end_row();

                    ui.label("Peak:");
                    ui.label(format!("{:.1} ms", frame_max_ms));
                    ui.end_row();

                    ui.label("Tick rate:");
                    ui.label(format!("{:.1} /s", self.ticks_per_sec));
                    ui.end_row();

                    ui.label("Players:");
                    ui.label(format!("{}", m.connected_players));
                    ui.end_row();

                    ui.label("Loaded hexes:");
                    ui.label(format!("{}", m.loaded_hexes));
                    ui.end_row();
                });

                ui.add_space(8.0);
                ui.separator();

                // --- Memory ---
                ui.heading("Memory");
                ui.add_space(4.0);

                let total = m.memory_bytes;
                let map = m.memory_map_bytes;
                let other = total.saturating_sub(map);

                ui.label(format!("Total: {}", format_bytes(total)));
                ui.add_space(4.0);

                // Stacked bar
                if total > 0 {
                    let map_frac = (map as f32 / total as f32).clamp(0.0, 1.0);

                    let bar_height = 20.0;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), bar_height),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);

                    let color_map = egui::Color32::from_rgb(100, 160, 220);
                    let color_other = egui::Color32::from_rgb(160, 160, 160);

                    // Map slice (left)
                    if map_frac > 0.0 {
                        let mut r = rect;
                        r.set_right(rect.left() + rect.width() * map_frac);
                        painter.rect_filled(r, 3.0, color_map);
                    }

                    // Other slice fills the rest (already painted as bg, just
                    // paint over to get the right color)
                    if map_frac < 1.0 {
                        let mut r = rect;
                        r.set_left(rect.left() + rect.width() * map_frac);
                        painter.rect_filled(r, 3.0, color_other);
                    }

                    ui.add_space(6.0);

                    // Legend
                    egui::Grid::new("mem_legend").num_columns(3).show(ui, |ui| {
                        ui.colored_label(color_map, "\u{25A0}");
                        ui.label("Map");
                        ui.label(format_bytes(map));
                        ui.end_row();

                        ui.colored_label(color_other, "\u{25A0}");
                        ui.label("Other");
                        ui.label(format_bytes(other));
                        ui.end_row();
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                // --- Uptime ---
                let secs = m.snapshot_time_secs;
                let hours = (secs / 3600.0) as u64;
                let mins = ((secs % 3600.0) / 60.0) as u64;
                let s = (secs % 60.0) as u64;
                ui.horizontal(|ui| {
                    ui.label("Uptime:");
                    ui.label(format!("{:02}:{:02}:{:02}", hours, mins, s));
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label("No data received yet.");
                });
            }
        });
    }
}
