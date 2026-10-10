use bevy::prelude::*;

use crate::{
    resources::{LoadedChunks, LodTriangleStats, SummaryCache, SummaryMeshes, TerrainMaterial},
    systems::world,
};

/// Terrain streaming and LoD mesh generation: chunk loading, eviction, and
/// the summary mesh pipeline (dispatch → async build → poll → spawn/update
/// entities).
pub struct WorldStreamingPlugin;

impl Plugin for WorldStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedChunks>();
        app.init_resource::<SummaryMeshes>();
        app.init_resource::<LodTriangleStats>();
        app.init_resource::<SummaryCache>();
        app.init_resource::<TerrainMaterial>();
        app.init_resource::<crate::resources::EdgeCenters>();
        app.init_resource::<crate::resources::CardBand>();
        app.init_resource::<crate::systems::gathering::CoverChanges>();
        app.init_resource::<crate::systems::den::Dens>();

        app.add_systems(Update, (
            world::do_spawn,
            crate::systems::gathering::apply,
            crate::systems::den::apply,
            world::dispatch_summary_tasks.after(world::do_spawn),
            world::poll_summary_meshes.after(world::dispatch_summary_tasks),
            world::update_terrain_cut,
        ));

        app.add_systems(Update, world::evict_data);
    }
}
