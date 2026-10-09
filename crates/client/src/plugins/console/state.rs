use bevy::prelude::*;

use crate::plugins::diagnostics::DateField;

/// Which coordinate system the goto input expects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GotoCoordType {
    WorldUnits,
    QR,
}

/// Text input state for the goto coordinate entry.
#[derive(Clone, Debug)]
pub struct GotoInputState {
    pub coord_type: GotoCoordType,
    /// 0 = first field (x / q), 1 = second field (y / r)
    pub active_field: usize,
    pub buffers: [String; 2],
}

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

/// Resource that tracks developer console state
#[derive(Resource)]
pub struct DevConsole {
    /// Whether the console is currently visible
    pub visible: bool,
    /// Current menu being displayed
    pub current_menu: MenuPath,
    /// Navigation history (breadcrumb trail)
    pub history: Vec<MenuPath>,
    /// Active goto text input (when in GotoInput menu)
    pub goto_input: Option<GotoInputState>,
    /// Text buffer for the lighting hour (when in LightingTime menu)
    pub lighting_time_buf: String,
    /// How long an arrow has scrubbed the lighting clock, in seconds.
    pub lighting_scrub_secs: f32,
    /// The date field Up and Down step (when in LightingTime menu).
    pub lighting_date_field: DateField,
}

impl Default for DevConsole {
    fn default() -> Self {
        Self {
            visible: false,
            current_menu: MenuPath::Root,
            history: Vec::new(),
            goto_input: None,
            lighting_time_buf: String::new(),
            lighting_scrub_secs: 0.0,
            lighting_date_field: DateField::default(),
        }
    }
}

/// Represents the current menu path in the console hierarchy
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuPath {
    Root,
    Terrain,
    LightingTime,
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
            MenuPath::Terrain => "Terrain Settings",
            MenuPath::LightingTime => "Lighting Time",
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
