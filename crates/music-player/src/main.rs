//! A player for the pieces' variations: draws a seed of a chosen piece,
//! composes and renders it as the shipped files are (`render::take`), plays it
//! once through, rests, and draws the next. Nothing is read from
//! `music/` — the pool's files are three seeds of the countless a piece
//! has. A variation is rendered whole into memory before it plays, so
//! the time bar can seek anywhere; the next is rendered while the
//! current plays.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Layout, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind};
use music::pieces::PIECES;
use music::render::{self, Bank, SAMPLE_RATE};
use music::rng::Rng;
use music::score::Score;

/// The silence between one variation's end and the next one's start.
const REST: Duration = Duration::from_secs(4);

/// Seeds are drawn under this so they can be read and noted; a piece
/// keys nothing to a seed's value, so the range costs no variety.
const SEEDS: usize = 1_000_000;

/// Back past this far into a variation, "previous" starts it again
/// instead of going back one.
const RESTART_S: f64 = 3.0;

const INK: Color32 = Color32::from_rgb(0x15, 0x17, 0x1B);
const PANEL: Color32 = Color32::from_rgb(0x1E, 0x21, 0x28);
const RULE: Color32 = Color32::from_rgb(0x2A, 0x2E, 0x37);
const EDGE: Color32 = Color32::from_rgb(0x3A, 0x3F, 0x4A);
const ROW_ON: Color32 = Color32::from_rgb(0x25, 0x29, 0x32);
const PARCHMENT: Color32 = Color32::from_rgb(0xED, 0xE6, 0xD6);
const MUTED: Color32 = Color32::from_rgb(0xA3, 0x9C, 0x8C);
const DOT: Color32 = Color32::from_rgb(0x5C, 0x5A, 0x55);
const LAMP: Color32 = Color32::from_rgb(0xD9, 0xA4, 0x41);
const LAMP_HOVER: Color32 = Color32::from_rgb(0xEB, 0xC0, 0x77);
const LAMP_LOW: Color32 = Color32::from_rgb(0x8A, 0x74, 0x48);
const READY: Color32 = Color32::from_rgb(0x7F, 0xA6, 0x7A);
const ALERT: Color32 = Color32::from_rgb(0xE0, 0x7A, 0x5F);

/// The display face, for the piece's name.
const DISPLAY: &str = "display";

/// The pieces panel's width, the most its list grows before it scrolls,
/// and what the panel takes besides the list: the filter row, the
/// footnote and the rules between.
const PANEL_W: f32 = 360.0;
const LIST_H: f32 = 320.0;
const PANEL_CHROME_H: f32 = 52.0 + 34.0 + 4.0;

/// A variation: a piece and a seed, its render once it arrives.
struct Variation {
    piece: usize,
    seed: u64,
    take: Option<Arc<Take>>,
    failed: Option<String>,
}

impl Variation {
    fn job(&self) -> Job {
        (self.piece, self.seed)
    }
}

struct Take {
    score: Score,
    audio: Arc<Vec<[f32; 2]>>,
}

type Job = (usize, u64);

// ── Deck: what the output stream plays ──

/// The variation under the playhead, shared with the audio callback.
#[derive(Default)]
struct Deck {
    audio: Option<Arc<Vec<[f32; 2]>>>,
    /// The playhead, in frames of the render at `SAMPLE_RATE`.
    at: f64,
    playing: bool,
}

impl Deck {
    fn ended(&self) -> bool {
        self.audio.as_ref().is_some_and(|a| self.at >= a.len() as f64)
    }

    /// The next output frame, `step` render frames on, linearly
    /// interpolated where the device runs at another rate.
    fn frame(&mut self, step: f64) -> [f32; 2] {
        let Some(audio) = &self.audio else { return [0.0; 2] };
        if !self.playing || self.at >= audio.len() as f64 {
            return [0.0; 2];
        }
        let i = self.at as usize;
        let f = (self.at - i as f64) as f32;
        let (a, b) = (audio[i], audio[(i + 1).min(audio.len() - 1)]);
        self.at += step;
        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
    }
}

fn open_output(deck: Arc<Mutex<Deck>>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: cpal::StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => output::<f32>(&device, &config, deck),
        cpal::SampleFormat::I16 => output::<i16>(&device, &config, deck),
        cpal::SampleFormat::U16 => output::<u16>(&device, &config, deck),
        cpal::SampleFormat::I32 => output::<i32>(&device, &config, deck),
        f => return Err(format!("unsupported output sample format {f:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn output<T: SizedSample + FromSample<f32>>(device: &cpal::Device, config: &cpal::StreamConfig, deck: Arc<Mutex<Deck>>) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let step = SAMPLE_RATE as f64 / config.sample_rate as f64;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut deck = deck.lock().unwrap();
                for out in data.chunks_mut(channels) {
                    let [l, r] = deck.frame(step);
                    for (c, s) in out.iter_mut().enumerate() {
                        let v = match (channels, c) {
                            (1, _) => (l + r) * 0.5,
                            (_, 0) => l,
                            (_, 1) => r,
                            _ => 0.0,
                        };
                        *s = T::from_sample(v);
                    }
                }
            },
            |e| eprintln!("music-player: output: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

// ── Worker: renders one variation at a time ──

/// The one variation the player needs rendered next; the worker takes
/// it when it is free, so a request made while it renders replaces any
/// still waiting.
#[derive(Default)]
struct Wanted {
    job: Mutex<Option<Job>>,
    posted: Condvar,
}

enum Done {
    Bank(Result<(), String>),
    Take(Job, Result<Take, String>),
}

fn spawn_worker(wanted: Arc<Wanted>, busy: Arc<Mutex<Option<Job>>>) -> Receiver<Done> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let bank = match Bank::find(None).ok_or_else(|| "no SoundFont found; set SOUNDFONT".to_string()).and_then(|p| Bank::load(&p)) {
            Ok(b) => {
                let _ = tx.send(Done::Bank(Ok(())));
                b
            }
            Err(e) => {
                let _ = tx.send(Done::Bank(Err(e)));
                return;
            }
        };
        loop {
            let job = {
                let mut slot = wanted.job.lock().unwrap();
                while slot.is_none() {
                    slot = wanted.posted.wait(slot).unwrap();
                }
                let job = slot.take().unwrap();
                *busy.lock().unwrap() = Some(job);
                job
            };
            let (piece, seed) = job;
            let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&PIECES[piece], seed, &bank)))
                .map(|(score, audio)| Take { score, audio: Arc::new(audio) })
                .map_err(|_| format!("{} seed {seed} panicked as it composed", PIECES[piece].name));
            *busy.lock().unwrap() = None;
            if tx.send(Done::Take(job, taken)).is_err() {
                return;
            }
        }
    });
    rx
}

// ── App ──

struct Player {
    chosen: Vec<bool>,
    history: Vec<Variation>,
    at: usize,
    /// The next fresh draw; none while no piece is chosen.
    upcoming: Option<Variation>,
    /// Whether the pieces panel is open, and its filter.
    choosing: bool,
    filter: String,
    /// Whether the listener wants sound: the deck plays when this is set
    /// and the current variation has arrived.
    playing: bool,
    /// When the rest after the current variation ends.
    rest_until: Option<Instant>,
    deck: Arc<Mutex<Deck>>,
    _stream: Option<cpal::Stream>,
    wanted: Arc<Wanted>,
    busy: Arc<Mutex<Option<Job>>>,
    done: Receiver<Done>,
    bank: Option<Result<(), String>>,
    output_error: Option<String>,
    rng: Rng,
}

impl Player {
    fn new() -> Self {
        let deck = Arc::new(Mutex::new(Deck::default()));
        let (stream, output_error) = match open_output(deck.clone()) {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e)),
        };
        let wanted = Arc::new(Wanted::default());
        let busy = Arc::new(Mutex::new(None));
        let done = spawn_worker(wanted.clone(), busy.clone());
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        let mut rng = Rng::new(nanos);
        let chosen = vec![true; PIECES.len()];
        let first = draw(&mut rng, &chosen).expect("a piece is chosen");
        let upcoming = draw(&mut rng, &chosen);
        Player { chosen, history: vec![first], at: 0, upcoming, choosing: false, filter: String::new(), playing: true, rest_until: None, deck, _stream: stream, wanted, busy, done, bank: None, output_error, rng }
    }

    fn current(&self) -> &Variation {
        &self.history[self.at]
    }

    /// Takes in what the worker finished, puts the current variation on
    /// the deck once it has arrived, and asks for the one needed next.
    fn poll(&mut self) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Bank(b) => self.bank = Some(b),
                Done::Take(job, taken) => {
                    let taken = taken.map(Arc::new);
                    let near = self.at.saturating_sub(1)..=self.at + 1;
                    let in_history = self.history.iter_mut().enumerate().filter(|(i, _)| near.contains(i)).map(|(_, v)| v);
                    for v in in_history.chain(self.upcoming.as_mut()).filter(|v| v.job() == job) {
                        match &taken {
                            Ok(t) => v.take = Some(t.clone()),
                            Err(e) => v.failed = Some(e.clone()),
                        }
                    }
                }
            }
        }

        let current = self.current();
        let mut deck = self.deck.lock().unwrap();
        if let Some(t) = &current.take {
            if !deck.audio.as_ref().is_some_and(|a| Arc::ptr_eq(a, &t.audio)) {
                deck.audio = Some(t.audio.clone());
                deck.at = 0.0;
            }
        }
        deck.playing = self.playing;
        let ended = deck.ended();
        drop(deck);

        if self.current().failed.is_some() && self.playing {
            self.next();
        } else if ended && self.playing {
            let until = *self.rest_until.get_or_insert_with(|| Instant::now() + REST);
            if Instant::now() >= until {
                self.next();
            }
        }

        let need = std::iter::once(self.current()).chain(self.following()).find(|v| v.take.is_none() && v.failed.is_none()).map(Variation::job);
        if let Some(job) = need {
            if *self.busy.lock().unwrap() != Some(job) {
                let mut slot = self.wanted.job.lock().unwrap();
                if *slot != Some(job) {
                    *slot = Some(job);
                    self.wanted.posted.notify_one();
                }
            }
        }
    }

    /// What plays after the current variation: the one after it in the
    /// history where the listener went back, else the upcoming draw.
    fn following(&self) -> Option<&Variation> {
        self.history.get(self.at + 1).or(self.upcoming.as_ref())
    }

    /// Moves on to what follows; with nothing chosen and no history ahead,
    /// the current variation stays.
    fn next(&mut self) {
        if self.at + 1 == self.history.len() {
            let fresh = draw(&mut self.rng, &self.chosen);
            match std::mem::replace(&mut self.upcoming, fresh) {
                Some(up) => self.history.push(up),
                None => return,
            }
        }
        self.go(self.at + 1);
    }

    fn previous(&mut self) {
        if self.at == 0 || self.position() > RESTART_S {
            self.seek(0.0);
        } else {
            self.go(self.at - 1);
        }
    }

    /// Makes history entry `i` current, and lets go of every render but
    /// its neighbours': a variation runs to tens of megabytes, and one
    /// gone back to is rendered again.
    fn go(&mut self, i: usize) {
        self.at = i;
        self.rest_until = None;
        for (k, v) in self.history.iter_mut().enumerate() {
            if k + 1 < i || k > i + 1 {
                v.take = None;
            }
        }
        let mut deck = self.deck.lock().unwrap();
        deck.audio = None;
        deck.at = 0.0;
    }

    fn seek(&mut self, seconds: f64) {
        self.rest_until = None;
        self.deck.lock().unwrap().at = (seconds * SAMPLE_RATE as f64).max(0.0);
    }

    fn position(&self) -> f64 {
        self.deck.lock().unwrap().at / SAMPLE_RATE as f64
    }

    /// A change of the chosen pieces redraws the upcoming variation where
    /// its piece is no longer among them, or draws one where none was.
    fn choose(&mut self, pieces: &[usize], on: bool) {
        for i in pieces {
            self.chosen[*i] = on;
        }
        if !self.upcoming.as_ref().is_some_and(|v| self.chosen[v.piece]) {
            self.upcoming = draw(&mut self.rng, &self.chosen);
        }
    }
}

/// A seed of one of the chosen pieces, where any is.
fn draw(rng: &mut Rng, chosen: &[bool]) -> Option<Variation> {
    let pieces: Vec<usize> = (0..PIECES.len()).filter(|i| chosen[*i]).collect();
    if pieces.is_empty() {
        return None;
    }
    Some(Variation { piece: *rng.pick(&pieces), seed: rng.below(SEEDS) as u64, take: None, failed: None })
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A piece's name as a title: `overworld-ambient` reads Overworld Ambient.
fn title(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn run(job: &mut LayoutJob, text: &str, size: f32, color: Color32) {
    job.append(text, 0.0, TextFormat { font_id: mono(size), color, ..Default::default() });
}

/// Spaced capitals, the window's small headings.
fn caps(text: &str, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, TextFormat { font_id: mono(size), color: MUTED, extra_letter_spacing: size * 0.17, ..Default::default() });
    job
}

#[derive(Clone, Copy)]
enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
}

fn paint_glyph(p: &egui::Painter, c: Pos2, glyph: Glyph, color: Color32) {
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
    }
}

/// A round transport button: the lamp-lit one is the main action.
fn round_button(ui: &mut egui::Ui, rect: Rect, id: &str, glyph: Glyph, lit: bool) -> Response {
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

fn check_box(p: &egui::Painter, at: Pos2, on: bool) {
    let rect = Rect::from_min_size(at, vec2(16.0, 16.0));
    if on {
        p.rect_filled(rect, 3.0, LAMP);
        let mark = vec![at + vec2(4.0, 8.2), at + vec2(6.8, 11.0), at + vec2(12.0, 5.2)];
        p.add(Shape::line(mark, Stroke::new(1.8_f32, INK)));
    } else {
        p.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, EDGE), StrokeKind::Inside);
    }
}

fn flat_button(ui: &mut egui::Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).font(mono(11.0)).color(PARCHMENT)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0_f32, EDGE)).corner_radius(6.0).min_size(vec2(44.0, 32.0)))
        .on_hover_cursor(CursorIcon::PointingHand)
}

impl Player {
    /// The pieces button: the first chosen piece, how many more, and how
    /// many of all; it opens the pieces panel.
    fn pieces_button(&mut self, ui: &mut egui::Ui) -> Response {
        let names: Vec<&str> = (0..PIECES.len()).filter(|i| self.chosen[*i]).map(|i| PIECES[i].name).collect();
        let label = match names.len() {
            0 => "No pieces".to_string(),
            1 => names[0].to_string(),
            n => format!("{} +{}", names[0], n - 1),
        };
        let p = ui.painter().clone();
        let label = p.layout_no_wrap(label, mono(12.0), PARCHMENT);
        let count = p.layout_no_wrap(format!("{} of {}", names.len(), PIECES.len()), mono(12.0), MUTED);
        let width = 14.0 + 14.0 + 10.0 + label.size().x + 10.0 + count.size().x + 10.0 + 12.0 + 12.0;
        let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let edge = if self.choosing {
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
        p.add(Shape::line(vec![pos2(x + 3.0, cy - 1.5), pos2(x + 6.0, cy + 1.5), pos2(x + 9.0, cy - 1.5)], Stroke::new(1.6_f32, MUTED)));
        response
    }

    fn header(&mut self, ui: &mut egui::Ui) -> Rect {
        let mut anchor = Rect::NOTHING;
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(caps("MUSIC PLAYER", 11.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let button = self.pieces_button(ui);
                if button.clicked() {
                    self.choosing = !self.choosing;
                }
                anchor = button.rect;
            });
        });
        anchor
    }

    /// The left half: the piece by name, its seed, what it drew, and the
    /// summary of its score.
    fn now_playing(&self, ui: &mut egui::Ui) {
        let v = self.current();
        let p = ui.painter().clone();
        let name = p.layout_no_wrap(title(PIECES[v.piece].name), FontId::new(30.0, FontFamily::Name(DISPLAY.into())), PARCHMENT);
        let seed = p.layout_no_wrap(format!("seed {}", v.seed), mono(11.0), MUTED);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), name.size().y), Sense::hover());
        let name_w = name.size().x;
        // The display face sits high in its line: the seed's foot is lifted
        // to the name's baseline, about a fifth of its line over the bottom.
        let foot = rect.bottom() - name.size().y * 0.2;
        p.galley(rect.left_top(), name, PARCHMENT);
        p.galley(pos2(rect.left() + name_w + 12.0, foot - seed.size().y), seed, MUTED);
        ui.add_space(4.0);
        match (&v.take, &v.failed) {
            (_, Some(e)) => {
                ui.label(RichText::new(e).font(mono(12.0)).color(ALERT));
            }
            (Some(t), _) => {
                let s = &t.score;
                let facts = [s.sections[0].name.to_string(), s.key.name(), s.meter.label(), format!("{:.0} bpm", s.eighth_bpm / 2.0), (if s.loops { "loop" } else { "one-shot" }).to_string()];
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
                ui.label(RichText::new("composing…").font(mono(12.0)).color(MUTED));
            }
        }
    }

    /// The right half: previous, play or pause, next; the section playing
    /// and the clock; the time bar.
    fn controls(&mut self, ui: &mut egui::Ui) {
        let block = 60.0 + 16.0 + 14.0 + 6.0 + 14.0;
        ui.add_space(((ui.available_height() - block) / 2.0).max(0.0));
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(width, 60.0), Sense::hover());
        let c = row.center();
        let side = |dir: f32| Rect::from_center_size(pos2(c.x + dir * (30.0 + 20.0 + 22.0), c.y), vec2(44.0, 44.0));
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
            (None, None) => ("composing…".to_string(), MUTED),
        };
        let p = ui.painter();
        p.text(line.left_center(), Align2::LEFT_CENTER, where_, mono(11.0), ink);
        p.text(line.right_center(), Align2::RIGHT_CENTER, format!("{} / {}", clock(at), clock(total)), mono(11.0), MUTED);
        ui.add_space(6.0);
        self.timeline(ui, take.as_deref(), total, at);
    }

    /// The time bar: one segment a section, the ones played lit, the one
    /// playing half lit, a knob at the playhead; a press or a drag anywhere
    /// on it moves the playhead there.
    fn timeline(&mut self, ui: &mut egui::Ui, take: Option<&Take>, total: f64, at: f64) {
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
            // A one-shot rings on past its last section; the ring is the
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

    fn footer(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let small = |text: &str, color: Color32| RichText::new(text).font(mono(11.0)).color(color);
            let up = self.following();
            let (dot, state) = match up {
                None => (DOT, "nothing chosen"),
                Some(v) if v.failed.is_some() => (ALERT, "failed"),
                Some(v) if v.take.is_some() => (READY, "ready"),
                Some(_) => (LAMP, "composing…"),
            };
            let (spot, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
            ui.painter().circle_filled(spot.center(), 3.5, dot);
            ui.label(small("Up next", MUTED));
            if let Some(v) = up {
                ui.label(small(&format!("{} · seed {}", PIECES[v.piece].name, v.seed), PARCHMENT));
            }
            ui.label(small(state, MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(e) = &self.output_error {
                    ui.label(small(&format!("audio output: {e}"), ALERT));
                }
                if let Some(Err(e)) = &self.bank {
                    ui.label(small(&format!("SoundFont: {e}"), ALERT));
                }
            });
        });
    }

    /// The pieces panel, hung under its button: a filter, all and none, and
    /// every piece under its pool. A press outside it or Escape closes it.
    fn pieces_panel(&mut self, ctx: &egui::Context, anchor: Rect) {
        if !self.choosing {
            return;
        }
        let mut flips: Vec<(Vec<usize>, bool)> = Vec::new();
        let at = pos2(anchor.right() - PANEL_W, anchor.bottom() + 8.0);
        let list_h = (ctx.screen_rect().bottom() - at.y - PANEL_CHROME_H - 12.0).clamp(68.0, LIST_H);
        let area = egui::Area::new(egui::Id::new("pieces")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
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
                let mut pools: Vec<&str> = Vec::new();
                for i in &shown {
                    if !pools.contains(&PIECES[*i].pool) {
                        pools.push(PIECES[*i].pool);
                    }
                }
                egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
                    ui.add_space(4.0);
                    if shown.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                        ui.painter().text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "No piece matches", mono(12.0), MUTED);
                    }
                    for pool in &pools {
                        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                        let heading = ui.painter().layout_job(caps(&pool.to_uppercase(), 10.0));
                        ui.painter().galley(pos2(rect.left() + 14.0, rect.bottom() - 4.0 - heading.size().y), heading, MUTED);
                        for i in shown.iter().copied().filter(|i| PIECES[*i].pool == *pool) {
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
                    ui.label(RichText::new("Each variation: a fresh seed of a chosen piece.").font(mono(11.0)).color(MUTED));
                });
            });
        });
        for (pieces, on) in flips {
            self.choose(&pieces, on);
        }
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !anchor.contains(p)));
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.choosing = false;
        }
    }
}

/// A full-width hairline between the panel's parts.
fn rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, RULE);
}

impl eframe::App for Player {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(50));
        let side = |top: i8, bottom: i8| egui::Margin { left: 20, right: 20, top, bottom };
        let anchor = egui::TopBottomPanel::top("header").frame(egui::Frame::new().fill(INK).inner_margin(side(10, 10))).show(ctx, |ui| self.header(ui)).inner;
        egui::TopBottomPanel::bottom("footer").frame(egui::Frame::new().fill(INK).inner_margin(side(10, 10))).show(ctx, |ui| self.footer(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(INK).inner_margin(side(18, 14))).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 32.0;
            ui.columns(2, |halves| {
                halves[0].spacing_mut().item_spacing = vec2(12.0, 6.0);
                self.now_playing(&mut halves[0]);
                halves[1].spacing_mut().item_spacing = vec2(12.0, 0.0);
                self.controls(&mut halves[1]);
            });
        });
        self.pieces_panel(ctx, anchor);
    }
}

/// Egui's dark theme in the window's colours: hairlines in the rule's
/// colour, the text cursor and selection lamp-lit.
fn visuals() -> egui::Visuals {
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

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Plex".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../fonts/IBMPlexMono-Regular.ttf"))));
    fonts.font_data.insert("Cormorant".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../fonts/CormorantGaramond-SemiBold.ttf"))));
    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        fonts.families.entry(family).or_default().insert(0, "Plex".to_owned());
    }
    fonts.families.insert(FontFamily::Name(DISPLAY.into()), vec!["Cormorant".to_owned(), "Plex".to_owned()]);
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([880.0, 340.0]).with_min_inner_size([720.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Music Player",
        options,
        Box::new(|cc| {
            // Egui follows the system's light or dark theme, each with its own
            // visuals; the window's colours are one dark set.
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx.set_visuals(visuals());
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(Player::new()))
        }),
    )
}
