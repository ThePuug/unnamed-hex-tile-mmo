//! The Settings page: the server's settings (`common::settings`) as the
//! store keeps them, changed there directly, so it works with no server
//! running; and the server itself (`child`), started, stopped and
//! restarted here. A server that is running hears each change from the
//! store: a hot setting holds at once, a cold one from its next start.
//!
//! The store is asked on a thread of its own, one question at a time, so
//! a store that is slow or gone never holds the window up.

use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use common::settings::{store_url, Setting};
use eframe::egui;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::{
    child::{Server, State},
    widgets::*,
};

/// How often the page reads the store while it shows.
const READ_EVERY: Duration = Duration::from_secs(2);

/// How long a stop or a restart stays armed for its second click.
const ARMED_FOR: Duration = Duration::from_secs(5);

/// What the page asks of the store. Each is answered with every setting
/// kept, as it then stands.
enum Ask {
    Read,
    Set(Setting, i32),
}

/// What a click on the server's row asks for. One that drops every
/// player takes a second click.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Start,
    Stop,
    Restart,
}

/// Every setting kept, by its number, or why the store did not answer.
type Answer = Result<Vec<(i32, i32)>, String>;

pub struct Page {
    asks: mpsc::Sender<Ask>,
    answers: mpsc::Receiver<Answer>,
    /// Whether an ask is out and unanswered.
    pending: bool,
    asked: Option<Instant>,
    /// What the store last answered.
    kept: Option<Answer>,
    /// Each setting's value as the page shows it, in `Setting::ALL`'s
    /// order: as kept, but for the one being typed.
    shown: [i32; Setting::ALL.len()],
    editing: Option<Setting>,
    armed: Option<(Order, Instant)>,
}

impl Page {
    pub fn new() -> Self {
        let (asks, asked) = mpsc::channel();
        let (answer, answers) = mpsc::channel();
        std::thread::spawn(move || work(asked, answer));
        Self { asks, answers, pending: false, asked: None, kept: None, shown: Setting::ALL.map(Setting::default), editing: None, armed: None }
    }

    fn ask(&mut self, ask: Ask) {
        self.pending = self.asks.send(ask).is_ok();
        self.asked = Some(Instant::now());
    }

    /// A setting's value as kept: as built where the store holds no row
    /// for it.
    fn kept(&self, setting: Setting) -> i32 {
        let rows = self.kept.as_ref().and_then(|kept| kept.as_ref().ok());
        rows.and_then(|rows| rows.iter().find(|row| row.0 == setting.id())).map_or(setting.default(), |row| setting.held(row.1))
    }

    /// Takes the store's answers, and reads it again when the last read
    /// is old.
    fn keep_up(&mut self) {
        while let Ok(answer) = self.answers.try_recv() {
            self.pending = false;
            self.kept = Some(answer);
            for (at, setting) in Setting::ALL.into_iter().enumerate() {
                if self.editing != Some(setting) {
                    self.shown[at] = self.kept(setting);
                }
            }
        }
        if !self.pending && self.asked.is_none_or(|asked| asked.elapsed() >= READ_EVERY) {
            self.ask(Ask::Read);
        }
    }

    /// The store's state, for the status line.
    pub fn status(&self) -> (&'static str, egui::Color32) {
        match &self.kept {
            None => ("ASKING THE STORE", COLOR_DIM),
            Some(Ok(_)) => ("STORE OPEN", COLOR_NORMAL),
            Some(Err(_)) => ("NO STORE", COLOR_CRITICAL),
        }
    }

    /// Draws the page: the server's row, then the settings.
    pub fn draw(&mut self, ui: &mut egui::Ui, server: &mut Server) {
        self.keep_up();
        // A value's field, drawn as the buttons are: bright on the dark
        let widgets = &mut ui.visuals_mut().widgets;
        for (state, edge) in [(&mut widgets.inactive, COLOR_DIM), (&mut widgets.hovered, COLOR_NORMAL), (&mut widgets.active, COLOR_NORMAL)] {
            state.weak_bg_fill = COLOR_FIELD;
            state.bg_fill = COLOR_FIELD;
            state.bg_stroke = egui::Stroke::new(1.0_f32, edge);
            state.fg_stroke = egui::Stroke::new(1.0_f32, COLOR_NORMAL);
        }
        self.draw_server(ui, server);
        ui.add_space(8.0);
        if let Some(Err(why)) = &self.kept {
            // The store answers and has no table yet: a server makes it as it starts
            if why.contains("\"setting\" does not exist") {
                ui.label(colored_mono("  This store has no settings yet. START the server once and they show here.", COLOR_WARN));
            } else {
                ui.label(colored_mono(&format!("  The store at {} does not answer: {why}", store_url()), COLOR_CRITICAL));
            }
            return;
        }
        if self.kept.is_none() {
            ui.label(colored_mono("  ASKING THE STORE...", COLOR_DIM));
            return;
        }

        draw_section(ui, "SETTINGS", |ui| {
            let mut editing = None;
            for (at, setting) in Setting::ALL.into_iter().enumerate() {
                let kept = self.kept(setting);
                let (least, most) = setting.range();
                ui.horizontal(|ui| {
                    ui.label(colored_mono(&format!("{:<22}", setting.name()), COLOR_NORMAL));
                    let field = ui.add_sized([90.0, 18.0], egui::DragValue::new(&mut self.shown[at]).range(least..=most));
                    if field.has_focus() || field.dragged() {
                        editing = Some(setting);
                    }
                    let done = field.lost_focus() || field.drag_stopped();
                    if done && self.shown[at] != kept {
                        self.ask(Ask::Set(setting, self.shown[at]));
                    }
                    if button(ui, &format!("AS BUILT {}", setting.default()), COLOR_NORMAL, kept != setting.default()) {
                        self.shown[at] = setting.default();
                        self.ask(Ask::Set(setting, setting.default()));
                    }
                    let (tag, color) = if setting.hot() { ("HOT ", COLOR_NORMAL) } else { ("COLD", COLOR_WARN) };
                    ui.label(colored_mono(&format!(" {tag}  {least}..{most}  "), color));
                    let waits = if setting.hot() { "" } else { ", from the server's next start" };
                    ui.label(colored_mono(&format!("{}{waits}", setting.says()), COLOR_DIM));
                });
            }
            self.editing = editing;
            ui.label(colored_mono("Click a value to type it, or drag it. HOT holds at once for every player; COLD waits for a restart.", COLOR_BORDER));
        });
    }

    /// The server's row: how it stands, and the buttons that start, stop
    /// and restart it.
    fn draw_server(&mut self, ui: &mut egui::Ui, server: &mut Server) {
        let state = server.state();
        let running = matches!(state, State::Running(_));
        let (text, color) = match &state {
            State::Running(pid) => (format!("RUNNING  pid {pid}"), COLOR_NORMAL),
            State::Stopping(pid) => (format!("STOPPING pid {pid}, writing what it holds"), COLOR_WARN),
            State::Stopped(None) => ("STOPPED".to_string(), COLOR_DIM),
            State::Stopped(Some(went)) => (format!("STOPPED  it {went}"), if went == "stopped" { COLOR_DIM } else { COLOR_CRITICAL }),
        };
        draw_section(ui, "SERVER", |ui| {
            ui.horizontal(|ui| {
                let armed = self.armed.filter(|(_, at)| at.elapsed() < ARMED_FOR).map(|(order, _)| order);
                let mut order = None;
                for (wanted, label, enabled) in [(Order::Start, "START", matches!(state, State::Stopped(_))), (Order::Stop, "STOP", running), (Order::Restart, "RESTART", running)] {
                    let lit = armed == Some(wanted);
                    let label = if lit { format!("{label}: CLICK AGAIN") } else { label.to_string() };
                    if button(ui, &label, if lit { COLOR_CRITICAL } else { COLOR_WARN }, enabled) {
                        order = Some(wanted);
                    }
                }
                match order {
                    Some(Order::Start) => server.start(),
                    Some(wanted) if armed == Some(wanted) => {
                        self.armed = None;
                        if wanted == Order::Stop { server.stop() } else { server.restart() }
                    }
                    Some(wanted) => self.armed = Some((wanted, Instant::now())),
                    None => {}
                }
                ui.label(colored_mono(&format!("   {text}"), color));
            });
            ui.label(colored_mono("The console is the one thing that starts a server, and the server stops with it. STOP and RESTART drop every player.", COLOR_BORDER));
        });
    }
}

/// What a button and a value's field are filled with: a step off the
/// page's ground, so their bright text and edge carry them.
const COLOR_FIELD: egui::Color32 = egui::Color32::from_rgb(22, 34, 22);

/// A button reading `text` in `color`, edged in it; one that cannot be
/// pressed is edged and lettered in the page's border colour and takes
/// no click. Whether it was clicked.
fn button(ui: &mut egui::Ui, text: &str, color: egui::Color32, enabled: bool) -> bool {
    let color = if enabled { color } else { COLOR_BORDER };
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let button = egui::Button::new(colored_mono(text, color)).fill(COLOR_FIELD).stroke(egui::Stroke::new(1.0_f32, color)).sense(sense);
    ui.add(button).clicked()
}

/// Answers each ask in turn for as long as the page lives, on a
/// connection opened at the first and again after any that fails.
fn work(asks: mpsc::Receiver<Ask>, answers: mpsc::Sender<Answer>) {
    let mut pool = None;
    for ask in asks {
        let answer = futures_lite::future::block_on(answer(&mut pool, ask));
        if answer.is_err() {
            pool = None;
        }
        if answers.send(answer.map_err(|e| e.to_string())).is_err() {
            return;
        }
    }
}

async fn answer(pool: &mut Option<PgPool>, ask: Ask) -> Result<Vec<(i32, i32)>, sqlx::Error> {
    let pool = match pool {
        Some(pool) => pool,
        None => pool.insert(PgPoolOptions::new().max_connections(1).acquire_timeout(Duration::from_secs(3)).connect(&store_url()).await?),
    };
    match ask {
        Ask::Read => {}
        Ask::Set(setting, value) => {
            sqlx::query("INSERT INTO setting (id, value) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET value = EXCLUDED.value")
                .bind(setting.id())
                .bind(setting.held(value))
                .execute(&*pool)
                .await?;
        }
    }
    sqlx::query_as("SELECT id, value FROM setting").fetch_all(&*pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On the store `DATABASE_URL` names: the console starts a server,
    /// which hears a hot setting written here; a restart leaves another
    /// process running; and a stop leaves none. Puts the setting back as
    /// built.
    #[test]
    #[ignore = "needs the store, and starts a server on it"]
    fn the_console_starts_a_server_that_hears_it_and_stops_with_it() {
        fn wait(server: &mut Server, wanted: impl Fn(&State) -> bool) -> State {
            let asked = Instant::now();
            while !wanted(&server.state()) {
                assert!(asked.elapsed() < Duration::from_secs(120), "the server stands {:?}", server.state());
                std::thread::sleep(Duration::from_millis(200));
            }
            server.state()
        }
        let mut server = Server::default();
        server.start();
        let State::Running(first) = wait(&mut server, |state| matches!(state, State::Running(_))) else { unreachable!() };
        // Long enough to be built, to open the store and to listen on it
        std::thread::sleep(Duration::from_secs(40));
        assert_eq!(server.state(), State::Running(first), "the server stays up");

        let setting = Setting::DensShown;
        let mut pool = None;
        let rows = futures_lite::future::block_on(answer(&mut pool, Ask::Set(setting, 7))).unwrap();
        assert!(rows.contains(&(setting.id(), 7)));
        std::thread::sleep(Duration::from_secs(2));
        futures_lite::future::block_on(answer(&mut pool, Ask::Set(setting, setting.default()))).unwrap();
        std::thread::sleep(Duration::from_secs(2));

        server.restart();
        wait(&mut server, |state| matches!(state, State::Running(pid) if *pid != first));
        std::thread::sleep(Duration::from_secs(20));

        server.stop();
        assert_eq!(wait(&mut server, |state| matches!(state, State::Stopped(_))), State::Stopped(Some("stopped".to_string())));
    }
}
