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
    let font_bytes = include_bytes!("../../../assets/fonts/IosevkaNerdFont-Regular.ttf");
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

use common::glyphs::*;

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
// Callers use standard format!() for layout. Nerd Font glyphs use {:<2} for 2-cell width.

/// 1-char gap between segments.
fn seg_gap(ui: &mut egui::Ui, char_width: f32) {
    ui.add_space(char_width);
}

/// Full segment: 15 characters.
fn seg_full(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(s.chars().count(), 15,
        "seg_full: 15 chars expected, got {} for {:?}", s.chars().count(), s);
    ui.label(colored_mono(s, color));
}

/// Half-segment: 7 characters.
fn seg_half(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(s.chars().count(), 7,
        "seg_half: 7 chars expected, got {} for {:?}", s.chars().count(), s);
    ui.label(colored_mono(s, color));
}

/// Row builder with auto-gapped segments.
struct Seg<'a> {
    ui: &'a mut egui::Ui,
    cw: f32,
    count: usize,
}

impl<'a> Seg<'a> {
    fn half(&mut self, s: &str, color: Color32) {
        if self.count > 0 { self.ui.add_space(self.cw); }
        seg_half(self.ui, s, color);
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

/// Quarter-segment: 3 characters.
#[allow(dead_code)]
fn seg_quarter(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(s.chars().count(), 3,
        "seg_quarter: 3 chars expected, got {} for {:?}", s.chars().count(), s);
    ui.label(colored_mono(s, color));
}

/// Eighth-segment: 1 character.
#[allow(dead_code)]
fn seg_eighth(ui: &mut egui::Ui, s: &str, color: Color32) {
    debug_assert_eq!(s.chars().count(), 1,
        "seg_eighth: 1 char expected, got {} for {:?}", s.chars().count(), s);
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

// ── Funnel visualization ──

/// Draw a narrowing funnel of horizontal bars. Each bar's width is proportional
/// to its count relative to the first stage's count.
/// suffix_half: optional 7-display-cell half-segment string (e.g., cache hit rate).
fn draw_funnel(
    ui: &mut egui::Ui,
    stages: &[(&str, f64, &str)], // (label, count, suffix_half)
    cw: f32,
    rh: f32,
) {
    if stages.is_empty() { return; }
    let max_val = stages[0].1.max(1.0);
    let bar_width_max = 31.0 * cw;

    use numfmt::{NumFmt, Precision, Overflow};
    const FUNNEL_VAL: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };

    for &(label, count, suffix) in stages {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            // Label half (7 display cells)
            seg_half(ui, &format!("{:>7}", label), COLOR_DIM);
            seg_gap(ui, cw);

            // Bar (31ch fixed width)
            let bar_frac = (count / max_val).clamp(0.0, 1.0) as f32;
            let filled_width = (bar_frac * bar_width_max).max(0.0);
            let bar_size = egui::vec2(bar_width_max, rh - 2.0);
            let (rect, _) = ui.allocate_exact_size(bar_size, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, COLOR_SPARK_BG);
            if filled_width > 0.5 {
                let filled = egui::Rect::from_min_size(rect.min, egui::vec2(filled_width, rect.height()));
                painter.rect_filled(filled, 0.0, COLOR_DIM);
            }
            if count == 0.0 {
                painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Outside);
            }

            seg_gap(ui, cw);

            // Value half (7 display cells)
            seg_half(ui, &format!("{:>5}  ", FUNNEL_VAL.fmt(count)), COLOR_DIM);

            // Optional suffix half (7 display cells, e.g., cache hit rate)
            if !suffix.is_empty() {
                seg_gap(ui, cw);
                seg_half(ui, suffix, COLOR_DIM);
            }
        });
    }
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

struct TimingEntry {
    hist_p95: History,
    hist_n: History,
}

impl TimingEntry {
    fn new() -> Self { Self { hist_p95: History::new(), hist_n: History::new() } }
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

    timing_entries: HashMap<String, TimingEntry>,
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
            timing_entries: HashMap::new(),
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

    fn handle_event(&mut self, packet: MetricsPacket) {
        if packet.group != "timings" { return; }

        for (field, val) in &packet.fields {
            if let Some(sys) = field.strip_suffix(".p95") {
                self.timing_entries.entry(sys.to_string())
                    .or_insert_with(TimingEntry::new)
                    .hist_p95.push(*val as f64);
            } else if let Some(sys) = field.strip_suffix(".n") {
                self.timing_entries.entry(sys.to_string())
                    .or_insert_with(TimingEntry::new)
                    .hist_n.push(*val as f64);
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
                        ui.label(colored_mono("CONNECTED", COLOR_NORMAL));
                    } else {
                        ui.label(colored_mono("NO SIGNAL", COLOR_CRITICAL));
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
                    use numfmt::{NumFmt, Precision, Overflow};
                    // Timing values (ms): collapsing decimal, suffixed
                    const TIME5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
                    // Counts (entities, overruns): integer, suffixed
                    const COUNT5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                    // Byte rates: collapsing decimal, suffixed
                    const RATE5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };

                    let mem_ceiling = next_power_of_two_ceil(
                        self.hist_mem.0.iter().copied().fold(0.0_f64, f64::max)
                    ) as f32;

                    const ALARM_MEM: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };
                    const ALARM_NET: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };

                    const OVERRUN: NumFmt = NumFmt { width: 1, precision: Precision::Integer, overflow: Overflow::Clamp };
                    const ALARM_OVERRUN: Alarm = Alarm { bands: &[
                        (0.5, COLOR_DIM),        // 0 = dim
                        (f64::INFINITY, COLOR_CRITICAL), // >0 = red
                    ]};

                    draw_section(&mut cols[0], "SYSTEM", |ui| {
                        // FRAME: label | value | spark | peak | !overruns | MEM label | MEM value | spark
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "FRAME"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", TIME5.fmt(self.field("frame_peak_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_frame.as_f32(), SparkScale::Fixed(125.0), &ALARM_TIMING, cw, rh);
                            seg_gap(ui, cw);
                            let pv = TIME5.fmt(self.hist_frame.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                            seg_gap(ui, cw);
                            let ov_val = self.hist_frame_overruns.visible_sum(bar_count);
                            let ov = OVERRUN.fmt(ov_val);
                            seg_quarter(ui, &format!("{}{:<2}", ov, GLYPH_OVERRUN), ALARM_OVERRUN.color(ov_val));
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>7}", "MEM"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", RATE5.fmt(self.field("memory_mb")), "MB"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_mem.as_f32(), SparkScale::Fixed(mem_ceiling), &ALARM_MEM, cw, rh);
                        });
                        // TICK: label | value | spark | peak | !overruns
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "TICK"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", TIME5.fmt(self.field("tick_peak_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_tick.as_f32(), SparkScale::Fixed(125.0), &ALARM_TIMING, cw, rh);
                            seg_gap(ui, cw);
                            let pv = TIME5.fmt(self.hist_tick.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                            seg_gap(ui, cw);
                            let ov_val = self.hist_tick_overruns.visible_sum(bar_count);
                            let ov = OVERRUN.fmt(ov_val);
                            seg_quarter(ui, &format!("{}{:<2}", ov, GLYPH_OVERRUN), ALARM_OVERRUN.color(ov_val));
                        });
                        // NET UP: label | value | spark | peak
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:<2}NET  ", GLYPH_NET_UP), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", RATE5.fmt(self.field("net_sent_bps")), "Bs"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_net_sent.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            let pv = RATE5.fmt(self.hist_net_sent.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                        // NET DOWN: same pattern
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:<2}NET  ", GLYPH_NET_DOWN), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", RATE5.fmt(self.field("net_recv_bps")), "Bs"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_net_recv.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            let pv = RATE5.fmt(self.hist_net_recv.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                        // Per-channel: label | queue | sparkline | buf% label | buf% value
                        const CHAN_QUEUE: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
                        const CHAN_BUF: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Clamp };
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "ORD"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", CHAN_QUEUE.fmt(self.field("net_ord_queue")), "B"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_ord_queue.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>7}", "buf%"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", CHAN_BUF.fmt(self.field("net_ord_buf_pct"))), COLOR_DIM);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "UNORD"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", CHAN_QUEUE.fmt(self.field("net_unord_queue")), "B"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_unord_queue.as_f32(), SparkScale::Auto, &ALARM_NET, cw, rh);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>7}", "buf%"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", CHAN_BUF.fmt(self.field("net_unord_buf_pct"))), COLOR_DIM);
                        });
                    });

                    const ALARM_ASYNC: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };

                    draw_section(&mut cols[1], "ASYNC", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "DUR"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}{:<2}", TIME5.fmt(self.field("async.task_duration_ms")), "ms"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_dur.as_f32(), SparkScale::Auto, &ALARM_ASYNC, cw, rh);
                            seg_gap(ui, cw);
                            let pv = TIME5.fmt(self.hist_async_dur.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "QUEUE"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", COUNT5.fmt(self.field("async.tasks_in_flight"))), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_spark(ui, &self.hist_async_queue.as_f32(), SparkScale::Auto, &ALARM_ASYNC, cw, rh);
                            seg_gap(ui, cw);
                            let pv = COUNT5.fmt(self.hist_async_queue.visible_max(bar_count));
                            seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, pv), COLOR_DIM);
                        });
                    });

                    draw_section(&mut cols[1], "TIMINGS", |ui| {
                        let mut names: Vec<&String> = self.timing_entries.keys().collect();
                        names.sort();
                        if names.is_empty() {
                            seg_row(ui, cw, |s| { s.half("     --", COLOR_DIM); });
                        }
                        for name in names {
                            let entry = &self.timing_entries[name];
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let display: String = if name.len() > 15 {
                                    name[..15].to_string()
                                } else {
                                    format!("{:>15}", name)
                                };
                                seg_full(ui, &display, COLOR_DIM);
                                seg_gap(ui, cw);
                                // Label: p95 per invocation
                                let p95_val = entry.hist_p95.0.back().copied().unwrap_or(0.0);
                                seg_half(ui, &format!("{:>5}{:<2}", TIME5.fmt(p95_val), "ms"), COLOR_DIM);
                                seg_gap(ui, cw);
                                // Sparkline: p95 per interval
                                seg_spark(ui, &entry.hist_p95.as_f32(), SparkScale::Fixed(125.0), &ALARM_TIMING, cw, rh);
                                seg_gap(ui, cw);
                                // Peak: max p95 in visible sparkline window
                                let peak = entry.hist_p95.visible_max(bar_count);
                                seg_half(ui, &format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(peak)), COLOR_DIM);
                                seg_gap(ui, cw);
                                // Overruns: count of intervals where p95 > 125ms
                                let start = entry.hist_p95.0.len().saturating_sub(bar_count);
                                let ov_val = entry.hist_p95.0.range(start..).filter(|&&v| v > 125.0).count() as f64;
                                let ov = OVERRUN.fmt(ov_val);
                                seg_quarter(ui, &format!("{}{:<2}", ov, GLYPH_OVERRUN), ALARM_OVERRUN.color(ov_val));
                                seg_gap(ui, cw);
                                // Total sample count across visible sparkline window
                                let total_n = entry.hist_n.visible_sum(bar_count);
                                seg_half(ui, &format!("n={:<5}", COUNT5.fmt(total_n)), COLOR_DIM);
                            });
                        }
                    });

                    draw_section(&mut cols[2], "WORLD", |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            seg_half(ui, &format!("{:>7}", "#PLR"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", COUNT5.fmt(self.field("connected_players"))), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>7}", "#NPC"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", COUNT5.fmt(self.field("npc_count"))), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>7}", "#HEX"), COLOR_DIM);
                            seg_gap(ui, cw);
                            seg_half(ui, &format!("{:>5}  ", COUNT5.fmt(self.field("loaded_hexes"))), COLOR_DIM);
                        });
                    });

                    draw_section(&mut cols[2], "EVENTS", |ui| {
                        let visible = self.field("evt.visible");
                        let t_hits = self.field("evt.tile_hits");
                        let t_misses = self.field("evt.tile_misses");
                        let t_total = t_hits + t_misses;
                        let tile_pct = if t_total > 0.0 { (t_hits / t_total * 100.0) as u32 } else { 0 };

                        let tile_pct = (tile_pct as u32).min(99);
                        // Composite row: cached tiles + tile hit%
                        seg_row(ui, cw, |s| {
                            s.half("       ", COLOR_DIM);
                            s.half(" cached", COLOR_DIM);
                            s.half(&format!("{:>5}  ", COUNT5.fmt(visible)), COLOR_DIM);
                            s.half(&format!("{:<2}{:>2}%  ", GLYPH_CACHE, tile_pct), COLOR_DIM);
                        });

                        // Per-event rows: index + cell hit%
                        for name in &["plates", "spines", "spawner"] {
                            let index = self.field(&format!("evt.{name}.index"));
                            let c_hits = self.field(&format!("evt.{name}.cell_hits"));
                            let c_misses = self.field(&format!("evt.{name}.cell_misses"));
                            let c_total = c_hits + c_misses;
                            let cell_pct = if c_total > 0.0 { (c_hits / c_total * 100.0) as u32 } else { 0 };
                            let cell_pct = cell_pct.min(99);

                            seg_row(ui, cw, |s| {
                                s.half(&format!("{:>7}", name), COLOR_DIM);
                                s.half("indexed", COLOR_DIM);
                                s.half(&format!("{:>5}  ", COUNT5.fmt(index)), COLOR_DIM);
                                s.half(&format!("{:<2}{:>2}%  ", GLYPH_CACHE, cell_pct), COLOR_DIM);
                            });
                        }
                    });
                });

            });
    }
}
