//! The server, as the admin console's child: the console is the one
//! thing that starts a server, and it stops with the console.
//!
//! The console holds the other end of the server's standard input and
//! says nothing on it. Letting go of it is the word to stop: the server
//! writes what it holds and goes, whether the console closed it or died.
//! A restart is a stop and, once the server is gone, a start: a new
//! process, built anew where the console runs from the workspace.

use std::{
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

use common::settings::CONSOLE_ENV;

/// How long a server let go of is waited on, as the console closes,
/// before it is killed.
const PATIENCE: Duration = Duration::from_secs(20);

/// How the server stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// None started, or the last one gone: how it went, where it did.
    Stopped(Option<String>),
    Running(u32),
    /// Let go of and not yet gone.
    Stopping(u32),
}

#[derive(Default)]
pub struct Server {
    child: Option<(Child, Option<ChildStdin>)>,
    /// Whether to start one once the one stopping is gone.
    again: bool,
    /// When the one stopping was let go of.
    let_go: Option<Instant>,
    /// How the last one went.
    went: Option<String>,
}

/// What starts a server: cargo where the console itself was run by it,
/// so the server is built as it is started; else the executable beside
/// the console's own.
fn command() -> Command {
    /// What `cargo run` tells the console of its own package.
    const PACKAGE_ENV: [&str; 7] = [
        "CARGO_MANIFEST_DIR",
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
        "CARGO_PKG_VERSION_PRE",
        "CARGO_MANIFEST_LINKS",
    ];
    match std::env::var_os("CARGO") {
        Some(cargo) => {
            // The workspace it was built in, wherever it is run from
            let workspace = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
            let mut command = Command::new(cargo);
            command.args(["run", "--manifest-path", workspace, "--bin", "server"]);
            // Cargo checks a build script's `rerun-if-env-changed` against its
            // own environment, and ring's names these: left as the console's,
            // every build from here and then from a shell rebuilds ring.
            for name in PACKAGE_ENV {
                command.env_remove(name);
            }
            command
        }
        None => {
            let beside = std::env::current_exe().map(|console| console.with_file_name(format!("server{}", std::env::consts::EXE_SUFFIX)));
            Command::new(beside.unwrap_or_else(|_| "server".into()))
        }
    }
}

impl Server {
    /// Looks at the child, and starts the next where a restart waited on
    /// this one going.
    pub fn state(&mut self) -> State {
        if let Some((child, stdin)) = &mut self.child {
            match child.try_wait() {
                Ok(None) => return if stdin.is_some() { State::Running(child.id()) } else { State::Stopping(child.id()) },
                Ok(Some(status)) => self.went = Some(if status.success() { "stopped".to_string() } else { format!("went with {status}") }),
                Err(e) => self.went = Some(format!("lost: {e}")),
            }
            self.child = None;
            // Beside the server's own lines, where its output goes
            if let Some(let_go) = self.let_go.take() {
                tracing::info!("the server was gone {:.1?} after it was let go of", let_go.elapsed());
            }
            if std::mem::take(&mut self.again) {
                self.start();
                return self.state();
            }
        }
        State::Stopped(self.went.clone())
    }

    /// Starts a server where none runs. Its output goes where the
    /// console's does.
    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }
        match command().env(CONSOLE_ENV, "1").stdin(Stdio::piped()).spawn() {
            Ok(mut child) => {
                let stdin = child.stdin.take();
                self.child = Some((child, stdin));
                self.went = None;
            }
            Err(e) => self.went = Some(format!("would not start: {e}")),
        }
    }

    /// Lets go of the server: it writes what it holds and stops.
    pub fn stop(&mut self) {
        self.again = false;
        if let Some((_, stdin)) = &mut self.child {
            if stdin.take().is_some() {
                self.let_go = Some(Instant::now());
            }
        }
    }

    /// Stops the server and starts one once it is gone.
    pub fn restart(&mut self) {
        self.stop();
        self.again = self.child.is_some();
        if !self.again {
            self.start();
        }
    }
}

impl Drop for Server {
    /// The console is closing: the server is let go of and waited on, and
    /// killed only where it will not go.
    fn drop(&mut self) {
        self.stop();
        let Some((mut child, _)) = self.child.take() else { return };
        let asked = Instant::now();
        while asked.elapsed() < PATIENCE {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
    }
}
