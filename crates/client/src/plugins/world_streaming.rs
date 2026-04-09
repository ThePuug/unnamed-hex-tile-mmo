use bevy::prelude::*;

use crate::{
    resources::{ForcedSummaryRadius, LoadedChunks, LodTriangleStats, SummaryCache, SummaryMeshes},
    systems::world,
};

/// Plugin for terrain streaming and LoD mesh generation.
///
/// Owns chunk loading, eviction, and the summary mesh pipeline
/// (dispatch → async build → poll → spawn/update entities).
pub struct WorldStreamingPlugin;

impl Plugin for WorldStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedChunks>();
        app.init_resource::<SummaryMeshes>();
        app.init_resource::<ForcedSummaryRadius>();
        app.init_resource::<LodTriangleStats>();
        app.init_resource::<SummaryCache>();

        app.add_systems(Update, (
            world::do_spawn,
            world::dispatch_summary_tasks.after(world::do_spawn),
            world::poll_summary_meshes.after(world::dispatch_summary_tasks),
        ));

        #[cfg(feature = "admin")]
        app.add_systems(
            Update,
            world::evict_data.run_if(crate::plugins::flyover::not_in_flyover),
        );
        #[cfg(not(feature = "admin"))]
        app.add_systems(Update, world::evict_data);
    }
}
