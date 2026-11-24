use bevy::prelude::*;

use crate::plugins::diagnostics::DateField;

/// Which coordinate system the goto input expects.
#[cfg(feature = "admin")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GotoCoordType {
    WorldUnits,
    QR,
}

/// Text input state for the goto coordinate entry.
#[cfg(feature = "admin")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GotoInputState {
    pub coord_type: GotoCoordType,
    /// 0 = first field (x / q), 1 = second field (y / r)
    pub active_field: usize,
    pub buffers: [String; 2],
}

#[cfg(feature = "admin")]
impl GotoInputState {
    pub fn new(coord_type: GotoCoordType) -> Self {
        Self {
            coord_type,
            active_field: 0,
            buffers: [String::new(), String::new()],
        }
    }

    pub fn field_labels(&self) -> [&'static str; 2] {
        match self.coord_type {
            GotoCoordType::WorldUnits => ["X", "Y"],
            GotoCoordType::QR => ["Q", "R"],
        }
    }
}

/// The developer console's state. Held in every build, so what reads
/// `visible` compiles everywhere; only an admin build has the console
/// that sets it.
#[derive(Resource, Clone, PartialEq)]
pub struct DevConsole {
    /// Whether the console is currently visible
    pub visible: bool,
    /// Current menu being displayed
    pub current_menu: MenuPath,
    /// Navigation history (breadcrumb trail)
    pub history: Vec<MenuPath>,
    /// Active goto text input (when in GotoInput menu)
    #[cfg(feature = "admin")]
    pub goto_input: Option<GotoInputState>,
    /// Text buffer for the hour typed (when in the WorldClock menu)
    pub clock_buf: String,
    /// How long an arrow has scrubbed the world's clock, in seconds.
    pub clock_scrub_secs: f32,
    /// The date field Up and Down step (when in the WorldClock menu).
    pub clock_date_field: DateField,
    /// How far back a rollback goes (when in the Rollback menu).
    pub rollback: Rollback,
    /// Text buffer for the value typed (when in a Setting menu).
    pub setting_buf: String,
}

impl Default for DevConsole {
    fn default() -> Self {
        Self {
            visible: false,
            current_menu: MenuPath::Root,
            history: Vec::new(),
            #[cfg(feature = "admin")]
            goto_input: None,
            clock_buf: String::new(),
            clock_scrub_secs: 0.0,
            clock_date_field: DateField::default(),
            rollback: Rollback::default(),
            setting_buf: String::new(),
        }
    }
}

/// How far back a rollback goes: so many of a unit, of game time or of
/// real time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rollback {
    pub game: bool,
    /// 0 minutes, 1 hours, 2 days.
    pub unit: usize,
    pub amount: u32,
    /// Whether Enter was pressed once: the next rolls back.
    pub armed: bool,
}

impl Default for Rollback {
    fn default() -> Self {
        Self { game: false, unit: 0, amount: 5, armed: false }
    }
}

impl Rollback {
    pub const UNITS: [&'static str; 3] = ["minutes", "hours", "days"];

    /// The span it goes back, in real milliseconds: a game minute, hour
    /// and day are the calendar's.
    pub fn ms(self) -> u64 {
        use common_bevy::systems::{DAY_MS, HOUR_MS, MINUTE_MS};
        let unit = match (self.game, self.unit) {
            (true, 0) => MINUTE_MS as u64,
            (true, 1) => HOUR_MS as u64,
            (true, _) => DAY_MS as u64,
            (false, 0) => 60_000,
            (false, 1) => 3_600_000,
            (false, _) => 86_400_000,
        };
        self.amount as u64 * unit
    }

    pub fn says(self) -> String {
        format!("{} {} of {} time", self.amount, Self::UNITS[self.unit], if self.game { "game" } else { "real" })
    }
}

/// Represents the current menu path in the console hierarchy
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuPath {
    Root,
    /// What every player shares: the clock and the rollback
    World,
    /// Where the client stands and whom it sees as
    Travel,
    /// What is staged ahead of the actor the client sees as
    Stages,
    /// The server's settings, for every player
    Settings,
    /// One setting, its value typed
    Setting(common_bevy::settings::Setting),
    /// What this client alone draws and sends: its overlays and its
    /// added latency
    Client,
    WorldClock,
    /// Roll the whole world back to an earlier moment
    #[cfg(feature = "admin")]
    Rollback,
    #[cfg(feature = "admin")]
    GotoSelect,
    #[cfg(feature = "admin")]
    GotoInput,
    #[cfg(feature = "admin")]
    View,
    /// Latency added to the client's own traffic
    #[cfg(feature = "admin")]
    Latency,
    /// Pick a party's archetype
    #[cfg(feature = "admin")]
    Stage(Staging),
}

/// What the console stages ahead of the actor the client sees as.
#[cfg(feature = "admin")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Staging {
    /// A den that actor can beat alone, out of its reach
    Den,
    /// One fighter at the balance arena's level, out of its reach
    Party,
    /// One fighter at the balance arena's level, engaging it
    Opposition,
}

/// The dens the console places, in menu order from numpad 1.
#[cfg(feature = "admin")]
pub const DENS: [(&str, common_bevy::archetype::EnemyArchetype); 6] = {
    use common_bevy::archetype::EnemyArchetype::*;
    [("Dog Pack", Berserker), ("Forest Sprites", Flanker), ("Juggernauts", Juggernaut), ("Defenders", Defender), ("Skirmishers", Skirmisher), ("Ambushers", Ambusher)]
};

impl MenuPath {
    pub fn display_name(&self) -> &str {
        match self {
            MenuPath::Root => "Main Menu",
            MenuPath::World => "World",
            MenuPath::Travel => "Travel",
            MenuPath::Stages => "Stage",
            MenuPath::Settings => "Settings",
            MenuPath::Setting(setting) => setting.name(),
            MenuPath::Client => "Client",
            MenuPath::WorldClock => "World Clock",
            #[cfg(feature = "admin")]
            MenuPath::Rollback => "Rollback",
            #[cfg(feature = "admin")]
            MenuPath::GotoSelect => "Goto — Select Coordinates",
            #[cfg(feature = "admin")]
            MenuPath::GotoInput => "Goto — Enter Coordinates",
            #[cfg(feature = "admin")]
            MenuPath::View => "View",
            #[cfg(feature = "admin")]
            MenuPath::Latency => "Added Latency",
            #[cfg(feature = "admin")]
            MenuPath::Stage(Staging::Den) => "Spawn Den",
            #[cfg(feature = "admin")]
            MenuPath::Stage(Staging::Party) => "Stage Party",
            #[cfg(feature = "admin")]
            MenuPath::Stage(Staging::Opposition) => "Stage Opposition",
        }
    }
}
