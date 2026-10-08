//! What every page draws with: a row of fixed-width segments, sparklines
//! scaled against a ceiling, alarm colours, and the boxed section.
//!
//! A segment is 15 characters, a half 7, a quarter 3, a wide two segments
//! and the gap between them; segments in a row are a character apart, so
//! columns line up whatever a row holds. Nerd Font glyphs take two cells
//! (`{:<2}`).

use eframe::egui::{self, Color32};

pub const FONT_SIZE: f32 = 12.0;
pub const SPARKLINE_CHARS: usize = 15;
const MIN_BAR_WIDTH_PX: f32 = 2.0;

pub const COLOR_NORMAL: Color32 = Color32::from_rgb(180, 255, 180);
pub const COLOR_CRITICAL: Color32 = Color32::from_rgb(255, 80, 80);
pub const COLOR_WARN: Color32 = Color32::from_rgb(255, 200, 60);
pub const COLOR_DIM: Color32 = Color32::from_rgb(120, 160, 120);
pub const COLOR_BORDER: Color32 = Color32::from_rgb(80, 120, 80);
pub const COLOR_BG: Color32 = Color32::from_rgb(10, 15, 10);
const COLOR_SPARK_BG: Color32 = Color32::from_rgb(30, 30, 35);

pub fn load_fonts(ctx: &egui::Context) {
    let font_bytes = include_bytes!("../../../assets/fonts/IosevkaNerdFont-Regular.ttf");
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Iosevka".to_owned(), std::sync::Arc::new(egui::FontData::from_static(font_bytes)));
    fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "Iosevka".to_owned());
    ctx.set_fonts(fonts);
}

pub fn mono_font() -> egui::FontId {
    egui::FontId::monospace(FONT_SIZE)
}

pub fn colored_mono(text: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: mono_font(), color, ..Default::default() });
    job
}

/// How many bars a sparkline shows: the window a page's peaks and sums
/// are taken over, so they agree with what is drawn.
pub fn bar_count(char_width: f32) -> usize {
    (SPARKLINE_CHARS as f32 * char_width / MIN_BAR_WIDTH_PX).floor() as usize
}

/// The power of two at or above `val`, at least 1: a ceiling that holds
/// still while a value wanders under it.
pub fn power_of_two_ceil(val: f64) -> f64 {
    if val <= 1.0 { 1.0 } else { 2.0_f64.powf(val.log2().ceil()) }
}

/// What a sparkline's bars are scaled against.
pub enum SparkScale {
    /// A known budget or limit.
    Fixed(f32),
    /// The most in the history shown.
    Auto,
}

/// Colour bands evaluated low to high: a value takes the first band whose
/// threshold it is under, e.g. `[(100, GREEN), (125, YELLOW), (INF, RED)]`.
pub struct Alarm {
    pub bands: &'static [(f64, Color32)],
}

impl Alarm {
    pub fn color(&self, val: f64) -> Color32 {
        self.bands.iter().find(|&&(threshold, _)| val < threshold).or(self.bands.last()).map_or(COLOR_NORMAL, |&(_, c)| c)
    }
}

pub const DIM: Alarm = Alarm { bands: &[(f64::INFINITY, COLOR_DIM)] };

/// Any value over zero is an alarm.
pub const ANY: Alarm = Alarm { bands: &[(0.5, COLOR_DIM), (f64::INFINITY, COLOR_CRITICAL)] };

fn draw_sparkline(ui: &mut egui::Ui, history: &[f32], scale: SparkScale, alarm: &Alarm, char_width: f32, row_height: f32) {
    let width = SPARKLINE_CHARS as f32 * char_width;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COLOR_SPARK_BG);

    let bars = bar_count(char_width);
    let samples = &history[history.len().saturating_sub(bars)..];
    if samples.is_empty() || bars == 0 {
        return;
    }
    let max_val = match scale {
        SparkScale::Fixed(v) => v,
        SparkScale::Auto => samples.iter().copied().fold(0.0_f32, f32::max),
    };
    if max_val <= 0.0 {
        return;
    }
    let bar_width = width / bars as f32;
    let offset = bars - samples.len();
    for (i, &val) in samples.iter().enumerate() {
        let bar_height = (val / max_val).clamp(0.0, 1.0) * rect.height();
        if bar_height < 0.5 {
            continue;
        }
        let x = rect.left() + (offset + i) as f32 * bar_width;
        let bar = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - bar_height), egui::vec2(bar_width, bar_height));
        painter.rect_filled(bar, 0.0, alarm.color(val as f64));
    }
}

/// A name as a half-segment: right-aligned when it fits, its first 7
/// characters when it does not.
pub fn fit_half(name: &str) -> String {
    if name.chars().count() > 7 { name.chars().take(7).collect() } else { format!("{name:>7}") }
}

/// A row of segments, each a character after the last.
pub struct Seg<'a> {
    ui: &'a mut egui::Ui,
    cw: f32,
    rh: f32,
    count: usize,
}

impl Seg<'_> {
    fn emit(&mut self, s: &str, chars: usize, color: Color32) {
        debug_assert_eq!(s.chars().count(), chars, "segment: {chars} chars expected, got {} for {s:?}", s.chars().count());
        if self.count > 0 {
            self.ui.add_space(self.cw);
        }
        self.ui.label(colored_mono(s, color));
        self.count += 1;
    }

    pub fn wide(&mut self, s: &str, color: Color32) {
        self.emit(s, 31, color);
    }

    pub fn full(&mut self, s: &str, color: Color32) {
        self.emit(s, 15, color);
    }

    pub fn half(&mut self, s: &str, color: Color32) {
        self.emit(s, 7, color);
    }

    pub fn quarter(&mut self, s: &str, color: Color32) {
        self.emit(s, 3, color);
    }

    pub fn spark(&mut self, history: &[f32], scale: SparkScale, alarm: &Alarm) {
        if self.count > 0 {
            self.ui.add_space(self.cw);
        }
        draw_sparkline(self.ui, history, scale, alarm, self.cw, self.rh);
        self.count += 1;
    }
}

pub fn seg_row(ui: &mut egui::Ui, cw: f32, rh: f32, f: impl FnOnce(&mut Seg)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        f(&mut Seg { ui, cw, rh, count: 0 });
    });
}

/// A labelled box as wide as its column.
pub fn draw_section(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.label(colored_mono(label, COLOR_BORDER));
    let avail = ui.available_width();
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0_f32, COLOR_BORDER)).inner_margin(4.0).show(ui, |ui| {
        ui.set_min_width(avail - 10.0);
        content(ui);
    });
}
