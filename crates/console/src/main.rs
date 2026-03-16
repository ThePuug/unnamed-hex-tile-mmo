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
    let font_bytes = include_bytes!("../assets/Iosevka-Regular.ttc");
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
const SPARKLINE_CHARS: usize = 14;
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
const EVENT_WINDOW_LEN: usize = 500;

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
    /// Count of most recent `n` samples exceeding a threshold.
    fn visible_count_above(&self, n: usize, threshold: f64) -> f64 {
        let start = self.0.len().saturating_sub(n);
        self.0.range(start..).filter(|&&v| v >= threshold).count() as f64
    }
}


struct EventWindow(VecDeque<f64>);
impl EventWindow {
    fn new() -> Self { Self(VecDeque::with_capacity(EVENT_WINDOW_LEN + 1)) }
    fn push(&mut self, val: f64) {
        if self.0.len() > EVENT_WINDOW_LEN { self.0.pop_front(); }
        self.0.push_back(val);
    }
    fn p95(&self) -> f64 {
        if self.0.is_empty() { return 0.0; }
        let mut sorted: Vec<f64> = self.0.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = (sorted.len() as f64 * 0.95).ceil() as usize;
        sorted[rank.saturating_sub(1)]
    }
}

// ── Pure formatting ──

/// Always returns exactly 5 characters (fractional).
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
    } else if v < 100_000.0 {
        format!("{:>5.0}", v)
    } else if v < 10_000_000.0 {
        format!("{:>4.0}K", v / 1_000.0)
    } else if v < 10_000_000_000.0 {
        format!("{:>4.0}M", v / 1_000_000.0)
    } else if v < 10_000_000_000_000.0 {
        format!("{:>4.0}B", v / 1_000_000_000.0)
    } else {
        format!("{:>4.0}T", v / 1_000_000_000_000.0)
    }
}

// ── Rect-based sparkline ──

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

/// Scale mode for sparkline Y axis.
enum SparkScale {
    /// Fixed ceiling — bars scaled against a known budget/limit.
    Fixed(f32),
    /// Auto-scale to the max value in the history.
    Auto,
}

fn draw_sparkline(
    ui: &mut egui::Ui, history: &[f32],
    scale: SparkScale, fixed_color: Option<Color32>,
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

    // Right-aligned: take the most recent `bar_count` samples.
    let n = history.len();
    let start = n.saturating_sub(bar_count);
    let samples = &history[start..];

    let max_val = match scale {
        SparkScale::Fixed(v) => v,
        SparkScale::Auto => samples.iter().cloned().fold(0.0_f32, f32::max),
    };
    if max_val <= 0.0 { return; }
    let bar_width = width / bar_count as f32;
    let offset = bar_count - samples.len(); // empty bars on the left

    for (i, &val) in samples.iter().enumerate() {
        let t = (val / max_val).clamp(0.0, 1.0);
        let bar_height = t * rect.height();
        if bar_height < 0.5 { continue; }
        let x = rect.left() + (offset + i) as f32 * bar_width;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - bar_height),
            egui::vec2(bar_width, bar_height),
        );
        let color = fixed_color.unwrap_or_else(|| lerp_color(COLOR_DIM, COLOR_CRITICAL, t));
        painter.rect_filled(bar_rect, 0.0, color);
    }
}

fn next_power_of_two_ceil(val: f64) -> f64 {
    if val <= 1.0 { return 1.0; }
    2.0_f64.powf(val.log2().ceil())
}

// ── Value types ──

#[derive(Clone, Copy)]
enum Val {
    Dec(f64),
    Int(f64),
}

impl Val {
    fn raw(self) -> f64 {
        match self { Val::Dec(v) | Val::Int(v) => v }
    }
    fn format(self) -> String {
        match self {
            Val::Dec(v) => format_value(v),
            Val::Int(v) => format_int(v),
        }
    }
    fn format_left(self) -> String {
        format!("{:<5}", self.format().trim_start())
    }
}

// ── Alarm bands ──

/// Color bands evaluated low-to-high. Value falls into the first band
/// whose threshold it is below. Example: `[(100, GREEN), (125, YELLOW), (INF, RED)]`
struct Alarm {
    bands: &'static [(f64, Color32)],
}

impl Alarm {
    const NONE: Self = Self { bands: &[(f64::INFINITY, COLOR_NORMAL)] };

    fn color(&self, val: f64) -> Color32 {
        for &(threshold, color) in self.bands {
            if val < threshold { return color; }
        }
        self.bands.last().map_or(COLOR_NORMAL, |&(_, c)| c)
    }
}

const COLOR_WARN: Color32 = Color32::from_rgb(255, 200, 60);

// ── Composable segments (each exactly 14 monospace chars) ──

/// 1-char gap between segments.
fn seg_gap(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(char_width);
}

/// "LABEL nnn.d uu" — 14 chars: 5 label + 1 sp + 5 value + 1 sp + 2 unit
fn seg_label(ui: &mut egui::Ui, label: &str, value: Val, unit: &str) {
    ui.label(colored_mono(
        &format!("{:>5} {} {:<2}", label, value.format(), unit),
        COLOR_DIM,
    ));
}

/// Sparkline rect — 14 chars wide
fn seg_spark(
    ui: &mut egui::Ui, history: &[f32],
    scale: SparkScale, fixed_color: Option<Color32>,
    char_width: f32, row_height: f32,
) {
    draw_sparkline(ui, history, scale, fixed_color, char_width, row_height);
}

/// Symbol width for stat segments.
#[derive(Clone, Copy)]
enum Sym {
    /// Single-cell glyph (e.g. '!')
    Narrow(char),
    /// Double-cell glyph (e.g. '↑')
    Wide(char),
}

impl Sym {
    fn cells(self) -> usize {
        match self { Sym::Narrow(_) => 1, Sym::Wide(_) => 2 }
    }
    fn as_str(self) -> String {
        match self { Sym::Narrow(c) | Sym::Wide(c) => c.to_string() }
    }
}

const SYM_UP: Sym = Sym::Wide('↑');
const SYM_BANG: Sym = Sym::Narrow('!');

/// Two symbol+value stats — always 14 chars total.
/// Each stat = symbol + 5-char value. Separator flexes to fill 14:
///   wide+wide: 2+5 + 0 + 2+5 = 14 (jammed together)
///   wide+narrow: 2+5 + 1 + 1+5 = 14
///   narrow+narrow: 1+5 + 2 + 1+5 = 14
fn seg_stats(
    ui: &mut egui::Ui,
    a: Option<(Sym, Val, &Alarm)>,
    b: Option<(Sym, Val, &Alarm)>,
) {
    let a_width = a.map_or(0, |(s, _, _)| s.cells() + 5);
    let b_width = b.map_or(0, |(s, _, _)| s.cells() + 5);
    let gap = 14_usize.saturating_sub(a_width + b_width);

    let (a_text, a_color) = match a {
        Some((s, v, alarm)) => (format!("{}{}", s.as_str(), v.format_left()), alarm.color(v.raw())),
        None => (String::new(), COLOR_DIM),
    };
    let (b_text, b_color) = match b {
        Some((s, v, alarm)) => (format!("{}{}", s.as_str(), v.format_left()), alarm.color(v.raw())),
        None => (String::new(), COLOR_DIM),
    };

    if !a_text.is_empty() {
        ui.label(colored_mono(&a_text, a_color));
    }
    let spacer = " ".repeat(gap);
    if !b_text.is_empty() {
        ui.label(colored_mono(&format!("{}{}", spacer, b_text), b_color));
    } else if !spacer.is_empty() {
        ui.label(colored_mono(&spacer, COLOR_DIM));
    }
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
            .with_inner_size([1410.0, 380.0])
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

    hist_qem_err: History, hist_qem_vtx: History, hist_qem_net: History,
    event_qem_err: EventWindow, event_qem_vtx: EventWindow, event_qem_net: EventWindow,

    hist_async_dur: History,
    hist_async_queue: History,
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
            hist_qem_err: History::new(), hist_qem_vtx: History::new(), hist_qem_net: History::new(),
            event_qem_err: EventWindow::new(), event_qem_vtx: EventWindow::new(), event_qem_net: EventWindow::new(),
            hist_async_dur: History::new(),
            hist_async_queue: History::new(),
        }
    }

    fn field(&self, name: &str) -> f64 {
        self.snapshot.get(name).copied().unwrap_or(0.0) as f64
    }

    fn poll(&mut self) {
        while let Ok(packet) = self.rx.try_recv() {
            self.last_received = Some(Instant::now());
            match packet.cadence {
                Cadence::Snapshot => self.handle_snapshot(packet),
                Cadence::Event => self.handle_event(packet),
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

        self.hist_qem_err.push(self.event_qem_err.p95());
        self.hist_qem_vtx.push(self.event_qem_vtx.p95());
        self.hist_qem_net.push(self.event_qem_net.p95());

        self.hist_async_dur.push(self.field("async.task_duration_ms"));
        self.hist_async_queue.push(self.field("async.tasks_in_flight"));
    }

    fn handle_event(&mut self, packet: MetricsPacket) {
        if packet.group == "qem" {
            for (name, val) in &packet.fields {
                match name.as_str() {
                    "geometric_error" => self.event_qem_err.push(*val as f64),
                    "render_compression" => self.event_qem_vtx.push(*val as f64),
                    "network_compression" => self.event_qem_net.push(*val as f64),
                    _ => {}
                }
            }
        }
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
                const ALARM_OVERRUNS: Alarm = Alarm { bands: &[
                    (1.0, COLOR_NORMAL),
                    (f64::INFINITY, COLOR_CRITICAL),
                ]};
                const ALARM_QEM_ERR: Alarm = Alarm { bands: &[
                    (2.0, COLOR_NORMAL),
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

                    draw_section(&mut cols[0], "SYSTEM", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "FRAME", Val::Dec(self.field("frame_peak_ms")), "ms");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_frame.as_f32(), SparkScale::Fixed(125.0), None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_frame.visible_max(bar_count)), &ALARM_TIMING)),
                                Some((SYM_BANG, Val::Int(self.hist_frame_overruns.visible_sum(bar_count)), &ALARM_OVERRUNS)),
                            );
                            seg_gap(ui, cw);
                            seg_label(ui, "MEM", Val::Dec(self.field("memory_mb")), "MB");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_mem.as_f32(), SparkScale::Fixed(mem_ceiling), Some(COLOR_DIM), cw, rh);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "TICK", Val::Dec(self.field("tick_peak_ms")), "ms");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_tick.as_f32(), SparkScale::Fixed(125.0), None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_tick.visible_max(bar_count)), &ALARM_TIMING)),
                                Some((SYM_BANG, Val::Int(self.hist_tick_overruns.visible_sum(bar_count)), &ALARM_OVERRUNS)),
                            );
                        });
                    });

                    draw_section(&mut cols[1], "ASYNC", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "DUR", Val::Dec(self.field("async.task_duration_ms")), "ms");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_dur.as_f32(), SparkScale::Auto, None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_async_dur.visible_max(bar_count)), &Alarm::NONE)),
                                None,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "QUEUE", Val::Int(self.field("async.tasks_in_flight")), "  ");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_queue.as_f32(), SparkScale::Auto, None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Int(self.hist_async_queue.visible_max(bar_count)), &Alarm::NONE)),
                                None,
                            );
                        });
                    });

                    draw_section(&mut cols[2], "WORLD", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "#PLR", Val::Int(self.field("connected_players")), "  ");
                            seg_gap(ui, cw);
                            seg_label(ui, "#NPC", Val::Int(self.field("npc_count")), "  ");
                            seg_gap(ui, cw);
                            seg_label(ui, "#HEX", Val::Int(self.field("loaded_hexes")), "  ");
                        });
                    });
                });

                ui.add_space(4.0);

                let err_p95 = self.event_qem_err.p95();
                let vtx_p95 = self.event_qem_vtx.p95();
                let net_p95 = self.event_qem_net.p95();

                ui.columns(SECTION_COLS, |cols| {
                    draw_section(&mut cols[0], "QEM", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "ERR", Val::Dec(err_p95), "wu");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_qem_err.as_f32(), SparkScale::Fixed(2.0), None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_qem_err.visible_max(bar_count)), &ALARM_QEM_ERR)),
                                Some((SYM_BANG, Val::Int(self.hist_qem_err.visible_count_above(bar_count, 2.0)), &ALARM_OVERRUNS)),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "VTX", Val::Dec(vtx_p95), "  ");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_qem_vtx.as_f32(), SparkScale::Fixed(1.0), None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_qem_vtx.visible_max(bar_count)), &Alarm::NONE)),
                                None,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_label(ui, "NET", Val::Dec(net_p95), "  ");
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_qem_net.as_f32(), SparkScale::Fixed(1.0), None, cw, rh);
                            seg_gap(ui, cw);
                            seg_stats(ui,
                                Some((SYM_UP, Val::Dec(self.hist_qem_net.visible_max(bar_count)), &Alarm::NONE)),
                                None,
                            );
                        });
                    });
                });
            });
    }
}
