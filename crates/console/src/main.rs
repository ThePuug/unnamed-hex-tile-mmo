use std::collections::{HashMap, VecDeque};
use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

// ── Wire types (mirror of common_bevy::metrics) ──

const METRICS_MAGIC: [u8; 4] = *b"GMSV";
const METRICS_VERSION: u16 = 9;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Cadence { Snapshot = 0, Event = 1 }

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MetricsPacket {
    group: String,
    cadence: Cadence,
    timestamp_secs: f64,
    fields: Vec<(String, f32)>,
}

// ── Font ──

fn load_fonts(ctx: &egui::Context) {
    let font_bytes = include_bytes!("../../../assets/fonts/Iosevka-Regular.ttc");
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Iosevka".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(font_bytes)),
    );
    fonts.families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "Iosevka".to_owned());
    ctx.set_fonts(fonts);
}

// ── Layout constants ──

const FONT_SIZE: f32 = 12.0;
const SECTION_COLS: usize = 3;
const SPARKLINE_CHARS: usize = 15;
const MIN_BAR_WIDTH_PX: f32 = 2.0;

const COLOR_NORMAL: Color32 = Color32::from_rgb(180, 255, 180);
const COLOR_CRITICAL: Color32 = Color32::from_rgb(255, 80, 80);
const COLOR_DIM: Color32 = Color32::from_rgb(120, 160, 120);
const COLOR_BORDER: Color32 = Color32::from_rgb(80, 120, 80);
const COLOR_BG: Color32 = Color32::from_rgb(10, 15, 10);
const COLOR_SPARK_BG: Color32 = Color32::from_rgb(30, 30, 35);

fn mono_font() -> egui::FontId { egui::FontId::monospace(FONT_SIZE) }

fn colored_mono(text: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat {
        font_id: mono_font(),
        color,
        ..Default::default()
    });
    job
}

// ── Data structures ──

const HISTORY_LEN: usize = 60;


struct History(VecDeque<f64>);
impl History {
    fn new() -> Self { Self(VecDeque::with_capacity(HISTORY_LEN)) }
    fn push(&mut self, val: f64) {
        if self.0.len() >= HISTORY_LEN { self.0.pop_front(); }
        self.0.push_back(val);
    }
    fn as_f32(&self) -> Vec<f32> { self.0.iter().map(|&v| v as f32).collect() }
    /// Max of the most recent `n` samples (matches sparkline visible window).
    fn visible_max(&self, n: usize) -> f64 {
        let start = self.0.len().saturating_sub(n);
        self.0.range(start..).copied().fold(0.0_f64, f64::max)
    }
    /// Sum of the most recent `n` samples (matches sparkline visible window).
    fn visible_sum(&self, n: usize) -> f64 {
        let start = self.0.len().saturating_sub(n);
        self.0.range(start..).copied().sum()
    }
}




use common::numfmt;

// ── Rect-based sparkline ──



/// Scale mode for sparkline Y axis.
enum SparkScale {
    /// Fixed ceiling — bars scaled against a known budget/limit.
    Fixed(f32),
    /// Auto-scale to the max value in the history.
    Auto,
}

fn draw_sparkline(
    ui: &mut egui::Ui, history: &[f32],
    scale: SparkScale, alarm: &Alarm,
    char_width: f32, row_height: f32,
) {
    let width = SPARKLINE_CHARS as f32 * char_width;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, row_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COLOR_SPARK_BG);

    if history.is_empty() { return; }
    let bar_count = (width / MIN_BAR_WIDTH_PX).floor() as usize;
    if bar_count == 0 { return; }

    let n = history.len();
    let start = n.saturating_sub(bar_count);
    let samples = &history[start..];

    let max_val = match scale {
        SparkScale::Fixed(v) => v,
        SparkScale::Auto => samples.iter().cloned().fold(0.0_f32, f32::max),
    };
    if max_val <= 0.0 { return; }
    let bar_width = width / bar_count as f32;
    let offset = bar_count - samples.len();

    for (i, &val) in samples.iter().enumerate() {
        let t = (val / max_val).clamp(0.0, 1.0);
        let bar_height = t * rect.height();
        if bar_height < 0.5 { continue; }
        let x = rect.left() + (offset + i) as f32 * bar_width;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - bar_height),
            egui::vec2(bar_width, bar_height),
        );
        let color = alarm.color(val as f64);
        painter.rect_filled(bar_rect, 0.0, color);
    }
}

fn next_power_of_two_ceil(val: f64) -> f64 {
    if val <= 1.0 { return 1.0; }
    2.0_f64.powf(val.log2().ceil())
}


// ── Alarm bands ──

/// Color bands evaluated low-to-high. Value falls into the first band
/// whose threshold it is below. Example: `[(100, GREEN), (125, YELLOW), (INF, RED)]`
struct Alarm {
    bands: &'static [(f64, Color32)],
}

impl Alarm {
    #[allow(dead_code)]
    const NONE: Self = Self { bands: &[(f64::INFINITY, COLOR_NORMAL)] };

    fn color(&self, val: f64) -> Color32 {
        for &(threshold, color) in self.bands {
            if val < threshold { return color; }
        }
        self.bands.last().map_or(COLOR_NORMAL, |&(_, c)| c)
    }
}

const COLOR_WARN: Color32 = Color32::from_rgb(255, 200, 60);

// ── Tile-width segment primitives ──
//
// Each function takes a pre-built string and asserts its display width matches the slot.
// Callers use standard format!() for layout; wide chars (↑, ↔, △ etc.) are 2 display
// cells but 1 Rust char — account for them at the call site, not here.

/// Display cell count for a string, accounting for known wide chars in Iosevka.
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if matches!(c, '△' | '●' | '○' | '↑' | '↓' | '↔') { 2 } else { 1 })
        .sum()
}

/// 1-char gap between segments.
fn seg_gap(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(char_width);
}

/// Full segment: 15 display cells.
fn seg_full(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(display_width(s), SPARKLINE_CHARS,
        "seg_full: {} cells expected, got {} for {:?}", SPARKLINE_CHARS, display_width(s), s);
    ui.label(colored_mono(s, color));
}

/// Half-segment: 7 display cells.
#[allow(dead_code)]
fn seg_half(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(display_width(s), 7,
        "seg_half: 7 cells expected, got {} for {:?}", display_width(s), s);
    ui.label(colored_mono(s, color));
}

/// Quarter-segment: 3 display cells.
#[allow(dead_code)]
fn seg_quarter(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(display_width(s), 3,
        "seg_quarter: 3 cells expected, got {} for {:?}", display_width(s), s);
    ui.label(colored_mono(s, color));
}

/// Eighth-segment: 1 display cell.
#[allow(dead_code)]
fn seg_eighth(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(display_width(s), 1,
        "seg_eighth: 1 cell expected, got {} for {:?}", display_width(s), s);
    ui.label(colored_mono(s, color));
}

/// Sparkline rect — 15 chars wide.
fn seg_spark(
    ui: &mut egui::Ui, history: &[f32],
    scale: SparkScale, alarm: &Alarm,
    char_width: f32, row_height: f32,
) {
    draw_sparkline(ui, history, scale, alarm, char_width, row_height);
}

// ── Section box ──

fn draw_section<F: FnOnce(&mut egui::Ui)>(
    ui: &mut egui::Ui, label: &str, content: F,
) {
    ui.label(colored_mono(label, COLOR_BORDER));
    let avail = ui.available_width();
    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, COLOR_BORDER))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.set_min_width(avail - 10.0);
            content(ui);
        });
}

// ── App ──

fn main() -> eframe::Result {
    let (tx, rx) = mpsc::channel::<MetricsPacket>();
    std::thread::spawn(move || receiver_loop(tx));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1515.0, 380.0])
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };

    eframe::run_native(
        "Server Console",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(ConsoleApp::new(rx)))
        }),
    )
}

fn receiver_loop(tx: mpsc::Sender<MetricsPacket>) {
    let socket = UdpSocket::bind("127.0.0.1:5100").expect("failed to bind to metrics port 5100");
    socket.set_read_timeout(Some(std::time::Duration::from_secs(1))).ok();
    let mut buf = [0u8; 4096];
    loop {
        let n = match socket.recv(&mut buf) { Ok(n) => n, Err(_) => continue };
        if n < 6 || buf[..4] != METRICS_MAGIC { continue; }
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != METRICS_VERSION {
            eprintln!("metrics version mismatch: expected {}, got {}", METRICS_VERSION, version);
            continue;
        }
        let Ok((packet, _)) =
            bincode::serde::decode_from_slice::<MetricsPacket, _>(&buf[6..n], bincode::config::legacy())
        else { continue };
        if tx.send(packet).is_err() { break; }
    }
}

struct ConsoleApp {
    rx: mpsc::Receiver<MetricsPacket>,
    last_received: Option<Instant>,
    snapshot: HashMap<String, f32>,
    snapshot_timestamp: f64,
    prev_snapshot_timestamp: f64,
    prev_tick_count: f32,
    ticks_per_sec: f64,
    char_width: Option<f32>,
    row_height: Option<f32>,

    hist_frame: History, hist_tick: History, hist_mem: History,
    hist_frame_overruns: History, hist_tick_overruns: History,

    hist_async_dur: History,
    hist_async_queue: History,

    hist_net_sent: History,
    hist_net_recv: History,
    hist_ord_queue: History,
    hist_unord_queue: History,
}

impl ConsoleApp {
    fn new(rx: mpsc::Receiver<MetricsPacket>) -> Self {
        Self {
            rx, last_received: None,
            snapshot: HashMap::new(),
            snapshot_timestamp: 0.0, prev_snapshot_timestamp: 0.0,
            prev_tick_count: 0.0, ticks_per_sec: 0.0,
            char_width: None, row_height: None,
            hist_frame: History::new(), hist_tick: History::new(), hist_mem: History::new(),
            hist_frame_overruns: History::new(), hist_tick_overruns: History::new(),
            hist_async_dur: History::new(),
            hist_async_queue: History::new(),
            hist_net_sent: History::new(),
            hist_net_recv: History::new(),
            hist_ord_queue: History::new(),
            hist_unord_queue: History::new(),
        }
    }

    fn field(&self, name: &str) -> f64 {
        self.snapshot.get(name).copied().unwrap_or(0.0) as f64
    }

    fn poll(&mut self) {
        while let Ok(packet) = self.rx.try_recv() {
            self.last_received = Some(Instant::now());
            if packet.cadence == Cadence::Snapshot {
                self.handle_snapshot(packet);
            }
        }
    }

    fn handle_snapshot(&mut self, packet: MetricsPacket) {
        self.snapshot.clear();
        for (name, val) in &packet.fields { self.snapshot.insert(name.clone(), *val); }

        self.prev_snapshot_timestamp = self.snapshot_timestamp;
        self.snapshot_timestamp = packet.timestamp_secs;
        let dt = self.snapshot_timestamp - self.prev_snapshot_timestamp;
        if dt > 0.0 {
            let tc = self.field("tick_count") as f32;
            self.ticks_per_sec = (tc - self.prev_tick_count) as f64 / dt;
            self.prev_tick_count = tc;
        }

        self.hist_frame.push(self.field("frame_peak_ms"));
        self.hist_tick.push(self.field("tick_peak_ms"));
        self.hist_mem.push(self.field("memory_mb"));
        self.hist_frame_overruns.push(self.field("frame_overruns"));
        self.hist_tick_overruns.push(self.field("tick_overruns"));

        self.hist_async_dur.push(self.field("async.task_duration_ms"));
        self.hist_async_queue.push(self.field("async.tasks_in_flight"));
        self.hist_net_sent.push(self.field("net_sent_bps"));
        self.hist_net_recv.push(self.field("net_recv_bps"));
        self.hist_ord_queue.push(self.field("net_ord_queue"));
        self.hist_unord_queue.push(self.field("net_unord_queue"));
    }

    fn is_connected(&self) -> bool {
        self.last_received.map(|t| t.elapsed().as_secs_f64() < 5.0).unwrap_or(false)
    }

    fn measure_font(&mut self, ui: &egui::Ui) {
        if self.char_width.is_none() {
            let font = mono_font();
            self.char_width = Some(ui.fonts(|f| f.glyph_width(&font, '0')));
            self.row_height = Some(ui.fonts(|f| f.row_height(&font)));
        }
    }
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(COLOR_BG).inner_margin(8.0))
            .show(ctx, |ui| {
                self.measure_font(ui);
                let cw = self.char_width.unwrap_or(7.0);
                let rh = self.row_height.unwrap_or(14.0);

                // ── Status bar ──
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if self.is_connected() {
                        ui.label(colored_mono("● CONNECTED", COLOR_NORMAL));
                    } else {
                        ui.label(colored_mono("● NO SIGNAL", COLOR_CRITICAL));
                    }
                    let secs = self.snapshot_timestamp;
                    ui.label(colored_mono(
                        &format!("   T+ {:02}:{:02}:{:02}",
                            (secs / 3600.0) as u64,
                            ((secs % 3600.0) / 60.0) as u64,
                            (secs % 60.0) as u64),
                        COLOR_DIM));
                    if let Some(t) = self.last_received {
                        let ago = t.elapsed().as_secs_f64();
                        let c = if ago < 4.0 { COLOR_DIM } else { COLOR_CRITICAL };
                        ui.label(colored_mono(&format!("   {:.0}s", ago), c));
                    }
                });

                ui.add_space(4.0);

                if self.snapshot.is_empty() && !self.is_connected() {
                    ui.label(colored_mono("\n  AWAITING TELEMETRY...", COLOR_DIM));
                    return;
                }

                // ── Alarm definitions ──
                const ALARM_TIMING: Alarm = Alarm { bands: &[
                    (100.0, COLOR_NORMAL),
                    (125.0, COLOR_WARN),
                    (f64::INFINITY, COLOR_CRITICAL),
                ]};

                // Visible bar count — matches sparkline slice
                let spark_width = SPARKLINE_CHARS as f32 * cw;
                let bar_count = (spark_width / MIN_BAR_WIDTH_PX).floor() as usize;

                // ── Row 1: SYSTEM | ASYNC | WORLD ──
                ui.columns(SECTION_COLS, |cols| {
                    let mem_ceiling = next_power_of_two_ceil(
                        self.hist_mem.0.iter().copied().fold(0.0_f64, f64::max)
                    ) as f32;

                    const ALARM_MEM: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };
                    const ALARM_NET: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };

                    draw_section(&mut cols[0], "SYSTEM", |ui| {
                        // FRAME: label | spark | ↑peak + !overruns
                        // ↑ wide (2D,1R): pad+↑+peak fills 7D; !+overruns:<5 fills 6D; gap=2
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "FRAME", numfmt::DEC5.fmt(self.field("frame_peak_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_frame.as_f32(), SparkScale::Fixed(125.0), &ALARM_TIMING, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::DEC5.fmt(self.hist_frame.visible_max(bar_count));
                            let ov = numfmt::INT5.fmt(self.hist_frame_overruns.visible_sum(bar_count));
                            seg_full(ui, &format!("{}↑{}  !{ov:<5}", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_NORMAL);
                            seg_gap(ui, cw);
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "MEM", numfmt::DEC5.fmt(self.field("memory_mb")), "MB"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_mem.as_f32(), SparkScale::Fixed(mem_ceiling), &ALARM_MEM, cw, rh);
                        });
                        // TICK: label | spark | ↑peak + !overruns
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "TICK", numfmt::DEC5.fmt(self.field("tick_peak_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_tick.as_f32(), SparkScale::Fixed(125.0), &ALARM_TIMING, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::DEC5.fmt(self.hist_tick.visible_max(bar_count));
                            let ov = numfmt::INT5.fmt(self.hist_tick_overruns.visible_sum(bar_count));
                            seg_full(ui, &format!("{}↑{}  !{ov:<5}", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_NORMAL);
                        });
                        // ↑NET: "↑NET" = 4R,5D (↑ wide) | spark | ↑peak (single stat, 7D + 8 trailing)
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("↑NET  {:>5} {:<2}", numfmt::DEC5.fmt(self.field("net_sent_bps")), "Bs"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_net_sent.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::DEC5.fmt(self.hist_net_sent.visible_max(bar_count));
                            seg_full(ui, &format!("{}↑{}        ", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_DIM);
                        });
                        // ↓NET: same pattern
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("↓NET  {:>5} {:<2}", numfmt::DEC5.fmt(self.field("net_recv_bps")), "Bs"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_net_recv.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::DEC5.fmt(self.hist_net_recv.visible_max(bar_count));
                            seg_full(ui, &format!("{}↑{}        ", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_DIM);
                        });
                        // Per-channel: label+queue | sparkline | buf% stat
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "ORD", numfmt::INT5.fmt(self.field("net_ord_queue")), "B"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_ord_queue.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "buf%", numfmt::DEC5.fmt(self.field("net_ord_buf_pct")), ""), COLOR_DIM);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "UNORD", numfmt::INT5.fmt(self.field("net_unord_queue")), "B"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_unord_queue.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "buf%", numfmt::DEC5.fmt(self.field("net_unord_buf_pct")), ""), COLOR_DIM);
                        });
                    });

                    const ALARM_ASYNC: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };

                    draw_section(&mut cols[1], "ASYNC", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "DUR", numfmt::DEC5.fmt(self.field("async.task_duration_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_dur.as_f32(), SparkScale::Auto, &ALARM_ASYNC, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::DEC5.fmt(self.hist_async_dur.visible_max(bar_count));
                            seg_full(ui, &format!("{}↑{}        ", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_DIM);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "QUEUE", numfmt::INT5.fmt(self.field("async.tasks_in_flight")), ""), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_queue.as_f32(), SparkScale::Auto, &ALARM_ASYNC, cw, rh);
                            seg_gap(ui, cw);
                            let pv = numfmt::INT5.fmt(self.hist_async_queue.visible_max(bar_count));
                            seg_full(ui, &format!("{}↑{}        ", " ".repeat(5usize.saturating_sub(pv.len())), pv), COLOR_DIM);
                        });
                    });

                    draw_section(&mut cols[2], "WORLD", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "#PLR", numfmt::INT5.fmt(self.field("connected_players")), ""), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "#NPC", numfmt::INT5.fmt(self.field("npc_count")), ""), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_full(ui, &format!("{:>5}  {:>5} {:<2}", "#HEX", numfmt::INT5.fmt(self.field("loaded_hexes")), ""), COLOR_DIM);
                        });
                    });
                });

            });
    }
}
