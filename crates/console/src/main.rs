//! The monitoring console: a page for each process publishing metrics to
//! its kind's multicast group (`common::metrics`), switched with the
//! number keys, Tab, or a click on the strip at the top. Processes of one
//! kind share a group, and each is told apart by the address it sends
//! from: every client running gets a page of its own, numbered in the
//! order first heard, and loses it once silent for `FORGET`. Every source
//! is heard whichever page shows, so a page switched to has its history.

mod client;
mod feed;
mod server;
mod widgets;

use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::mpsc,
    time::Duration,
};

use eframe::egui;

use common::metrics::{self, MetricsPacket};
use feed::Feed;
use widgets::*;

/// What kind of process a page shows, in the strip's order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Server,
    Client,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::Server, Kind::Client];

    fn label(self) -> &'static str {
        match self {
            Kind::Server => "SERVER",
            Kind::Client => "CLIENT",
        }
    }

    fn group(self) -> SocketAddrV4 {
        match self {
            Kind::Server => metrics::SERVER_GROUP,
            Kind::Client => metrics::CLIENT_GROUP,
        }
    }

    /// The topic this kind publishes every time, whose clock the status
    /// line reads.
    fn clock_topic(self) -> &'static str {
        match self {
            Kind::Server => "server",
            Kind::Client => "frame",
        }
    }

    fn draw(self, ui: &mut egui::Ui, feed: &Feed, cw: f32, rh: f32) {
        match self {
            Kind::Server => server::draw(ui, feed, cw, rh),
            Kind::Client => client::draw(ui, feed, cw, rh),
        }
    }
}

/// How long a process may go unheard before its page goes.
const FORGET: Duration = Duration::from_secs(30);

/// The console's size: three columns of the client page, whose timings
/// column is the tallest of any page.
const WINDOW: [f32; 2] = [1515.0, 680.0];

fn main() -> eframe::Result {
    let (tx, rx) = mpsc::channel::<(Kind, SocketAddr, MetricsPacket)>();
    for kind in Kind::ALL {
        let tx = tx.clone();
        std::thread::spawn(move || listen(kind, tx));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(WINDOW).with_resizable(false).with_maximize_button(false),
        ..Default::default()
    };
    eframe::run_native(
        "Console",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(Console { rx, sources: Vec::new(), shown: Shown::Kind(Kind::Server), char_width: None, row_height: None }))
        }),
    )
}

/// Hears `kind`'s group for as long as the console runs, each packet
/// passed on with the address it came from.
fn listen(kind: Kind, tx: mpsc::Sender<(Kind, SocketAddr, MetricsPacket)>) {
    let group = kind.group();
    let socket = match metrics::subscriber(group) {
        Ok(socket) => socket,
        Err(e) => {
            eprintln!("cannot join {group}: {e}");
            return;
        }
    };
    socket.set_read_timeout(Some(Duration::from_secs(1))).ok();
    let mut buf = [0u8; 65536];
    loop {
        let Ok((n, sender)) = socket.recv_from(&mut buf) else { continue };
        let Some(packet) = MetricsPacket::decode(&buf[..n]) else { continue };
        if tx.send((kind, sender, packet)).is_err() {
            return;
        }
    }
}

/// One process heard: its kind, the address it sends from, its number
/// among its kind (1 for the first), and what it published.
struct Source {
    kind: Kind,
    sender: SocketAddr,
    number: usize,
    feed: Feed,
}

impl Source {
    fn label(&self) -> String {
        if self.number == 1 { self.kind.label().to_string() } else { format!("{} {}", self.kind.label(), self.number) }
    }
}

/// The page up: a source, or a kind none of whose processes is heard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Source(SocketAddr),
    Kind(Kind),
}

struct Console {
    rx: mpsc::Receiver<(Kind, SocketAddr, MetricsPacket)>,
    sources: Vec<Source>,
    shown: Shown,
    char_width: Option<f32>,
    row_height: Option<f32>,
}

impl Console {
    fn hear(&mut self, kind: Kind, sender: SocketAddr, packet: MetricsPacket) {
        let at = match self.sources.iter().position(|s| s.kind == kind && s.sender == sender) {
            Some(at) => at,
            None => {
                // The lowest number none of its kind holds now
                let number = (1..).find(|n| !self.sources.iter().any(|s| s.kind == kind && s.number == *n)).unwrap();
                self.sources.push(Source { kind, sender, number, feed: Feed::default() });
                self.sources.len() - 1
            }
        };
        self.sources[at].feed.hear(packet);
    }

    fn forget_silent(&mut self) {
        self.sources.retain(|s| s.feed.heard().is_some_and(|t| t.elapsed() < FORGET));
    }

    /// The pages in the strip's order: each kind's sources by number, or
    /// the kind alone while none is heard.
    fn pages(&self) -> Vec<Shown> {
        let mut pages = Vec::new();
        for kind in Kind::ALL {
            let mut ofkind: Vec<&Source> = self.sources.iter().filter(|s| s.kind == kind).collect();
            ofkind.sort_by_key(|s| s.number);
            if ofkind.is_empty() {
                pages.push(Shown::Kind(kind));
            }
            pages.extend(ofkind.iter().map(|s| Shown::Source(s.sender)));
        }
        pages
    }

    fn source(&self, sender: SocketAddr) -> Option<&Source> {
        self.sources.iter().find(|s| s.sender == sender)
    }

    fn kind_of(&self, page: Shown) -> Option<Kind> {
        match page {
            Shown::Source(sender) => self.source(sender).map(|s| s.kind),
            Shown::Kind(kind) => Some(kind),
        }
    }

    fn label(&self, page: Shown) -> String {
        match page {
            Shown::Source(sender) => self.source(sender).map_or_else(String::new, Source::label),
            Shown::Kind(kind) => kind.label().to_string(),
        }
    }

    /// Keeps the page shown while it is in the strip; once it is gone, the
    /// first page of the same kind, else the strip's first.
    fn settle(&mut self, before: Option<Kind>) {
        let pages = self.pages();
        if pages.contains(&self.shown) {
            return;
        }
        self.shown = pages.iter().copied().find(|&p| self.kind_of(p) == before).unwrap_or(pages[0]);
    }

    fn switch(&mut self, ctx: &egui::Context) {
        let pages = self.pages();
        let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9];
        ctx.input(|input| {
            for (page, key) in pages.iter().zip(keys) {
                if input.key_pressed(key) {
                    self.shown = *page;
                }
            }
            if input.key_pressed(egui::Key::Tab) {
                let at = pages.iter().position(|&p| p == self.shown).unwrap_or(0);
                self.shown = pages[(at + 1) % pages.len()];
            }
        });
    }

    /// The strip of pages, each lit while it shows and dark while it is a
    /// kind none of whose processes is heard; then the shown source's
    /// state, its clock, and the address it sends from.
    fn status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (number, page) in self.pages().into_iter().enumerate() {
                let lit = page == self.shown;
                let text = format!("{}{} {}{} ", if lit { "[" } else { " " }, number + 1, self.label(page), if lit { "]" } else { " " });
                let color = match (lit, page) {
                    (true, _) => COLOR_NORMAL,
                    (false, Shown::Source(_)) => COLOR_DIM,
                    (false, Shown::Kind(_)) => COLOR_BORDER,
                };
                if ui.add(egui::Label::new(colored_mono(&text, color)).sense(egui::Sense::click())).clicked() {
                    self.shown = page;
                }
            }
            ui.label(colored_mono("   ", COLOR_DIM));
            let Some(source) = (match self.shown {
                Shown::Source(sender) => self.source(sender),
                Shown::Kind(_) => None,
            }) else {
                ui.label(colored_mono("NO SIGNAL", COLOR_CRITICAL));
                return;
            };
            if source.feed.is_live() {
                ui.label(colored_mono("CONNECTED", COLOR_NORMAL));
            } else {
                ui.label(colored_mono("NO SIGNAL", COLOR_CRITICAL));
            }
            let (secs, _) = source.feed.clock(source.kind.clock_topic());
            ui.label(colored_mono(
                &format!("   T+ {:02}:{:02}:{:02}", (secs / 3600.0) as u64, ((secs % 3600.0) / 60.0) as u64, (secs % 60.0) as u64),
                COLOR_DIM,
            ));
            if let Some(heard) = source.feed.heard() {
                let ago = heard.elapsed().as_secs_f64();
                ui.label(colored_mono(&format!("   {ago:.0}s"), if ago < 4.0 { COLOR_DIM } else { COLOR_CRITICAL }));
            }
            ui.label(colored_mono(&format!("   {}", source.sender), COLOR_BORDER));
        });
    }
}

impl eframe::App for Console {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let before = self.kind_of(self.shown);
        while let Ok((kind, sender, packet)) = self.rx.try_recv() {
            self.hear(kind, sender, packet);
        }
        self.forget_silent();
        self.settle(before);
        self.switch(ctx);
        ctx.request_repaint_after(Duration::from_millis(250));

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(COLOR_BG).inner_margin(8.0)).show(ctx, |ui| {
            let font = mono_font();
            let cw = *self.char_width.get_or_insert_with(|| ui.fonts(|f| f.glyph_width(&font, '0')));
            let rh = *self.row_height.get_or_insert_with(|| ui.fonts(|f| f.row_height(&font)));

            self.status(ui);
            ui.add_space(4.0);

            let Some(source) = (match self.shown {
                Shown::Source(sender) => self.source(sender),
                Shown::Kind(_) => None,
            }) else {
                ui.label(colored_mono("\n  AWAITING TELEMETRY...", COLOR_DIM));
                return;
            };
            source.kind.draw(ui, &source.feed, cw, rh);
        });
    }
}
