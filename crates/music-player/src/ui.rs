//! The window's panels: what plays and its transport at the top, the
//! play-a-seed panel and the queue beside it, the sheet at the foot, and
//! the popovers, the credits among them.

use std::time::{Duration, Instant};

use eframe::egui::text::LayoutJob;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Layout, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind};
use music::pieces::PIECES;
use music::render::SAMPLE_RATE;
use music::SEEDS;

use crate::banks::Install;
use music::banks::folder;
use crate::midi::{self, Port};
use crate::player::{pools, Player, Popover, Take};
use crate::sheet::LANES;
use crate::theme::*;

/// A popover's width, the most its list grows before it scrolls, and
/// what it takes besides the list.
const PANEL_W: f32 = 360.0;
const LIST_H: f32 = 320.0;
const PANEL_CHROME_H: f32 = 52.0 + 34.0 + 4.0;

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

/// The play-a-seed and queue panels' footer: the composer's two rows.
const COMPOSER_H: f32 = 10.0 + 32.0 + 8.0 + 16.0;

impl Player {
    /// The composer's button: all the pieces, or the first chosen, how
    /// many more, and how many of all; it opens the pieces popover above
    /// it.
    pub fn pieces_button(&mut self, ui: &mut egui::Ui) -> Response {
        let names: Vec<&str> = (0..PIECES.len()).filter(|i| self.chosen[*i]).map(|i| PIECES[i].name).collect();
        let label = match names.len() {
            0 => "No pieces".to_string(),
            n if n == PIECES.len() => "All pieces".to_string(),
            1 => names[0].to_string(),
            n => format!("{} +{}", names[0], n - 1),
        };
        let p = ui.painter().clone();
        let label = p.layout_no_wrap(label, mono(12.0), PARCHMENT);
        let count = p.layout_no_wrap(format!("{} of {}", names.len(), PIECES.len()), mono(12.0), MUTED);
        let width = 14.0 + 14.0 + 10.0 + label.size().x + 10.0 + count.size().x + 10.0 + 12.0 + 12.0;
        let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let edge = if self.popover == Some(Popover::Pieces) {
            LAMP
        } else if response.hovered() {
            MUTED
        } else {
            EDGE
        };
        p.rect_filled(rect, 6.0, PANEL);
        p.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
        let cy = rect.center().y;
        let mut x = rect.left() + 14.0;
        let lines = Stroke::new(1.6_f32, LAMP);
        for (dy, len) in [(-3.5, 10.0), (0.0, 10.0), (3.5, 6.0)] {
            p.line_segment([pos2(x + 2.0, cy + dy), pos2(x + 2.0 + len, cy + dy)], lines);
        }
        x += 24.0;
        let (lw, cw) = (label.size().x, count.size().x);
        p.galley(pos2(x, cy - label.size().y / 2.0), label, PARCHMENT);
        x += lw + 10.0;
        p.galley(pos2(x, cy - count.size().y / 2.0), count, MUTED);
        x += cw + 10.0;
        chevron(&p, pos2(x + 6.0, cy), true, MUTED);
        response
    }

    /// The left half of the top: the piece by name, its seed and where it
    /// came from, what it drew, and the summary of its score.
    pub fn now_playing(&self, ui: &mut egui::Ui) {
        let v = self.current();
        let p = ui.painter().clone();
        let name = p.layout_no_wrap(title(PIECES[v.piece].name), FontId::new(30.0, FontFamily::Name(DISPLAY.into())), PARCHMENT);
        let seed = p.layout_no_wrap(format!("seed {}", v.seed), mono(11.0), MUTED);
        let source = p.layout_job(caps(v.source.label(), 10.0));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), name.size().y), Sense::hover());
        let name_w = name.size().x;
        let seed_w = seed.size().x;
        // The display face sits high in its line: the seed's foot is lifted
        // to the name's baseline, about a fifth of its line over the bottom.
        let foot = rect.bottom() - name.size().y * 0.2;
        p.galley(rect.left_top(), name, PARCHMENT);
        p.galley(pos2(rect.left() + name_w + 12.0, foot - seed.size().y), seed, MUTED);
        let chip = Rect::from_min_size(pos2(rect.left() + name_w + 12.0 + seed_w + 12.0, foot - 16.0), vec2(source.size().x + 10.0, 18.0));
        p.rect_stroke(chip, 3.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
        p.galley(chip.center() - source.size() / 2.0, source, MUTED);
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
                ui.label(job);
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&s.summary).font(mono(12.0)).color(MUTED).line_height(Some(19.0))).wrap());
            }
            (None, None) => {
                ui.label(RichText::new(self.state(v).1).font(mono(12.0)).color(MUTED));
            }
        }
    }

    /// The right half of the top: previous, play or pause, next; the
    /// section playing and the clock; the time bar.
    pub fn controls(&mut self, ui: &mut egui::Ui) -> Rect {
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(width, 60.0), Sense::hover());
        let c = row.center();
        let side = |dir: f32| Rect::from_center_size(pos2(c.x + dir * (30.0 + 20.0 + 22.0), c.y), vec2(44.0, 44.0));
        let far = |dir: f32| Rect::from_center_size(pos2(c.x + dir * (30.0 + 20.0 + 44.0 + 20.0 + 22.0), c.y), vec2(44.0, 44.0));
        let repeat = if self.repeat { "Repeat: on. This variation plays again after its rest." } else { "Repeat: off" };
        if round_button(ui, far(-1.0), "repeat", Glyph::Repeat, self.repeat).on_hover_text(repeat).clicked() {
            self.repeat = !self.repeat;
        }
        let midi_on = self.midi.link.lock().unwrap().port != Port::Off;
        let midi = far(1.0);
        if round_button(ui, midi, "midi", Glyph::Midi, midi_on).on_hover_text("MIDI out").clicked() {
            if self.popover == Some(Popover::Midi) {
                self.popover = None;
            } else {
                self.ports = midi::ports();
                self.popover = Some(Popover::Midi);
            }
        }
        if round_button(ui, side(-1.0), "previous", Glyph::Previous, false).clicked() {
            self.previous();
        }
        let toggle = if self.playing { Glyph::Pause } else { Glyph::Play };
        if round_button(ui, Rect::from_center_size(c, vec2(60.0, 60.0)), "play", toggle, true).clicked() {
            self.playing = !self.playing;
            self.rest_until = None;
        }
        if round_button(ui, side(1.0), "next", Glyph::Next, false).clicked() {
            self.next();
        }
        ui.add_space(16.0);

        let take = self.current().take.clone();
        let total = take.as_ref().map_or(0.0, |t| t.audio.len() as f64 / SAMPLE_RATE as f64);
        let at = self.position().min(total);
        let (line, _) = ui.allocate_exact_size(vec2(width, 14.0), Sense::hover());
        let (where_, ink) = match (&take, self.rest_until) {
            (_, Some(until)) => (format!("rest {:.0} s", until.saturating_duration_since(Instant::now()).as_secs_f32().ceil()), MUTED),
            (Some(t), None) => (t.score.sections.iter().rev().find(|s| t.score.seconds(s.start) <= at).map_or("", |s| s.name).to_string(), PARCHMENT),
            (None, None) => (self.state(self.current()).1.to_string(), MUTED),
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

    /// The play-a-seed panel: a piece and a seed, typed or rolled,
    /// played now or added to the queue.
    /// Returns the track button's rect, where its popover hangs.
    pub fn play_a_seed(&mut self, ui: &mut egui::Ui) -> Rect {
        let top = ui.max_rect();
        let p = ui.painter().clone();
        let heading = p.layout_job(caps("PLAY A SEED", 10.0));
        p.galley(top.left_top(), heading, MUTED);

        // The track and the seed, labelled over their fields.
        let y = top.top() + 22.0;
        let seed_w = 150.0;
        let track_rect = Rect::from_min_size(pos2(top.left(), y + 18.0), vec2(top.width() - seed_w - 10.0, 36.0));
        let seed_rect = Rect::from_min_size(pos2(track_rect.right() + 10.0, y + 18.0), vec2(seed_w - 46.0, 36.0));
        let dice_rect = Rect::from_min_size(pos2(seed_rect.right() + 10.0, y + 18.0), vec2(36.0, 36.0));
        p.text(pos2(track_rect.left(), y), Align2::LEFT_TOP, "Track", mono(11.0), MUTED);
        p.text(pos2(seed_rect.left(), y), Align2::LEFT_TOP, "Seed", mono(11.0), MUTED);

        let open = self.popover == Some(Popover::Track);
        let pick = hit(ui, track_rect, "track");
        if pick.clicked() {
            self.popover = if open { None } else { Some(Popover::Track) };
        }
        p.rect_filled(track_rect, 6.0, PANEL);
        p.rect_stroke(track_rect, 6.0, Stroke::new(1.0_f32, if open { LAMP } else if pick.hovered() { MUTED } else { EDGE }), StrokeKind::Inside);
        p.text(track_rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, PIECES[self.track].name, mono(12.0), PARCHMENT);
        chevron(&p, track_rect.right_center() - vec2(16.0, 0.0), false, MUTED);

        p.rect_filled(seed_rect, 6.0, INK);
        p.rect_stroke(seed_rect, 6.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
        let field = egui::TextEdit::singleline(&mut self.seed).font(mono(12.0)).text_color(PARCHMENT).frame(false).margin(egui::Margin::symmetric(10, 10)).vertical_align(Align::Center);
        ui.put(seed_rect, field);
        self.seed.retain(|c| c.is_ascii_digit());
        self.seed.truncate(9);

        let dice = hit(ui, dice_rect, "dice").on_hover_text("A random seed");
        p.rect_filled(dice_rect, 6.0, if dice.hovered() { RULE } else { PANEL });
        p.rect_stroke(dice_rect, 6.0, Stroke::new(1.0_f32, if dice.hovered() { MUTED } else { EDGE }), StrokeKind::Inside);
        let face = Rect::from_center_size(dice_rect.center(), vec2(14.0, 14.0));
        p.rect_stroke(face, 3.0, Stroke::new(1.4_f32, PARCHMENT), StrokeKind::Inside);
        for d in [vec2(-3.0, -3.0), vec2(0.0, 0.0), vec2(3.0, 3.0)] {
            p.circle_filled(face.center() + d, 1.0, PARCHMENT);
        }
        if dice.clicked() {
            self.seed = self.rng.below(SEEDS as usize).to_string();
        }

        // Play now, and add to the queue, at the panel's foot.
        let foot = top.bottom() - 40.0;
        let half = (top.width() - 10.0) / 2.0;
        let now_rect = Rect::from_min_size(pos2(top.left(), foot), vec2(half, 40.0));
        let add_rect = Rect::from_min_size(pos2(now_rect.right() + 10.0, foot), vec2(half, 40.0));
        let seed = self.typed_seed();
        let now = hit(ui, now_rect, "play now");
        p.rect_filled(now_rect, 6.0, if seed.is_none() { LAMP_LOW } else if now.hovered() { LAMP_HOVER } else { LAMP });
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
        if let Some(seed) = seed {
            if now.clicked() {
                self.play_now(self.track, seed);
            }
            if add.clicked() {
                self.enqueue(self.track, seed);
            }
        }
        track_rect
    }

    /// The queue: what the listener asked for, in order, each moved up or
    /// down or taken out; under it, the composer, who plays only when the
    /// queue is empty. Returns the composer's button's rect, where its
    /// popover hangs.
    pub fn queue_panel(&mut self, ui: &mut egui::Ui) -> Rect {
        let top = ui.max_rect();
        let p = ui.painter().clone();
        let count = match self.queue.len() {
            1 => "1 piece".to_string(),
            n => format!("{n} pieces"),
        };
        let heading = p.layout_job(caps(&format!("QUEUE · {count}"), 10.0));
        p.galley(pos2(top.left(), top.top() + 14.0 - heading.size().y / 2.0), heading, MUTED);
        let clear = ui.put(Rect::from_min_size(pos2(top.right() - 56.0, top.top()), vec2(56.0, 28.0)), |ui: &mut egui::Ui| flat_button(ui, "Clear"));
        if clear.clicked() {
            self.queue.clear();
        }

        let list = Rect::from_min_max(pos2(top.left(), top.top() + 38.0), pos2(top.right(), top.bottom() - COMPOSER_H - 10.0));
        if self.queue.is_empty() {
            dashed(&p, list, EDGE);
            let mid = list.center().y;
            p.text(pos2(list.left() + 16.0, mid - 10.0), Align2::LEFT_CENTER, "The queue is empty.", mono(12.0), PARCHMENT);
            let chosen = self.chosen.iter().filter(|c| **c).count();
            let line = format!("The composer plays next, from its {chosen} of {} pieces.", PIECES.len());
            p.text(pos2(list.left() + 16.0, mid + 10.0), Align2::LEFT_CENTER, line, mono(12.0), MUTED);
        } else {
            let mut moves: Vec<(usize, isize)> = Vec::new();
            let mut drops: Vec<usize> = Vec::new();
            let rows = self.queue.iter().map(|v| (v.piece, v.seed, self.state(v))).collect::<Vec<_>>();
            ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
                egui::ScrollArea::vertical().max_height(list.height()).auto_shrink([false, false]).show(ui, |ui| {
                    for (i, (piece, seed, (dot, state))) in rows.iter().enumerate() {
                        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(row, 6.0, PANEL);
                        let cy = row.center().y;
                        p.text(pos2(row.left() + 12.0, cy), Align2::LEFT_CENTER, (i + 1).to_string(), mono(11.0), MUTED);
                        let mut job = LayoutJob::default();
                        run(&mut job, PIECES[*piece].name, 12.0, PARCHMENT);
                        run(&mut job, &format!(" · seed {seed}"), 12.0, MUTED);
                        let name = p.layout_job(job);
                        p.galley(pos2(row.left() + 34.0, cy - name.size().y / 2.0), name, PARCHMENT);
                        // Only the head is rendered ahead; the rest wait.
                        let (dot, state, ink) = if i == 0 { (*dot, *state, PARCHMENT) } else { (DOT, "waiting", MUTED) };
                        let buttons = row.right() - 6.0 - 3.0 * 28.0;
                        let status = p.layout_no_wrap(state.to_string(), mono(11.0), ink);
                        let sx = buttons - 10.0 - status.size().x;
                        p.circle_filled(pos2(sx - 9.0, cy), 3.5, dot);
                        p.galley(pos2(sx, cy - status.size().y / 2.0), status, ink);
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
                });
            });
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

        // The composer: what it draws from, and what it plays next.
        let foot = Rect::from_min_max(pos2(top.left(), top.bottom() - COMPOSER_H), top.right_bottom());
        p.rect_filled(Rect::from_min_size(foot.left_top(), vec2(foot.width(), 1.0)), 0.0, RULE);
        let row1 = Rect::from_min_size(foot.left_top() + vec2(0.0, 10.0), vec2(foot.width(), 32.0));
        let mut anchor = Rect::NOTHING;
        ui.scope_builder(egui::UiBuilder::new().max_rect(row1).layout(Layout::right_to_left(Align::Center)), |ui| {
            let button = self.pieces_button(ui);
            if button.clicked() {
                self.popover = if self.popover == Some(Popover::Pieces) { None } else { Some(Popover::Pieces) };
            }
            anchor = button.rect;
        });
        // The label in the room the button leaves it, shortened or left
        // out where the window is narrow.
        let room = anchor.left() - row1.left() - 10.0;
        if let Some(label) = ["Then the composer draws from", "Composer draws from"].into_iter().map(|t| p.layout_no_wrap(t.to_string(), mono(11.0), MUTED)).find(|g| g.size().x <= room) {
            p.galley(pos2(row1.left(), row1.center().y - label.size().y / 2.0), label, MUTED);
        }
        let row2_y = row1.bottom() + 8.0 + 8.0;
        let (dot, name, state, ink) = match &self.composed {
            None => (DOT, String::new(), "nothing chosen", MUTED),
            Some(v) => {
                let name = format!("{} · seed {}", PIECES[v.piece].name, v.seed);
                if self.queue.is_empty() {
                    let (dot, state) = self.state(v);
                    (dot, name, state, PARCHMENT)
                } else {
                    (DOT, name, "waits for the queue", MUTED)
                }
            }
        };
        p.circle_filled(pos2(foot.left() + 3.5, row2_y), 3.5, dot);
        let mut job = LayoutJob::default();
        run(&mut job, "Composer next   ", 11.0, MUTED);
        run(&mut job, &name, 11.0, ink);
        run(&mut job, &format!("   {state}"), 11.0, MUTED);
        let line = p.layout_job(job);
        p.galley(pos2(foot.left() + 17.0, row2_y - line.size().y / 2.0), line, MUTED);
        anchor
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
        let take = self.current().take.clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.label(caps("SHEET", 10.0));
            if let Some(t) = &take {
                for (lane, v) in t.sheet.at(self.position()).iter().enumerate().filter_map(|(lane, v)| v.map(|v| (lane, v))) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (swatch, _) = ui.allocate_exact_size(vec2(10.0, 4.0), Sense::hover());
                        ui.painter().rect_filled(swatch, 1.0, VOICE_INKS[lane]);
                        ui.label(RichText::new(t.sheet.voices[v].name).font(mono(11.0)).color(PARCHMENT));
                    });
                }
            }
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
            });
        });
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), SHEET_H), Sense::hover());
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, 6.0, SHEET_INK);
        let Some(take) = take else {
            p.text(rect.center(), Align2::CENTER_CENTER, self.state(self.current()).1, mono(12.0), MUTED);
            p.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, RULE), StrokeKind::Inside);
            return credits;
        };
        let sheet = &take.sheet;
        let at = self.position();
        let x = |t: f64| rect.left() + PLAYHEAD_X + ((t - at) as f32) * SHEET_PX_PER_S;
        let (from, to) = (at - (PLAYHEAD_X / SHEET_PX_PER_S) as f64, at + ((rect.width() - PLAYHEAD_X) / SHEET_PX_PER_S) as f64);
        if sheet.bar_s > 0.0 {
            let mut b = (from.max(0.0) / sheet.bar_s).ceil() * sheet.bar_s;
            while b <= to {
                let bx = x(b);
                p.line_segment([pos2(bx, rect.top()), pos2(bx, rect.bottom())], Stroke::new(1.0_f32, BAR_LINE));
                b += sheet.bar_s;
            }
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

    /// The open popover, if any: the composer's pieces opening up from
    /// its button, the track list hung under its field, MIDI out under
    /// its button, the credits over theirs. A press outside it or Escape
    /// closes it.
    pub fn popover(&mut self, ctx: &egui::Context, pieces_anchor: Rect, track_anchor: Rect, midi_anchor: Rect, credits_anchor: Rect) {
        let Some(open) = self.popover else { return };
        let anchor = match open {
            Popover::Pieces => pieces_anchor,
            Popover::Track => track_anchor,
            Popover::Midi => midi_anchor,
            Popover::Credits => credits_anchor,
        };
        let area = match open {
            Popover::Pieces => {
                let list_h = (anchor.top() - 8.0 - PANEL_CHROME_H - 12.0).clamp(68.0, LIST_H);
                egui::Area::new(egui::Id::new("pieces")).order(egui::Order::Foreground).pivot(Align2::RIGHT_BOTTOM).fixed_pos(pos2(anchor.right(), anchor.top() - 8.0)).show(ctx, |ui| self.pieces_list(ui, list_h))
            }
            Popover::Track => {
                let list_h = (ctx.screen_rect().bottom() - anchor.bottom() - 8.0 - 24.0).clamp(68.0, LIST_H);
                let width = anchor.width();
                egui::Area::new(egui::Id::new("track")).order(egui::Order::Foreground).fixed_pos(pos2(anchor.left(), anchor.bottom() + 6.0)).show(ctx, |ui| self.track_list(ui, width, list_h))
            }
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

    /// The composer's pieces: a filter, all and none, and every piece
    /// under its pool.
    pub fn pieces_list(&mut self, ui: &mut egui::Ui, list_h: f32) {
        let mut flips: Vec<(Vec<usize>, bool)> = Vec::new();
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
            ui.set_width(PANEL_W - 2.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            egui::Frame::new().inner_margin(10.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    egui::Frame::new().fill(INK).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
                        ui.set_width(PANEL_W - 2.0 - 20.0 - 2.0 * (44.0 + 8.0) - 20.0);
                        ui.set_height(32.0);
                        ui.horizontal_centered(|ui| {
                            let (icon, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                            let lens = Stroke::new(1.6_f32, MUTED);
                            ui.painter().circle_stroke(icon.min + vec2(5.5, 5.5), 4.0, lens);
                            ui.painter().line_segment([icon.min + vec2(8.5, 8.5), icon.min + vec2(11.5, 11.5)], lens);
                            let field = egui::TextEdit::singleline(&mut self.filter)
                                .hint_text(RichText::new("Filter pieces or pools").font(mono(12.0)).color(MUTED))
                                .font(mono(12.0))
                                .text_color(PARCHMENT)
                                .frame(false)
                                .desired_width(f32::INFINITY);
                            ui.add(field);
                        });
                    });
                    if flat_button(ui, "All").clicked() {
                        flips.push(((0..PIECES.len()).collect(), true));
                    }
                    if flat_button(ui, "None").clicked() {
                        flips.push(((0..PIECES.len()).collect(), false));
                    }
                });
            });
            rule(ui);
            let query = self.filter.trim().to_lowercase();
            let shown: Vec<usize> = (0..PIECES.len()).filter(|i| query.is_empty() || PIECES[*i].name.contains(&query) || PIECES[*i].pool.contains(&query)).collect();
            egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
                ui.add_space(4.0);
                if shown.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                    ui.painter().text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "No piece matches", mono(12.0), MUTED);
                }
                for (pool, members) in pools() {
                    let members: Vec<usize> = members.into_iter().filter(|i| shown.contains(i)).collect();
                    if members.is_empty() {
                        continue;
                    }
                    pool_heading(ui, pool);
                    for i in members {
                        let on = self.chosen[i];
                        let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        let row = row.on_hover_cursor(CursorIcon::PointingHand);
                        let p = ui.painter();
                        if row.hovered() {
                            p.rect_filled(rect, 0.0, RULE);
                        } else if on {
                            p.rect_filled(rect, 0.0, ROW_ON);
                        }
                        check_box(p, pos2(rect.left() + 14.0, rect.center().y - 8.0), on);
                        p.text(pos2(rect.left() + 40.0, rect.center().y), Align2::LEFT_CENTER, PIECES[i].name, mono(12.0), if on { PARCHMENT } else { MUTED });
                        if row.clicked() {
                            flips.push((vec![i], !on));
                        }
                    }
                }
                ui.add_space(8.0);
            });
            rule(ui);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                ui.label(RichText::new("When the queue is empty, the composer plays a fresh seed of one of these.").font(mono(11.0)).color(MUTED));
            });
        });
        for (pieces, on) in flips {
            self.choose(&pieces, on);
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

    /// Every piece under its pool, one to pick for the play-a-seed panel.
    pub fn track_list(&mut self, ui: &mut egui::Ui, width: f32, list_h: f32) {
        let mut picked = None;
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(8.0).show(ui, |ui| {
            ui.set_width(width - 2.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
                ui.add_space(4.0);
                for (pool, members) in pools() {
                    pool_heading(ui, pool);
                    for i in members {
                        let on = i == self.track;
                        let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        let row = row.on_hover_cursor(CursorIcon::PointingHand);
                        let p = ui.painter();
                        if row.hovered() {
                            p.rect_filled(rect, 0.0, RULE);
                        } else if on {
                            p.rect_filled(rect, 0.0, ROW_ON);
                        }
                        p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, PIECES[i].name, mono(12.0), if on { PARCHMENT } else { MUTED });
                        if row.clicked() {
                            picked = Some(i);
                        }
                    }
                }
                ui.add_space(8.0);
            });
        });
        if let Some(i) = picked {
            self.track = i;
            self.popover = None;
        }
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
        ctx.request_repaint_after(Duration::from_millis(33));
        let side = |top: i8, bottom: i8| egui::Margin { left: 20, right: 20, top, bottom };
        let mut midi_anchor = Rect::NOTHING;
        egui::TopBottomPanel::top("now").resizable(false).min_height(176.0).frame(egui::Frame::new().fill(INK).inner_margin(side(20, 16))).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 32.0;
            ui.columns(2, |halves| {
                halves[0].spacing_mut().item_spacing = vec2(12.0, 6.0);
                self.now_playing(&mut halves[0]);
                halves[1].spacing_mut().item_spacing = vec2(12.0, 0.0);
                midi_anchor = self.controls(&mut halves[1]);
            });
        });
        let credits_anchor = egui::TopBottomPanel::bottom("sheet").frame(egui::Frame::new().fill(INK).inner_margin(side(14, 18))).show(ctx, |ui| self.sheet(ui)).inner;
        let (track_anchor, pieces_anchor) = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(INK).inner_margin(side(16, 18)))
            .show(ctx, |ui| {
                let all = ui.max_rect();
                let half = (all.width() - 32.0) / 2.0;
                let left = Rect::from_min_size(all.left_top(), vec2(half, all.height()));
                let right = Rect::from_min_size(pos2(left.right() + 32.0, all.top()), vec2(half, all.height()));
                let track = ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.play_a_seed(ui)).inner;
                let pieces = ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.queue_panel(ui)).inner;
                (track, pieces)
            })
            .inner;
        self.popover(ctx, pieces_anchor, track_anchor, midi_anchor, credits_anchor);
        self.drop_banks(ctx);
    }
}

