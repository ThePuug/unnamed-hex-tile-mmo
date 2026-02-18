use std::collections::VecDeque;
use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;
use serde::{Deserialize, Serialize};

// Mirror of common::metrics — keep in sync, version check catches drift.
const METRICS_MAGIC: [u8; 4] = *b"GMSV";
const METRICS_VERSION: u16 = 6;

#[derive(Clone, Default, Serialize, Deserialize)]
struct ServerMetrics {
    tick_count: u64,
    snapshot_time_secs: f64,
    loaded_hexes: u32,
    connected_players: u32,
    tick_duration_us: u64,
    tick_duration_max_us: u64,
    tick_overrun_count: u64,
    memory_bytes: u64,
    memory_map_bytes: u64,
    frame_duration_us: u64,
    frame_duration_max_us: u64,
    frame_overrun_count: u64,
}

/// Ring buffer for sparkline history.
const HISTORY_LEN: usize = 60;

/// 2-minute window at 2s intervals = 60 snapshots.
const WINDOW_LEN: usize = 60;

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
}

/// Rolling window that tracks a value over WINDOW_LEN snapshots.
/// Used to compute peaks and deltas over the 2-minute window.
struct RollingWindow(VecDeque<f64>);

impl RollingWindow {
    fn new() -> Self {
        Self(VecDeque::with_capacity(WINDOW_LEN + 1))
    }

    fn push(&mut self, val: f64) {
        if self.0.len() > WINDOW_LEN {
            self.0.pop_front();
        }
        self.0.push_back(val);
    }

    /// Max value in the window.
    fn max(&self) -> f64 {
        self.0.iter().copied().fold(0.0_f64, f64::max)
    }

    /// Difference between newest and oldest value (for cumulative counters).
    fn delta(&self) -> f64 {
        if self.0.len() < 2 {
            return 0.0;
        }
        self.0.back().unwrap() - self.0.front().unwrap()
    }
}

// --- Limits coloring ---

fn limit_color(val: f64, green_below: f64, yellow_below: f64) -> egui::Color32 {
    if val < green_below {
        egui::Color32::from_rgb(80, 200, 80)
    } else if val < yellow_below {
        egui::Color32::from_rgb(230, 180, 40)
    } else {
        egui::Color32::from_rgb(220, 60, 60)
    }
}

fn main() -> eframe::Result {
    let (tx, rx) = mpsc::channel::<ServerMetrics>();

    std::thread::spawn(move || receiver_loop(tx));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 260.0])
            .with_min_inner_size([420.0, 200.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Server Console",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ConsoleApp::new(rx)))
        }),
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
            break;
        }
    }
}

struct ConsoleApp {
    rx: mpsc::Receiver<ServerMetrics>,
    current: Option<ServerMetrics>,
    previous: Option<ServerMetrics>,
    last_received: Option<Instant>,
    ticks_per_sec: f64,

    // Sparkline histories (60 samples = 2 min at 2s intervals)
    hist_frame: History,
    hist_tick: History,
    hist_mem: History,

    // Rolling 2-minute windows for peaks and counters
    window_frame_peak: RollingWindow,
    window_frame_overruns: RollingWindow,
    window_tick_peak: RollingWindow,
    window_tick_overruns: RollingWindow,
}

impl ConsoleApp {
    fn new(rx: mpsc::Receiver<ServerMetrics>) -> Self {
        Self {
            rx,
            current: None,
            previous: None,
            last_received: None,
            ticks_per_sec: 0.0,
            hist_frame: History::new(),
            hist_tick: History::new(),
            hist_mem: History::new(),
            window_frame_peak: RollingWindow::new(),
            window_frame_overruns: RollingWindow::new(),
            window_tick_peak: RollingWindow::new(),
            window_tick_overruns: RollingWindow::new(),
        }
    }

    fn poll(&mut self) {
        let mut latest = None;
        while let Ok(snapshot) = self.rx.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            // Record sparkline history
            let frame_ms = snapshot.frame_duration_us as f64 / 1000.0;
            let tick_ms = snapshot.tick_duration_us as f64 / 1000.0;
            let mem_mb = snapshot.memory_bytes as f64 / (1024.0 * 1024.0);
            self.hist_frame.push(frame_ms);
            self.hist_tick.push(tick_ms);
            self.hist_mem.push(mem_mb);

            // Rolling windows — track per-snapshot peaks and cumulative counter
            let frame_peak_ms = snapshot.frame_duration_max_us as f64 / 1000.0;
            let tick_peak_ms = snapshot.tick_duration_max_us as f64 / 1000.0;
            self.window_frame_peak.push(frame_peak_ms);
            self.window_frame_overruns.push(snapshot.frame_overrun_count as f64);
            self.window_tick_peak.push(tick_peak_ms);
            self.window_tick_overruns.push(snapshot.tick_overrun_count as f64);

            // Compute tick rate
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

/// Draw a sparkline into an allocated rect.
/// `max_val` is the Y-axis ceiling. Values are clamped to it.
fn draw_sparkline(
    ui: &mut egui::Ui,
    history: &VecDeque<f64>,
    max_val: f64,
    width: f32,
    height: f32,
    color: egui::Color32,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 30, 35));

    if history.is_empty() || max_val <= 0.0 {
        return;
    }

    let n = history.len();
    let bar_w = rect.width() / HISTORY_LEN as f32;

    for (i, &val) in history.iter().enumerate() {
        let frac = (val / max_val).clamp(0.0, 1.0) as f32;
        let x = rect.left() + (HISTORY_LEN - n + i) as f32 * bar_w;
        let bar_h = frac * rect.height();
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - bar_h),
            egui::vec2(bar_w.max(1.0), bar_h),
        );
        painter.rect_filled(bar_rect, 0.0, color.linear_multiply(0.7 + 0.3 * frac));
    }
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        egui::CentralPanel::default().show(ctx, |ui| {
            // --- Connection status bar ---
            ui.horizontal(|ui| {
                if self.is_connected() {
                    ui.colored_label(egui::Color32::from_rgb(80, 200, 80), "\u{2B24}");
                    ui.monospace("CONNECTED");
                } else {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 80), "\u{2B24}");
                    ui.monospace("NO SIGNAL");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(m) = &self.current {
                        let secs = m.snapshot_time_secs;
                        let h = (secs / 3600.0) as u64;
                        let min = ((secs % 3600.0) / 60.0) as u64;
                        let s = (secs % 60.0) as u64;
                        ui.monospace(format!("T+ {:02}:{:02}:{:02}", h, min, s));
                    }
                    if let Some(t) = self.last_received {
                        let ago = t.elapsed().as_secs_f64();
                        let stale_color = if ago < 4.0 {
                            egui::Color32::from_rgb(120, 120, 120)
                        } else {
                            egui::Color32::from_rgb(200, 80, 80)
                        };
                        ui.colored_label(stale_color, format!("{:.0}s", ago));
                    }
                });
            });

            ui.separator();

            if let Some(m) = &self.current {
                let frame_ms = m.frame_duration_us as f64 / 1000.0;
                let tick_ms = m.tick_duration_us as f64 / 1000.0;
                let mem_mb = m.memory_bytes as f64 / (1024.0 * 1024.0);
                let map_mb = m.memory_map_bytes as f64 / (1024.0 * 1024.0);

                // Rolling 2-min peaks and overrun counts
                let frame_peak_2m = self.window_frame_peak.max();
                let frame_overruns_2m = self.window_frame_overruns.delta() as u64;
                let tick_peak_2m = self.window_tick_peak.max();
                let tick_overruns_2m = self.window_tick_overruns.delta() as u64;

                let sparkline_w = 100.0;
                let sparkline_h = 16.0;

                // --- FRAME row ---
                ui.horizontal(|ui| {
                    let color = limit_color(frame_ms, 50.0, 100.0);
                    ui.monospace("FRAME ");
                    ui.colored_label(color, egui::RichText::new(format!("{:>6.1}ms", frame_ms)).monospace());
                    draw_sparkline(ui, &self.hist_frame.0, 125.0, sparkline_w, sparkline_h, color);
                    let peak_color = limit_color(frame_peak_2m, 50.0, 100.0);
                    ui.colored_label(peak_color, egui::RichText::new(format!("pk {:>6.1}", frame_peak_2m)).monospace());
                    let overrun_color = if frame_overruns_2m > 0 {
                        egui::Color32::from_rgb(220, 60, 60)
                    } else {
                        egui::Color32::from_rgb(120, 120, 120)
                    };
                    ui.colored_label(overrun_color, egui::RichText::new(format!("{:>3}!", frame_overruns_2m)).monospace());
                });

                // --- TICK row ---
                ui.horizontal(|ui| {
                    let color = limit_color(tick_ms, 5.0, 50.0);
                    ui.monospace("TICK  ");
                    ui.colored_label(color, egui::RichText::new(format!("{:>6.1}ms", tick_ms)).monospace());
                    draw_sparkline(ui, &self.hist_tick.0, 125.0, sparkline_w, sparkline_h, color);
                    let peak_color = limit_color(tick_peak_2m, 5.0, 50.0);
                    ui.colored_label(peak_color, egui::RichText::new(format!("pk {:>6.1}", tick_peak_2m)).monospace());
                    let overrun_color = if tick_overruns_2m > 0 {
                        egui::Color32::from_rgb(220, 60, 60)
                    } else {
                        egui::Color32::from_rgb(120, 120, 120)
                    };
                    ui.colored_label(overrun_color, egui::RichText::new(format!("{:>3}!", tick_overruns_2m)).monospace());
                });

                // --- MEM row ---
                ui.horizontal(|ui| {
                    let color = limit_color(mem_mb, 512.0, 1024.0);
                    ui.monospace("MEM   ");
                    ui.colored_label(color, egui::RichText::new(format!("{:>6.1}MB", mem_mb)).monospace());
                    draw_sparkline(ui, &self.hist_mem.0, mem_mb * 1.5 + 1.0, sparkline_w, sparkline_h, color);
                    ui.monospace(format!("map{:>6.1}", map_mb));
                });

                ui.add_space(4.0);
                ui.separator();

                // --- Status row ---
                ui.horizontal(|ui| {
                    ui.monospace(format!(
                        "PLAYERS {:>3}    HEXES {:>8}",
                        m.connected_players, m.loaded_hexes,
                    ));
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.monospace("AWAITING TELEMETRY...");
                });
            }
        });
    }
}
