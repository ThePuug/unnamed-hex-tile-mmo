//! The window's look: its colours, its faces, and the painted controls
//! every panel is built from.

use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, pos2, vec2, Color32, CursorIcon, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind};

pub const INK: Color32 = Color32::from_rgb(0x15, 0x17, 0x1B);
pub const SHEET_INK: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x1E, 0x21, 0x28);
pub const RULE: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x37);
pub const BAR_LINE: Color32 = Color32::from_rgb(0x23, 0x26, 0x2D);
pub const EDGE: Color32 = Color32::from_rgb(0x3A, 0x3F, 0x4A);
pub const ROW_ON: Color32 = Color32::from_rgb(0x25, 0x29, 0x32);
pub const PARCHMENT: Color32 = Color32::from_rgb(0xED, 0xE6, 0xD6);
pub const MUTED: Color32 = Color32::from_rgb(0xA3, 0x9C, 0x8C);
pub const DOT: Color32 = Color32::from_rgb(0x5C, 0x5A, 0x55);
pub const LAMP: Color32 = Color32::from_rgb(0xD9, 0xA4, 0x41);
pub const LAMP_HOVER: Color32 = Color32::from_rgb(0xEB, 0xC0, 0x77);
pub const LAMP_LOW: Color32 = Color32::from_rgb(0x8A, 0x74, 0x48);
pub const SKY: Color32 = Color32::from_rgb(0x8F, 0xB3, 0xC9);
pub const READY: Color32 = Color32::from_rgb(0x7F, 0xA6, 0x7A);
pub const ALERT: Color32 = Color32::from_rgb(0xE0, 0x7A, 0x5F);

/// The sheet's voices, the most present first.
pub const VOICE_INKS: [Color32; 3] = [LAMP, SKY, READY];

/// The display face, for the piece's name.
pub const DISPLAY: &str = "display";

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn run(job: &mut LayoutJob, text: &str, size: f32, color: Color32) {
    job.append(text, 0.0, TextFormat { font_id: mono(size), color, ..Default::default() });
}

/// Spaced capitals, the window's small headings.
pub fn caps(text: &str, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, TextFormat { font_id: mono(size), color: MUTED, extra_letter_spacing: size * 0.17, ..Default::default() });
    job
}

#[derive(Clone, Copy)]
pub enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
    /// A MIDI socket: five pins in an arc.
    Midi,
}

pub fn paint_glyph(p: &egui::Painter, c: Pos2, glyph: Glyph, color: Color32) {
    let skip = |dir: f32| {
        p.line_segment([pos2(c.x + 5.0 * dir, c.y - 5.5), pos2(c.x + 5.0 * dir, c.y + 5.5)], Stroke::new(1.8_f32, color));
        let tri = vec![pos2(c.x - 5.0 * dir, c.y - 5.0), pos2(c.x + 2.0 * dir, c.y), pos2(c.x - 5.0 * dir, c.y + 5.0)];
        p.add(Shape::convex_polygon(tri, color, Stroke::NONE));
    };
    match glyph {
        Glyph::Previous => skip(-1.0),
        Glyph::Next => skip(1.0),
        Glyph::Pause => {
            for x in [c.x - 6.0, c.x + 2.0] {
                p.rect_filled(Rect::from_min_size(pos2(x, c.y - 7.0), vec2(4.0, 14.0)), 1.0, color);
            }
        }
        Glyph::Play => {
            let tri = vec![pos2(c.x - 4.0, c.y - 8.0), pos2(c.x + 8.0, c.y), pos2(c.x - 4.0, c.y + 8.0)];
            p.add(Shape::convex_polygon(tri, color, Stroke::NONE));
        }
        Glyph::Midi => {
            p.circle_stroke(c, 8.0, Stroke::new(1.6_f32, color));
            for k in 0..5 {
                let a = std::f32::consts::PI * (1.0 + k as f32 / 4.0);
                p.circle_filled(c + vec2(a.cos(), a.sin()) * 4.6, 1.3, color);
            }
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 6.6), vec2(3.0, 2.0)), 0.0, color);
        }
    }
}

/// A round transport button: the lamp-lit one is the main action.
pub fn round_button(ui: &mut egui::Ui, rect: Rect, id: &str, glyph: Glyph, lit: bool) -> Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    let (c, r) = (rect.center(), rect.width() / 2.0);
    if lit {
        p.circle_filled(c, r, if response.hovered() { LAMP_HOVER } else { LAMP });
        paint_glyph(p, c, glyph, INK);
    } else {
        p.circle_filled(c, r, if response.hovered() { RULE } else { PANEL });
        p.circle_stroke(c, r - 0.5, Stroke::new(1.0_f32, if response.hovered() { MUTED } else { EDGE }));
        paint_glyph(p, c, glyph, PARCHMENT);
    }
    response
}

pub fn check_box(p: &egui::Painter, at: Pos2, on: bool) {
    let rect = Rect::from_min_size(at, vec2(16.0, 16.0));
    if on {
        p.rect_filled(rect, 3.0, LAMP);
        let mark = vec![at + vec2(4.0, 8.2), at + vec2(6.8, 11.0), at + vec2(12.0, 5.2)];
        p.add(Shape::line(mark, Stroke::new(1.8_f32, INK)));
    } else {
        p.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
    }
}

pub fn flat_button(ui: &mut egui::Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).font(mono(11.0)).color(PARCHMENT)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(6.0).min_size(vec2(44.0, 28.0)))
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// A clickable rect at `rect`, its hover drawn by the caller.
pub fn hit(ui: &mut egui::Ui, rect: Rect, id: impl std::hash::Hash) -> Response {
    ui.interact(rect, ui.id().with(id), Sense::click()).on_hover_cursor(CursorIcon::PointingHand)
}

/// A small square icon button with nothing behind it until hovered.
pub fn icon_button(ui: &mut egui::Ui, rect: Rect, id: impl std::hash::Hash, paint: impl Fn(&egui::Painter, Pos2, Color32)) -> Response {
    let response = hit(ui, rect, id);
    let p = ui.painter();
    if response.hovered() {
        p.rect_filled(rect, 6.0, RULE);
    }
    paint(p, rect.center(), if response.hovered() { PARCHMENT } else { MUTED });
    response
}

pub fn chevron(p: &egui::Painter, c: Pos2, up: bool, color: Color32) {
    let dy = if up { -1.5 } else { 1.5 };
    p.add(Shape::line(vec![pos2(c.x - 3.0, c.y - dy), pos2(c.x, c.y + dy), pos2(c.x + 3.0, c.y - dy)], Stroke::new(1.6_f32, color)));
}

pub fn cross(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    p.line_segment([c + vec2(-3.0, -3.0), c + vec2(3.0, 3.0)], s);
    p.line_segment([c + vec2(3.0, -3.0), c + vec2(-3.0, 3.0)], s);
}

/// A full-width hairline between a popover's parts.
pub fn rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, RULE);
}

/// A dashed outline, the queue's empty box.
pub fn dashed(p: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(Shape::dashed_line(&corners, Stroke::new(1.0_f32, color), 4.0, 3.0));
}

pub fn pool_heading(ui: &mut egui::Ui, pool: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let heading = ui.painter().layout_job(caps(&pool.to_uppercase(), 10.0));
    ui.painter().galley(pos2(rect.left() + 14.0, rect.bottom() - 4.0 - heading.size().y), heading, MUTED);
}

pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A piece's name as a title: `overworld-ambient` reads Overworld Ambient.
pub fn title(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Egui's dark theme in the window's colours: hairlines in the rule's
/// colour, the text cursor and selection lamp-lit.
pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = INK;
    v.window_fill = PANEL;
    v.extreme_bg_color = INK;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, RULE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    v.widgets.inactive.bg_fill = EDGE;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.hovered.weak_bg_fill = RULE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    v.widgets.active.weak_bg_fill = EDGE;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, PARCHMENT);
    v.text_cursor.stroke = Stroke::new(1.5_f32, LAMP);
    v.selection.bg_fill = LAMP_LOW;
    v.selection.stroke = Stroke::new(1.0_f32, PARCHMENT);
    v
}

pub fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Plex".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../fonts/IBMPlexMono-Regular.ttf"))));
    fonts.font_data.insert("Cormorant".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../fonts/CormorantGaramond-SemiBold.ttf"))));
    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        fonts.families.entry(family).or_default().insert(0, "Plex".to_owned());
    }
    fonts.families.insert(FontFamily::Name(DISPLAY.into()), vec!["Cormorant".to_owned(), "Plex".to_owned()]);
    ctx.set_fonts(fonts);
}

