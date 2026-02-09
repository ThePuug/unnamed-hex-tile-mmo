use bevy::prelude::*;
use bevy::ecs::system::lifetimeless::SRes;
use iyes_perf_ui::prelude::*;
use iyes_perf_ui::entry::PerfUiEntry;
use iyes_perf_ui::utils::next_sort_key;

use super::config::DiagnosticsState;

// ============================================================================
// Components
// ============================================================================

/// Marker component for the root performance UI entity
///
/// Used to identify the performance overlay for visibility toggling.
#[derive(Component)]
pub struct PerfUiRootMarker;

/// Custom perf UI entry that displays terrain tile count
#[derive(Component, Debug, Clone)]
#[require(PerfUiRoot)]
pub struct PerfUiTerrainTiles {
    /// Custom label. If empty (default), the default label will be used.
    pub label: String,
    /// Sort Key (control where the entry will appear in the Perf UI).
    pub sort_key: i32,
}

impl Default for PerfUiTerrainTiles {
    fn default() -> Self {
        Self {
            label: String::from("Terrain Tiles"),
            sort_key: next_sort_key(),
        }
    }
}

impl PerfUiEntry for PerfUiTerrainTiles {
    type SystemParam = SRes<crate::common::resources::map::Map>;
    type Value = usize;

    fn label(&self) -> &str {
        if self.label.is_empty() {
            "Terrain Tiles"
        } else {
            &self.label
        }
    }

    fn sort_key(&self) -> i32 {
        self.sort_key
    }

    fn update_value(
        &self,
        param: &mut <Self::SystemParam as bevy::ecs::system::SystemParam>::Item<'_, '_>,
    ) -> Option<Self::Value> {
        Some(param.len())
    }

    fn format_value(&self, value: &Self::Value) -> String {
        format!("{}", value)
    }
}

// ============================================================================
// Systems
// ============================================================================

/// Creates the performance UI overlay on startup
///
/// The UI displays default metrics (FPS, frame time, entity count, etc.)
/// and respects the initial visibility setting from DiagnosticsState.
pub fn setup_performance_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
) {
    // Spawn the iyes_perf_ui root with default entries and our custom terrain tiles entry
    commands.spawn((
        PerfUiRootMarker,
        PerfUiRoot::default(),
        PerfUiDefaultEntries::default(),
        PerfUiTerrainTiles::default(),
        if state.perf_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));
}

