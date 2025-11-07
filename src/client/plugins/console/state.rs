use bevy::prelude::*;

/// Resource that tracks developer console state
#[derive(Resource)]
pub struct DevConsole {
    /// Whether the console is currently visible
    pub visible: bool,
    /// Current menu being displayed
    pub current_menu: MenuPath,
    /// Navigation history (breadcrumb trail)
    pub history: Vec<MenuPath>,
}

impl Default for DevConsole {
    fn default() -> Self {
        Self {
            visible: false,
            current_menu: MenuPath::Root,
            history: Vec::new(),
        }
    }
}

/// Represents the current menu path in the console hierarchy
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MenuPath {
    Root,
    Terrain,
    Performance,
}

impl MenuPath {
    /// Get human-readable name for breadcrumb display
    pub fn display_name(&self) -> &str {
        match self {
            MenuPath::Root => "Main Menu",
            MenuPath::Terrain => "Terrain Settings",
            MenuPath::Performance => "Performance Monitoring",
        }
    }
}
