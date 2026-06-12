use std::time::Duration;

use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;

use common_bevy::chunk::WorldDiscoveryCache;
use crate::systems::{actor, summary, world};

/// Plugin for server-side terrain streaming.
///
/// Owns chunk discovery, async generation, polling, and summary computation.
pub struct WorldStreamingPlugin;

impl Plugin for WorldStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldDiscoveryCache>();
        app.init_resource::<actor::ChunkTaskQueue>();
        app.init_resource::<summary::SummaryTaskQueue>();

        app.add_systems(Startup, world::setup);

        app.add_systems(Update, (
            actor::do_spawn_discover,
            actor::try_discover_chunk,
            actor::poll_chunk_tasks,
            // Visible-region enumeration costs ~tens of thousands of HashSet
            // ops per player; 4 Hz is plenty for terrain that only changes
            // with player movement. Polling stays per-frame so completed
            // summaries flow to clients without delay.
            summary::dispatch_summary_tasks.run_if(on_timer(Duration::from_millis(250))),
            summary::poll_summary_tasks,
        ));
    }
}
