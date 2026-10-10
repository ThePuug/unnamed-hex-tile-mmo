//! The window's panels: what plays and its transport at the top; under
//! it what to play — a band plays a track in a setting, at a seed — and
//! what plays next; the sheet at the foot; and the popovers, the credits
//! among them.

use std::time::{Duration, Instant};

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Layout, Rect, RichText, Sense, Shape, Stroke, StrokeKind};
use music::band::{self, Band};
use music::pieces::{Setting, Style, TRACKS};
use music::render::SAMPLE_RATE;
use music::SEEDS;

use crate::banks::Install;
use music::banks::folder;
use crate::midi::{self, Port};
use crate::player::{styles, Player, Popover, Take};
use crate::sheet::{Sheet, LANES};
use crate::theme::*;

/// The most a list grows before it scrolls.
const LIST_H: f32 = 320.0;

/// The setting's list: its width, to the setting field's left.
const SETTING_W: f32 = 220.0;

/// The band list: its width, to the band field's right.
const BAND_W: f32 = 360.0;

/// The MIDI popover's width.
const MIDI_W: f32 = 340.0;

/// The credits popover's width.
const CREDITS_W: f32 = 420.0;

/// The type the window is set in and its maker, each under the SIL Open
/// Font License, its text beside each font in `fonts/`.
const TYPE: &[(&str, &str)] = &[("IBM Plex Mono", "IBM"), ("Cormorant Garamond", "Christian Thalmann")];
const OFL: &str = "https://openfontlicense.org/";

/// The sheet: its height, how fast it scrolls, and where the playhead
/// stands in it.
const SHEET_H: f32 = 150.0;
const SHEET_PX_PER_S: f32 = 48.0;
const PLAYHEAD_X: f32 = 240.0;

/// The sheet's margin above its first lane and under its last, and the
/// band at a lane's top its voice is named in.
const LANE_PAD: f32 = 6.0;
const LABEL_H: f32 = 14.0;
/// How wide a note's head is drawn where it is struck, under a
/// sixteenth at the sheet's slowest.
const HEAD_W: f32 = 4.0;

/// A play up next.
const QUEUE_ROW_H: f32 = 36.0;

/// The selection's rows: the gap between them, a label's width and a
/// lock's.
const ROW_GAP: f32 = 8.0;
const LABEL_W: f32 = 64.0;
const LOCK_W: f32 = 32.0;

impl Player {
    /// The left half of the top: the track by name and where it came
    /// from; under it the band playing it, the setting and the seed; what
    /// it drew, and the summary of its score. Before the first play, how
    /// to start one.
    pub fn now_playing(&mut self, ui: &mut egui::Ui) {
        let Some(v) = self.current() else {
            ui.label(RichText::new("Nothing playing").font(FontId::new(30.0, FontFamily::Name(DISPLAY.into()))).color(MUTED));
            ui.add_space(4.0);
            ui.label(RichText::new("Play for the composer's draw, or play now from the selection.").font(mono(12.0)).color(MUTED));
            return;
        };
        let (piece, setting, band, seed, source) = (v.piece, v.setting, v.band, v.seed, v.source.label());
        let p = ui.painter().clone();
        let width = ui.available_width();
        let source = p.layout_job(caps(source, 10.0));
        let mut job = LayoutJob::default();
        job.append(&title(TRACKS[piece].name), 0.0, egui::TextFormat { font_id: FontId::new(30.0, FontFamily::Name(DISPLAY.into())), color: PARCHMENT, ..Default::default() });
        let name = fit(&p, job, width - 12.0 - source.size().x - 10.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, name.size().y), Sense::hover());
        // The display face sits high in its line: the chip's foot is lifted
        // to the name's baseline, about a fifth of its line over the bottom.
        let foot = rect.bottom() - name.size().y * 0.2;
        let chip = Rect::from_min_size(pos2(rect.left() + name.size().x + 12.0, foot - 16.0), vec2(source.size().x + 10.0, 18.0));
        p.galley(rect.left_top(), name, PARCHMENT);
        p.rect_stroke(chip, 3.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
        p.galley(chip.center() - source.size() / 2.0, source, MUTED);

        // Who plays it, where and at what seed; who the band is under the
        // pointer.
        ui.add_space(4.0);
        let mut job = LayoutJob::default();
        run(&mut job, band.name, 12.0, PARCHMENT);
        run(&mut job, &format!(" · in {} · seed {seed}", setting.name()), 12.0, MUTED);
        let who = fit(&p, job, width);
        let (line, _) = ui.allocate_exact_size(vec2(width, who.size().y), Sense::hover());
        let who_rect = Rect::from_min_size(line.min, who.size());
        p.galley(line.min, who, MUTED);
        ui.interact(who_rect, ui.id().with("band about"), Sense::hover()).on_hover_text(band.about);

        let Some(v) = self.current() else { return };
        ui.add_space(4.0);
        match (&v.take, &v.failed) {
            (_, Some(e)) => {
                ui.label(RichText::new(e).font(mono(12.0)).color(ALERT));
            }
            (Some(t), _) => {
                let s = &t.score;
                let facts = [s.story.to_string(), s.key.name(), s.meter.label(), format!("{:.0} bpm", s.eighth_bpm / 2.0)];
                let mut job = LayoutJob::default();
                for (i, fact) in facts.iter().enumerate() {
                    if i > 0 {
                        run(&mut job, "  ·  ", 12.0, DOT);
                    }
                    run(&mut job, fact, 12.0, LAMP);
                }
                let facts = fit(&p, job, width);
                let (line, _) = ui.allocate_exact_size(vec2(width, facts.size().y), Sense::hover());
                p.galley(line.left_top(), facts, LAMP);
                // The summary takes the lines the fixed top leaves it, cut
                // where it runs over, whole under the pointer.
                ui.add_space(4.0);
                let mut job = LayoutJob::default();
                job.append(&s.summary, 0.0, egui::TextFormat { font_id: mono(12.0), color: MUTED, line_height: Some(19.0), ..Default::default() });
                job.wrap = TextWrapping { max_width: width, max_rows: (ui.available_height() / 19.0).floor().max(1.0) as usize, break_anywhere: false, overflow_character: Some('…') };
                let summary = p.layout_job(job);
                let (rect, hover) = ui.allocate_exact_size(vec2(width, summary.size().y), Sense::hover());
                p.galley(rect.left_top(), summary, MUTED);
                hover.on_hover_text(&s.summary);
            }
            (None, None) => {
                ui.label(RichText::new(self.state(v).1).font(mono(12.0)).color(MUTED));
            }
        }
    }

    /// The right half of the top: repeat, previous, play or pause, next,
    /// autoplay and MIDI out; the section playing and the clock; the time
    /// bar.
    pub fn controls(&mut self, ui: &mut egui::Ui) -> Rect {
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(width, 60.0), Sense::hover());
        let (gap, small, big) = (14.0, 44.0, 60.0);
        let mut x = row.center().x - (5.0 * small + big + 5.0 * gap) / 2.0;
        let mut next_rect = |w: f32| {
            let r = Rect::from_min_size(pos2(x, row.center().y - w / 2.0), vec2(w, w));
            x += w + gap;
            r
        };
        let (repeat_r, previous_r, play_r, next_r, autoplay_r, midi) = (next_rect(small), next_rect(small), next_rect(big), next_rect(small), next_rect(small), next_rect(small));
        let repeat = if self.repeat { "Repeat: on. This play again after its rest." } else { "Repeat: off" };
        if round_button(ui, repeat_r, "repeat", Glyph::Repeat, self.repeat).on_hover_text(repeat).clicked() {
            self.repeat = !self.repeat;
        }
        if round_button(ui, previous_r, "previous", Glyph::Previous, false).clicked() {
            self.previous();
        }
        let toggle = if self.playing { Glyph::Pause } else { Glyph::Play };
        if round_button(ui, play_r, "play", toggle, true).clicked() {
            self.toggle_play();
        }
        if round_button(ui, next_r, "next", Glyph::Next, false).clicked() {
            self.next();
        }
        let autoplay = if self.autoplay { "Autoplay: on. When the queue runs out, a fresh play, changing what is not locked." } else { "Autoplay: off. When the queue runs out, the player stops." };
        if round_button(ui, autoplay_r, "autoplay", Glyph::Autoplay, self.autoplay).on_hover_text(autoplay).clicked() {
            self.autoplay = !self.autoplay;
        }
        let midi_on = self.midi.link.lock().unwrap().port != Port::Off;
        if round_button(ui, midi, "midi", Glyph::Midi, midi_on).on_hover_text("MIDI out").clicked() {
            if self.popover == Some(Popover::Midi) {
                self.popover = None;
            } else {
                self.ports = midi::ports();
                self.popover = Some(Popover::Midi);
            }
        }
        ui.add_space(16.0);

        let take = self.current().and_then(|v| v.take.clone());
        let total = take.as_ref().map_or(0.0, |t| t.audio.len() as f64 / SAMPLE_RATE as f64);
        let at = self.position().min(total);
        let (line, _) = ui.allocate_exact_size(vec2(width, 14.0), Sense::hover());
        let (where_, ink) = match (&take, self.rest_until) {
            (_, Some(until)) => (format!("rest {:.0} s", until.saturating_duration_since(Instant::now()).as_secs_f32().ceil()), MUTED),
            (Some(t), None) => {
                let section = t.score.sections.iter().rev().find(|s| t.score.seconds(s.start) <= at).map_or("", |s| s.name);
                // The bar too, so a moment heard can be named.
                (t.sheet.bar_at(at).map_or_else(|| section.to_string(), |b| format!("{section} · bar {b}")), PARCHMENT)
            }
            (None, None) => (self.current().map_or("nothing playing", |v| self.state(v).1).to_string(), MUTED),
        };
        let p = ui.painter();
        p.text(line.left_center(), Align2::LEFT_CENTER, where_, mono(11.0), ink);
        p.text(line.right_center(), Align2::RIGHT_CENTER, format!("{} / {}", clock(at), clock(total)), mono(11.0), MUTED);
        ui.add_space(6.0);
        self.timeline(ui, take.as_deref(), total, at);
        midi
    }

    /// The time bar: one segment a section, the ones played lit, the one
    /// playing half lit, a knob at the playhead; a press or a drag anywhere
    /// on it moves the playhead there.
    pub fn timeline(&mut self, ui: &mut egui::Ui, take: Option<&Take>, total: f64, at: f64) {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::click_and_drag());
        let p = ui.painter().clone();
        let cy = rect.center().y;
        let bar = |a: f32, b: f32, color: Color32| p.rect_filled(Rect::from_min_max(pos2(a, cy - 3.0), pos2(b, cy + 3.0)), 1.0, color);
        let Some(take) = take.filter(|_| total > 0.0) else {
            bar(rect.left(), rect.right(), RULE);
            return;
        };
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let x = |s: f64| rect.left() + (s / total).clamp(0.0, 1.0) as f32 * rect.width();
        let score = &take.score;
        let last = score.sections.len() - 1;
        for (i, section) in score.sections.iter().enumerate() {
            // A piece rings on past its last section; the ring is the
            // last section's.
            let (start, end) = (score.seconds(section.start), if i == last { total } else { score.seconds(section.end) });
            let color = if end <= at {
                LAMP
            } else if start <= at {
                LAMP_LOW
            } else {
                RULE
            };
            let (gap_a, gap_b) = (if i > 0 { 1.0 } else { 0.0 }, if i < last { 1.0 } else { 0.0 });
            bar(x(start) + gap_a, x(end) - gap_b, color);
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            self.seek((pointer.x - rect.left()) as f64 / rect.width() as f64 * total);
        }
        let knob = pos2(x(self.position().min(total)), cy);
        p.circle_filled(knob, 7.0, PARCHMENT);
        p.circle_stroke(knob, 6.0, Stroke::new(2.0_f32, INK));
    }

    /// What to play, a field a row — the band, the style, the track, the
    /// setting, the seed — each with its lock, what autoplay keeps; play
    /// now and add to the queue under them. Returns the band, style, track
    /// and setting fields' rects, where their lists hang.
    pub fn selection(&mut self, ui: &mut egui::Ui) -> [Rect; 4] {
        let top = ui.max_rect();
        let p = ui.painter().clone();
        let open = self.popover;
        // A row: its label, the field to the lock, the lock last.
        let row = |k: f32, label: &str| {
            let y = top.top() + k * (36.0 + ROW_GAP);
            p.text(pos2(top.left(), y + 18.0), Align2::LEFT_CENTER, label, mono(12.0), MUTED);
            let field = Rect::from_min_max(pos2(top.left() + LABEL_W, y), pos2(top.right() - LOCK_W - 6.0, y + 36.0));
            let lock = Rect::from_min_size(pos2(top.right() - LOCK_W, y + 2.0), vec2(LOCK_W, 32.0));
            (field, lock)
        };
        let toggle = |which: Popover| if open == Some(which) { None } else { Some(which) };

        let (band_rect, band_lock) = row(0.0, "band");
        let band = select_field(ui, band_rect, "band", self.band.map(|b| b.name), open == Some(Popover::Band));
        let band = match self.band {
            Some(b) => band.on_hover_text(b.about),
            None => band,
        };
        if band.clicked() {
            self.popover = toggle(Popover::Band);
        }
        let (style_rect, style_lock) = row(1.0, "style");
        if select_field(ui, style_rect, "style", self.style.map(Style::name), open == Some(Popover::Style)).clicked() {
            self.popover = toggle(Popover::Style);
        }
        let (track_rect, track_lock) = row(2.0, "track");
        let track = self.track.map(|t| title(TRACKS[t].name));
        if select_field(ui, track_rect, "track", track.as_deref(), open == Some(Popover::Track)).clicked() {
            self.popover = toggle(Popover::Track);
        }
        let (setting_rect, setting_lock) = row(3.0, "setting");
        let setting = self.chosen_setting();
        if select_field(ui, setting_rect, "setting", Some(setting_label(setting)), open == Some(Popover::Setting)).on_hover_text("The settings this track is made for, and none: the track as it is").clicked() {
            self.popover = toggle(Popover::Setting);
        }
        let (seed_cell, seed_lock) = row(4.0, "seed");
        let mut locks = self.locks;
        for (rect, id, what, on) in [(band_lock, "lock band", "band", &mut locks.band), (style_lock, "lock style", "style", &mut locks.style), (track_lock, "lock track", "track", &mut locks.track), (setting_lock, "lock setting", "setting", &mut locks.setting), (seed_lock, "lock seed", "seed", &mut locks.seed)] {
            let tip = if *on { format!("Locked: autoplay keeps the {what}") } else { format!("Unlocked: autoplay may change the {what}") };
            if lock_button(ui, rect, id, *on).on_hover_text(tip).clicked() {
                *on = !*on;
            }
        }
        if locks != self.locks {
            self.locks = locks;
            self.selected();
        }

        // The seed and its dice.
        let seed_rect = Rect::from_min_max(seed_cell.min, pos2(seed_cell.right() - 36.0 - 6.0, seed_cell.bottom()));
        let dice_rect = Rect::from_min_size(pos2(seed_rect.right() + 6.0, seed_cell.top()), vec2(36.0, 36.0));
        p.rect_filled(seed_rect, 6.0, INK);
        p.rect_stroke(seed_rect, 6.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
        let before = self.seed.clone();
        let field = egui::TextEdit::singleline(&mut self.seed).font(mono(12.0)).text_color(PARCHMENT).frame(false).margin(egui::Margin::symmetric(10, 10)).vertical_align(Align::Center);
        self.typing = ui.put(seed_rect, field).has_focus();
        self.seed.retain(|c| c.is_ascii_digit());
        self.seed.truncate(9);
        let dice = hit(ui, dice_rect, "dice").on_hover_text("A random seed");
        p.rect_filled(dice_rect, 6.0, if dice.hovered() { RULE } else { PANEL });
        p.rect_stroke(dice_rect, 6.0, Stroke::new(1.0_f32, if dice.hovered() { MUTED } else { EDGE }), StrokeKind::Inside);
        die(&p, dice_rect.center(), PARCHMENT);
        if dice.clicked() {
            self.seed = self.rng.below(SEEDS as usize).to_string();
        }
        if self.seed != before && self.locks.seed {
            self.selected();
        }

        // A band out of the track's style plays, untested.
        let note_y = seed_cell.bottom() + 10.0;
        if let Some((band, track)) = self.band.zip(self.track).filter(|(b, t)| b.style != TRACKS[*t].style) {
            let mut job = LayoutJob::default();
            run(&mut job, &format!("a {} band on a {} track: out of its style, untested", band.style.name(), TRACKS[track].style.name()), 11.0, DOT);
            let note = fit(&p, job, top.width() - LABEL_W);
            p.galley(pos2(top.left() + LABEL_W, note_y), note, DOT);
        }

        // Play now, and add to the queue, at the column's foot, once a band,
        // a track and a seed are chosen.
        let half = (top.width() - 10.0) / 2.0;
        let now_rect = Rect::from_min_size(pos2(top.left(), top.bottom() - 40.0), vec2(half, 40.0));
        let add_rect = Rect::from_min_size(pos2(now_rect.right() + 10.0, now_rect.top()), vec2(half, 40.0));
        let chosen = self.band.zip(self.track).zip(self.typed_seed());
        let now = hit(ui, now_rect, "play now");
        p.rect_filled(now_rect, 6.0, if chosen.is_none() { LAMP_LOW } else if now.hovered() { LAMP_HOVER } else { LAMP });
        let t = p.layout_no_wrap("Play now".to_string(), mono(12.0), INK);
        let start = now_rect.center() - vec2((t.size().x + 18.0) / 2.0, 0.0);
        p.add(Shape::convex_polygon(vec![start + vec2(0.0, -5.0), start + vec2(9.0, 0.0), start + vec2(0.0, 5.0)], INK, Stroke::NONE));
        p.galley(pos2(start.x + 18.0, now_rect.center().y - t.size().y / 2.0), t, INK);
        let add = hit(ui, add_rect, "add");
        p.rect_filled(add_rect, 6.0, if add.hovered() { RULE } else { Color32::TRANSPARENT });
        p.rect_stroke(add_rect, 6.0, Stroke::new(1.0_f32, LAMP), StrokeKind::Inside);
        let t = p.layout_no_wrap("Add to queue".to_string(), mono(12.0), PARCHMENT);
        let start = add_rect.center() - vec2((t.size().x + 20.0) / 2.0, 0.0);
        let plus = Stroke::new(1.6_f32, LAMP);
        p.line_segment([start + vec2(5.0, -5.0), start + vec2(5.0, 5.0)], plus);
        p.line_segment([start + vec2(0.0, 0.0), start + vec2(10.0, 0.0)], plus);
        p.galley(pos2(start.x + 20.0, add_rect.center().y - t.size().y / 2.0), t, PARCHMENT);
        if let Some(((band, track), seed)) = chosen {
            if now.clicked() {
                self.play_now(track, band, setting, seed);
            }
            if add.clicked() {
                self.enqueue(track, band, setting, seed);
            }
        }
        [band_rect, style_rect, track_rect, setting_rect]
    }

    /// Up next: the queue, in order, each moved up or down or taken out,
    /// and after it what the composer plays when it runs out, rolled
    /// again at its die.
    pub fn up_next(&mut self, ui: &mut egui::Ui) {
        let top = ui.max_rect();
        let p = ui.painter().clone();
        let heading = p.layout_job(caps("UP NEXT", 10.0));
        p.galley(pos2(top.left(), top.top() + 13.0 - heading.size().y / 2.0), heading, MUTED);
        let clear = ui.put(Rect::from_min_size(pos2(top.right() - 56.0, top.top()), vec2(56.0, 26.0)), |ui: &mut egui::Ui| flat_button(ui, "Clear"));
        if clear.clicked() {
            self.queue.clear();
        }

        let list = Rect::from_min_max(pos2(top.left(), top.top() + 34.0), top.right_bottom());
        let mut moves: Vec<(usize, isize)> = Vec::new();
        let mut drops: Vec<usize> = Vec::new();
        let rows = self.queue.iter().map(|v| (v.piece, format!(" · {} · {} · seed {}", v.band.name, v.setting.name(), v.seed), self.state(v))).collect::<Vec<_>>();
        let then = match &self.composed {
            Some(v) if self.autoplay => format!("then {} · {} · {} · seed {}", title(TRACKS[v.piece].name), v.band.name, v.setting.name(), v.seed),
            _ => "then stop".to_string(),
        };
        let then_state = match &self.composed {
            _ if !self.autoplay => (DOT, "autoplay off"),
            Some(v) if self.queue.is_empty() => self.state(v),
            _ => (DOT, "waits for the queue"),
        };
        let rollable = self.autoplay && self.composed.is_some();
        let mut rolled = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            egui::ScrollArea::vertical().max_height(list.height()).auto_shrink([false, false]).show(ui, |ui| {
                for (i, (piece, detail, (dot, state))) in rows.iter().enumerate() {
                    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), QUEUE_ROW_H), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(row, 6.0, PANEL);
                    let cy = row.center().y;
                    p.text(pos2(row.left() + 12.0, cy), Align2::LEFT_CENTER, (i + 1).to_string(), mono(11.0), MUTED);
                    // Only the head is rendered ahead; the rest wait.
                    let (dot, state, ink) = if i == 0 { (*dot, *state, PARCHMENT) } else { (DOT, "waiting", MUTED) };
                    let buttons = row.right() - 6.0 - 3.0 * 28.0;
                    let status = p.layout_no_wrap(state.to_string(), mono(11.0), ink);
                    let sx = buttons - 10.0 - status.size().x;
                    p.circle_filled(pos2(sx - 9.0, cy), 3.5, dot);
                    p.galley(pos2(sx, cy - status.size().y / 2.0), status, ink);
                    let mut job = LayoutJob::default();
                    run(&mut job, &title(TRACKS[*piece].name), 12.0, PARCHMENT);
                    run(&mut job, detail, 12.0, MUTED);
                    let line = fit(p, job, sx - 9.0 - 3.5 - 10.0 - (row.left() + 34.0));
                    p.galley(pos2(row.left() + 34.0, cy - line.size().y / 2.0), line, PARCHMENT);
                    let cell = |k: f32| Rect::from_min_size(pos2(buttons + k * 28.0, cy - 14.0), vec2(28.0, 28.0));
                    if icon_button(ui, cell(0.0), ("up", i), |p, c, ink| chevron(p, c, true, ink)).on_hover_text("Move up").clicked() {
                        moves.push((i, -1));
                    }
                    if icon_button(ui, cell(1.0), ("down", i), |p, c, ink| chevron(p, c, false, ink)).on_hover_text("Move down").clicked() {
                        moves.push((i, 1));
                    }
                    if icon_button(ui, cell(2.0), ("drop", i), cross).on_hover_text("Remove from the queue").clicked() {
                        drops.push(i);
                    }
                    ui.add_space(4.0);
                }
                // What the composer plays when the queue runs out, or that
                // the player stops; its die rolls the draw again.
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), QUEUE_ROW_H), Sense::hover());
                let p = ui.painter();
                dashed(p, row, EDGE);
                let cy = row.center().y;
                let (dot, state) = then_state;
                p.circle_filled(pos2(row.left() + 15.5, cy), 3.5, dot);
                let status = p.layout_no_wrap(state.to_string(), mono(11.0), MUTED);
                let right = if rollable { row.right() - 6.0 - 28.0 - 10.0 } else { row.right() - 12.0 };
                let sx = right - status.size().x;
                p.galley(pos2(sx, cy - status.size().y / 2.0), status, MUTED);
                let mut job = LayoutJob::default();
                run(&mut job, &then, 12.0, MUTED);
                let line = fit(p, job, sx - 10.0 - (row.left() + 28.0));
                p.galley(pos2(row.left() + 28.0, cy - line.size().y / 2.0), line, MUTED);
                if rollable {
                    let cell = Rect::from_min_size(pos2(row.right() - 6.0 - 28.0, cy - 14.0), vec2(28.0, 28.0));
                    if icon_button(ui, cell, "roll then", die).on_hover_text("Roll another: a fresh draw of what is not locked").clicked() {
                        rolled = true;
                    }
                }
            });
        });
        if rolled {
            self.selected();
        }
        for (i, by) in moves {
            let j = i as isize + by;
            if j >= 0 && (j as usize) < self.queue.len() {
                self.queue.swap(i, j as usize);
            }
        }
        for i in drops.into_iter().rev() {
            self.queue.remove(i);
        }
    }

    /// The sheet: each section's three most present voices scrolling past
    /// the playhead, one to a lane and each lane spanning its voice's
    /// pitches, what has played dimmed; the voice named where a section
    /// changes who is in its lane. Over it, the voices in the lanes at
    /// the playhead, and at its right the credits, the banks and anything
    /// wrong with the sound. Returns the credits' rect, where their popover
    /// stands.
    pub fn sheet(&mut self, ui: &mut egui::Ui) -> Rect {
        let mut credits = Rect::NOTHING;
        let take = self.current().and_then(|v| v.take.clone());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.label(caps("SHEET", 10.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let small = |text: String, color: Color32| RichText::new(text).font(mono(11.0)).color(color);
                let open = self.popover == Some(Popover::Credits);
                let link = ui.add(egui::Label::new(small("credits".to_string(), if open { PARCHMENT } else { MUTED }).underline()).sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand);
                if link.clicked() {
                    self.popover = if open { None } else { Some(Popover::Credits) };
                }
                credits = link.rect;
                if let Some((text, color)) = self.banks_state() {
                    let folder = folder().map_or_else(|| "none".to_string(), |f| f.display().to_string());
                    ui.label(small(text, color)).on_hover_text(format!("Drop a music-banks .7z on this window, or unpack it into\n{folder}\nand start the player again."));
                }
                if let Some(e) = &self.output_error {
                    ui.label(small(format!("audio output: {e}"), ALERT));
                }
                if let Some(Err(e)) = &self.bank {
                    ui.label(small(format!("SoundFont: {e}"), ALERT));
                }
                // The parts take the room left of the credits and the
                // banks, in a strip the wheel scrolls where they run past it.
                if let Some(t) = &take {
                    let strip = vec2(ui.available_width(), 18.0);
                    ui.allocate_ui_with_layout(strip, Layout::left_to_right(Align::Center), |ui| self.parts(ui, t));
                }
            });
        });
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), SHEET_H), Sense::hover());
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 6.0, SHEET_INK);
        let Some(take) = take else {
            p.text(rect.center(), Align2::CENTER_CENTER, self.current().map_or("nothing playing", |v| self.state(v).1), mono(12.0), MUTED);
            p.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, RULE), StrokeKind::Inside);
            return credits;
        };
        // The lanes are the mix's where a mix plays: a muted part gives
        // its lane to the next most present.
        let heard = self.heard_mix();
        let sheet: &Sheet = heard.as_ref().map_or(&take.sheet, |m| &m.sheet);
        let at = self.position();
        let x = |t: f64| rect.left() + PLAYHEAD_X + ((t - at) as f32) * SHEET_PX_PER_S;
        let (from, to) = (at - (PLAYHEAD_X / SHEET_PX_PER_S) as f64, at + ((rect.width() - PLAYHEAD_X) / SHEET_PX_PER_S) as f64);
        // Every bar line where the tempo puts it, numbered as a musician
        // counts.
        for (i, b) in sheet.bars.iter().enumerate().filter(|(_, b)| **b >= from && **b <= to) {
            let bx = x(*b);
            p.line_segment([pos2(bx, rect.top()), pos2(bx, rect.bottom())], Stroke::new(1.0_f32, BAR_LINE));
            p.text(pos2(bx + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, (i + 1).to_string(), mono(9.0), MUTED);
        }
        let lane_h = (rect.height() - 2.0 * LANE_PAD) / LANES as f32;
        for lane in 1..LANES {
            let ly = rect.top() + LANE_PAD + lane as f32 * lane_h;
            p.line_segment([pos2(rect.left(), ly), pos2(rect.right(), ly)], Stroke::new(1.0_f32, BAR_LINE));
        }
        for (lane, stretches) in sheet.lanes.iter().enumerate() {
            let top = rect.top() + LANE_PAD + lane as f32 * lane_h;
            let ink = VOICE_INKS[lane];
            for stretch in stretches.iter().filter(|s| s.to >= from && s.from <= to) {
                let cell = Rect::from_min_max(pos2(x(stretch.from).max(rect.left()), top), pos2(x(stretch.to).min(rect.right()), top + lane_h)).intersect(rect);
                let p = p.with_clip_rect(cell);
                let voice = &sheet.voices[stretch.voice];
                // The voice's pitches fill the lane under its name.
                let (low, high) = (top + lane_h - 4.0, top + LABEL_H);
                let y = |pitch: u8| low - (pitch - voice.lo) as f32 / (voice.hi - voice.lo) as f32 * (low - high);
                // A note is a head where it is struck and a hairline while
                // it carries, the heads over every line, so a tone ringing
                // on under the next ones leaves them in sight.
                let shown = || voice.notes.iter().filter(|n| n.1 >= from && n.0 <= to && n.0 >= stretch.from && n.0 < stretch.to);
                let ink_at = |end: f64| if end < at { ink.gamma_multiply(0.35) } else { ink };
                for (start, end, pitch) in shown() {
                    let line = [pos2(x(*start), y(*pitch)), pos2(x(*end) - 1.0, y(*pitch))];
                    p.line_segment(line, Stroke::new(1.0_f32, ink_at(*end).gamma_multiply(0.6)));
                }
                for (start, end, pitch) in shown() {
                    let a = x(*start);
                    let head = Rect::from_min_max(pos2(a, y(*pitch) - 2.0), pos2((a + HEAD_W).min(x(*end) - 1.0).max(a + 2.0), y(*pitch) + 2.0));
                    p.rect_filled(head, 1.0, ink_at(*end));
                }
                p.text(pos2(cell.left() + 6.0, top + 2.0), Align2::LEFT_TOP, voice.name, mono(10.0), ink.gamma_multiply(0.75));
            }
        }
        let head = rect.left() + PLAYHEAD_X;
        p.line_segment([pos2(head, rect.top()), pos2(head, rect.bottom())], Stroke::new(2.0_f32, PARCHMENT.gamma_multiply(0.85)));
        p.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, RULE), StrokeKind::Inside);
        credits
    }

    /// Every part of the play, each with its lane's ink where it holds
    /// one under the playhead; a click mutes it, struck out until the mix
    /// without it arrives and plays.
    fn parts(&mut self, ui: &mut egui::Ui, t: &Take) {
        let heard = self.heard_mix();
        let lanes = heard.as_ref().map_or(&t.sheet, |m| &m.sheet).at(self.position());
        // The plain wheel scrolls the strip: egui gives a one-way area the
        // wheel's other axis only when told to.
        ui.style_mut().always_scroll_the_only_direction = true;
        egui::ScrollArea::horizontal().id_salt("parts").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for inst in &t.score.instruments {
                    let sheet = heard.as_ref().map_or(&t.sheet, |m| &m.sheet);
                    let lane = lanes.iter().position(|v| v.is_some_and(|v| sheet.voices[v].name == inst.name));
                    let muted = self.muted.contains(inst.name);
                    let silent = muted && heard.as_ref().is_some_and(|m| m.muted.contains(&inst.name));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (swatch, _) = ui.allocate_exact_size(vec2(10.0, 4.0), Sense::hover());
                        if let Some(lane) = lane {
                            ui.painter().rect_filled(swatch, 1.0, VOICE_INKS[lane]);
                        }
                        let text = RichText::new(inst.name).font(mono(11.0)).color(if silent { DOT } else if muted { MUTED } else { PARCHMENT });
                        let text = if muted { text.strikethrough() } else { text };
                        let part = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_cursor(CursorIcon::PointingHand).on_hover_text(if muted { "unmute" } else { "mute" });
                        if part.clicked() {
                            self.toggle_mute(inst.name);
                        }
                    });
                }
            });
        });
    }

    /// What the banks are doing, as a word and its colour: an install's
    /// progress or failure, else how many sampled banks play.
    fn banks_state(&self) -> Option<(String, Color32)> {
        if let Some(install) = &self.install {
            return Some(match &*install.lock().unwrap() {
                Install::Unpacking { done, total } => (format!("unpacking banks {}%", done * 100 / (*total).max(1)), LAMP),
                Install::Failed(e) => (format!("banks: {e}"), ALERT),
                Install::Installed => ("loading banks…".to_string(), MUTED),
            });
        }
        match self.bank {
            Some(Ok(0)) => Some(("GeneralUser only · drop music-banks .7z here".to_string(), MUTED)),
            Some(Ok(n)) => Some((format!("{n} sampled banks"), MUTED)),
            Some(Err(_)) => None,
            None => Some(("loading banks…".to_string(), MUTED)),
        }
    }

    /// While files are carried over the window, a sign that dropping one
    /// installs it; a file dropped is installed as a banks archive.
    fn drop_banks(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.screen_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            p.rect_filled(screen, 0.0, INK.gamma_multiply(0.9));
            dashed(&p, screen.shrink(16.0), LAMP);
            p.text(screen.center(), Align2::CENTER_CENTER, "Drop the music-banks .7z to install it", mono(14.0), PARCHMENT);
        }
        if let Some(archive) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.install(archive);
        }
    }

    /// The open popover, if any: the band, style, track and setting lists
    /// hung under their fields, MIDI out under its button, the credits over
    /// theirs. A press outside it or Escape closes it.
    pub fn popover(&mut self, ctx: &egui::Context, [band_anchor, style_anchor, track_anchor, setting_anchor]: [Rect; 4], midi_anchor: Rect, credits_anchor: Rect) {
        let Some(open) = self.popover else { return };
        let anchor = match open {
            Popover::Band => band_anchor,
            Popover::Style => style_anchor,
            Popover::Track => track_anchor,
            Popover::Setting => setting_anchor,
            Popover::Midi => midi_anchor,
            Popover::Credits => credits_anchor,
        };
        let below = (ctx.screen_rect().bottom() - anchor.bottom() - 8.0 - 24.0).clamp(68.0, LIST_H);
        let area = match open {
            Popover::Band => egui::Area::new(egui::Id::new("band")).order(egui::Order::Foreground).fixed_pos(pos2(anchor.left(), anchor.bottom() + 6.0)).show(ctx, |ui| {
                if let Some(band) = band_list(ui, self.band, self.style, below) {
                    self.band = Some(band);
                    self.popover = None;
                    self.selected();
                }
            }),
            Popover::Style => {
                let width = anchor.width();
                egui::Area::new(egui::Id::new("style")).order(egui::Order::Foreground).fixed_pos(pos2(anchor.left(), anchor.bottom() + 6.0)).show(ctx, |ui| {
                    if let Some(style) = style_list(ui, width, self.style) {
                        self.popover = None;
                        self.choose_style(style);
                    }
                })
            }
            Popover::Track => {
                let width = anchor.width();
                egui::Area::new(egui::Id::new("track")).order(egui::Order::Foreground).fixed_pos(pos2(anchor.left(), anchor.bottom() + 6.0)).show(ctx, |ui| self.track_list(ui, width, below))
            }
            Popover::Setting => egui::Area::new(egui::Id::new("setting")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(pos2(anchor.right(), anchor.bottom() + 6.0)).show(ctx, |ui| self.setting_list(ui)),
            Popover::Midi => egui::Area::new(egui::Id::new("midi")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(pos2(anchor.right(), anchor.bottom() + 8.0)).show(ctx, |ui| self.midi_list(ui)),
            Popover::Credits => {
                let list_h = (anchor.top() - 8.0 - 24.0).max(120.0);
                egui::Area::new(egui::Id::new("credits")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(pos2(anchor.right(), anchor.top() - 8.0)).show(ctx, |ui| credits_list(ui, list_h))
            }
        };
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p)));
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.popover = None;
        }
    }

    /// MIDI out's ports as listed when the popover opened, the open one
    /// marked; what MIDI out does, and why a port did not open.
    pub fn midi_list(&mut self, ui: &mut egui::Ui) {
        let (open, error) = {
            let link = self.midi.link.lock().unwrap();
            (link.port.clone(), link.error.clone())
        };
        let mut picked = None;
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
            ui.set_width(MIDI_W - 2.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            pool_heading(ui, "midi out");
            for port in &self.ports {
                let on = *port == open;
                let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let row = row.on_hover_cursor(CursorIcon::PointingHand);
                let p = ui.painter();
                if row.hovered() {
                    p.rect_filled(rect, 0.0, RULE);
                } else if on {
                    p.rect_filled(rect, 0.0, ROW_ON);
                }
                let dot = pos2(rect.left() + 22.0, rect.center().y);
                p.circle_stroke(dot, 7.0, Stroke::new(1.0_f32, if on { LAMP } else { EDGE }));
                if on {
                    p.circle_filled(dot, 3.5, LAMP);
                }
                p.text(pos2(rect.left() + 40.0, rect.center().y), Align2::LEFT_CENTER, port.label(), mono(12.0), if on { PARCHMENT } else { MUTED });
                if row.clicked() {
                    picked = Some(port.clone());
                }
            }
            ui.add_space(6.0);
            rule(ui);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Some(e) = &error {
                    ui.add(egui::Label::new(RichText::new(e).font(mono(11.0)).color(ALERT)).wrap());
                }
                let mut note = "While a port is open the player is silent: the notes and a MIDI clock go to the port, and the program on it plays them.".to_string();
                if cfg!(windows) {
                    note += " Windows gives a program no port of its own: to reach a DAW here, make one with loopMIDI and pick it.";
                }
                ui.add(egui::Label::new(RichText::new(note).font(mono(11.0)).color(MUTED).line_height(Some(17.0))).wrap());
            });
        });
        if let Some(port) = picked {
            self.midi.choose(port);
            self.popover = None;
        }
    }

    /// The chosen style's tracks, else every style's under its name, one
    /// to pick, each with the settings it is made for.
    pub fn track_list(&mut self, ui: &mut egui::Ui, width: f32, list_h: f32) {
        let mut picked = None;
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
            ui.set_width(width - 2.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
                for (style, members) in styles().into_iter().filter(|(s, _)| self.style.is_none_or(|c| c == *s)) {
                    pool_heading(ui, &format!("{} tracks", style.name()));
                    for i in members {
                        let on = Some(i) == self.track;
                        let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        let row = row.on_hover_cursor(CursorIcon::PointingHand);
                        let p = ui.painter();
                        if row.hovered() {
                            p.rect_filled(rect, 0.0, RULE);
                        } else if on {
                            p.rect_filled(rect, 0.0, ROW_ON);
                        }
                        let made: Vec<&str> = TRACKS[i].settings.iter().map(|s| s.name()).collect();
                        let made = if made.is_empty() { "none".to_string() } else { made.join(" · ") };
                        let made = p.layout_no_wrap(made, mono(11.0), DOT);
                        let name = p.layout_no_wrap(title(TRACKS[i].name), mono(12.0), if on { PARCHMENT } else { MUTED });
                        p.galley(pos2(rect.left() + 14.0, rect.center().y - name.size().y / 2.0), name.clone(), PARCHMENT);
                        if rect.left() + 14.0 + name.size().x + 16.0 + made.size().x <= rect.right() - 14.0 {
                            p.galley(pos2(rect.right() - 14.0 - made.size().x, rect.center().y - made.size().y / 2.0), made, DOT);
                        }
                        if row.clicked() {
                            picked = Some(i);
                        }
                    }
                }
                ui.add_space(8.0);
            });
        });
        if let Some(i) = picked {
            self.popover = None;
            self.choose_track(i);
        }
    }

    /// The settings to choose from, one to pick.
    pub fn setting_list(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let taken = self.chosen_setting();
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
            ui.set_width(SETTING_W - 2.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            ui.add_space(4.0);
            for setting in self.settings() {
                let on = setting == taken;
                let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let row = row.on_hover_cursor(CursorIcon::PointingHand);
                let p = ui.painter();
                if row.hovered() {
                    p.rect_filled(rect, 0.0, RULE);
                } else if on {
                    p.rect_filled(rect, 0.0, ROW_ON);
                }
                p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, setting.name(), mono(12.0), if on { PARCHMENT } else { MUTED });
                if setting == Setting::None {
                    p.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, "as it is", mono(11.0), DOT);
                }
                if row.clicked() {
                    picked = Some(setting);
                }
            }
            ui.add_space(4.0);
        });
        if let Some(setting) = picked {
            self.setting = setting;
            self.popover = None;
            self.selected();
        }
    }
}

/// Every style, `taken` marked; the one pressed, if any.
fn style_list(ui: &mut egui::Ui, width: f32, taken: Option<Style>) -> Option<Style> {
    let mut picked = None;
    egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
        ui.set_width(width - 2.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        ui.add_space(4.0);
        for style in Style::ALL {
            let on = Some(style) == taken;
            let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let row = row.on_hover_cursor(CursorIcon::PointingHand);
            let p = ui.painter();
            if row.hovered() {
                p.rect_filled(rect, 0.0, RULE);
            } else if on {
                p.rect_filled(rect, 0.0, ROW_ON);
            }
            p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, style.name(), mono(12.0), if on { PARCHMENT } else { MUTED });
            if row.clicked() {
                picked = Some(style);
            }
        }
        ui.add_space(4.0);
    });
    picked
}

/// Every band a line, its style tagged at the right and who it is under
/// the pointer, the bands of `style` first where one is chosen; `taken`
/// marked; the one pressed, if any. Scrolls past `list_h`.
fn band_list(ui: &mut egui::Ui, taken: Option<&'static Band>, style: Option<Style>, list_h: f32) -> Option<&'static Band> {
    let mut picked = None;
    let styles = style.into_iter().chain(Style::ALL.into_iter().filter(|s| Some(*s) != style));
    egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
        ui.set_width(BAND_W - 2.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
            ui.add_space(4.0);
            for band in styles.flat_map(band::of_style) {
                let on = Some(band) == taken;
                let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let row = row.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(band.about);
                let p = ui.painter();
                if row.hovered() {
                    p.rect_filled(rect, 0.0, RULE);
                } else if on {
                    p.rect_filled(rect, 0.0, ROW_ON);
                }
                p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, band.name, mono(12.0), if on { PARCHMENT } else { MUTED });
                p.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, band.style.name(), mono(11.0), DOT);
                if row.clicked() {
                    picked = Some(band);
                }
            }
            ui.add_space(4.0);
        });
    });
    picked
}

/// What the setting field shows for `setting`.
fn setting_label(setting: Setting) -> &'static str {
    match setting {
        Setting::None => "none · as it is",
        s => s.name(),
    }
}

/// Whose recordings the music is made with and whose type the window is
/// set in, each with its licence, as the licences ask they be named;
/// scrolling past `list_h`.
fn credits_list(ui: &mut egui::Ui, list_h: f32) {
    egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
        ui.set_width(CREDITS_W - 2.0);
        egui::ScrollArea::vertical().max_height(list_h).show(ui, credits_entries);
    });
}

/// The credits, a heading over each kind.
fn credits_entries(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
    let entry = |ui: &mut egui::Ui, what: &str, work: &str, licence: &str, link: &str| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
            ui.label(RichText::new(what).font(mono(12.0)).color(PARCHMENT));
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Label::new(RichText::new(format!("{work} ·")).font(mono(11.0)).color(MUTED)).wrap());
                ui.hyperlink_to(RichText::new(licence).font(mono(11.0)).color(LAMP), link);
            });
        });
    };
    pool_heading(ui, "made with");
    for (tool, by, link) in music::credits::MADE_WITH {
        entry(ui, tool, by, link.trim_start_matches("https://"), link);
    }
    pool_heading(ui, "sounds");
    for c in music::credits::SOUNDS {
        entry(ui, c.what, c.work, c.licence, c.licence_link);
    }
    pool_heading(ui, "type");
    for (font, by) in TYPE {
        entry(ui, font, by, "SIL OFL 1.1", OFL);
    }
    ui.add_space(6.0);
    rule(ui);
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new("The music is composed here as it plays; these are the recordings and type it is made with.").font(mono(11.0)).color(MUTED).line_height(Some(17.0))).wrap());
    });
}

impl eframe::App for Player {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        // The space bar plays or pauses, unless it is typed into the seed;
        // taken here, no button with the focus takes it as its own.
        if !self.typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
            self.toggle_play();
        }
        ctx.request_repaint_after(Duration::from_millis(33));
        let side = |top: i8, bottom: i8| egui::Margin { left: 20, right: 20, top, bottom };
        let mut midi_anchor = Rect::NOTHING;
        egui::TopBottomPanel::top("now").resizable(false).exact_height(214.0).frame(egui::Frame::new().fill(INK).inner_margin(side(20, 16))).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 32.0;
            ui.columns(2, |halves| {
                halves[0].spacing_mut().item_spacing = vec2(12.0, 6.0);
                self.now_playing(&mut halves[0]);
                halves[1].spacing_mut().item_spacing = vec2(12.0, 0.0);
                midi_anchor = self.controls(&mut halves[1]);
            });
        });
        let credits_anchor = egui::TopBottomPanel::bottom("sheet").frame(egui::Frame::new().fill(INK).inner_margin(side(14, 18))).show(ctx, |ui| self.sheet(ui)).inner;
        let fields = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(INK).inner_margin(side(16, 18)))
            .show(ctx, |ui| {
                let all = ui.max_rect();
                let half = (all.width() - 32.0) / 2.0;
                let left = Rect::from_min_size(all.left_top(), vec2(half, all.height()));
                let right = Rect::from_min_size(pos2(left.right() + 32.0, all.top()), vec2(half, all.height()));
                let fields = ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.selection(ui)).inner;
                ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.up_next(ui));
                fields
            })
            .inner;
        self.popover(ctx, fields, midi_anchor, credits_anchor);
        self.drop_banks(ctx);
    }
}

