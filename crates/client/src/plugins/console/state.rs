use bevy::prelude::*;

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
}

impl Default for DevConsole {
    fn default() -> Self {
        Self {
            visible: false,
            current_menu: MenuPath::Root,
            history: Vec::new(),
            goto_input: None,
        }
    }
}

/// Represents the current menu path in the console hierarchy
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuPath {
    Root,
    Terrain,
    Performance,
    #[cfg(feature = "admin")]
    Admin,
    #[cfg(feature = "admin")]
    GotoSelect,
    #[cfg(feature = "admin")]
    GotoInput,
}

impl MenuPath {
    /// Get human-readable name for breadcrumb display
    pub fn display_name(&self) -> &str {
        match self {
            MenuPath::Root => "Main Menu",
            MenuPath::Terrain => "Terrain Settings",
            MenuPath::Performance => "Performance Monitoring",
            #[cfg(feature = "admin")]
            MenuPath::Admin => "Admin Tools",
            #[cfg(feature = "admin")]
            MenuPath::GotoSelect => "Goto — Select Coordinates",
            #[cfg(feature = "admin")]
            MenuPath::GotoInput => "Goto — Enter Coordinates",
        }
    }
}
