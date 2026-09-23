//! The metrics, written where something other than an eye can read
//! them.
//!
//! A metric belongs on the overlay: that is what it is for, and what it
//! is watched on. This writes those same numbers to a file so a run can
//! be measured without a person reading the screen — the overlay draws
//! through egui, which a screen capture does not reliably carry, and a
//! number nobody can read is a number nobody can check.
//!
//! Snapshots are appended, never overwritten. A render number means
//! nothing on its own and everything beside the one taken a moment
//! later with a single setting changed, so a log that keeps both is the
//! whole point: a file that streams over itself leaves only the last
//! state, which is the one comparison that cannot be made.
//!
//! Nothing here defines a metric. It reads the sources the overlay
//! reads, so a metric added there arrives here by being added there.

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use std::fmt::Write as _;
use std::time::Duration;

use super::RenderCensus;
use crate::plugins::settings::VideoSettings;

/// How often a snapshot is appended while logging without being asked
/// each time. Far apart, because a run is read afterwards and a log of
/// a thousand near-identical blocks is no easier to read than none.
const EVERY: Duration = Duration::from_secs(5);

/// How long a diagnostic may go unmeasured and still be written.
const STALE: Duration = Duration::from_secs(1);

/// Where the snapshots land. Gitignored, beside the other proofs.
const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/client/metrics.log");

/// How the metrics are being logged: every few seconds while `on`, and
/// once wherever `asked` is set, which the console does on a keypress.
#[derive(Resource, Default)]
pub struct MetricsDump {
    pub on: bool,
    pub asked: bool,
    due: Duration,
}

impl MetricsDump {
    /// Logging from the start when the binary was launched with
    /// `--dump-metrics`.
    pub fn from_args() -> Self {
        Self { on: std::env::args().any(|a| a == "--dump-metrics"), asked: false, due: Duration::ZERO }
    }
}

/// Write every metric the overlay shows, whether or not it is open: a
/// run being measured should not have to be watched as well.
pub fn dump_metrics(
    mut dump: ResMut<MetricsDump>,
    video: Res<VideoSettings>,
    census: Res<RenderCensus>,
    wood: Res<crate::plugins::forest::ForestDraws>,
    diagnostics: Res<DiagnosticsStore>,
    map: Res<common_bevy::resources::map::Map>,
    history: Res<super::metrics_overlay::MetricsHistory>,
    buffers: Res<common_bevy::resources::InputQueues>,
    locs: Query<&common_bevy::components::Loc>,
    time: Res<Time>,
) {
    let asked = std::mem::take(&mut dump.asked);
    dump.due = dump.due.saturating_sub(time.delta());
    if !asked && (!dump.on || !dump.due.is_zero()) {
        return;
    }
    dump.due = EVERY;

    // The local player is the one entity with an input queue.
    let tile = buffers.entities().find_map(|&ent| locs.get(ent).ok()).map(|loc| **loc);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "
# {:.1}s up | tile={} | msaa={} shadows={} vsync={} | {}",
        time.elapsed_secs(),
        tile.map_or("none".to_string(), |t| format!("({},{},{})", t.q, t.r, t.z)),
        video.samples.label(),
        video.shadows.label(),
        video.vsync.label(),
        if asked { "asked for" } else { "every few seconds" },
    );

    // A pass that stopped running — the shadow views once shadows are off —
    // keeps its last value in the store, which would read as current.
    for d in diagnostics.iter() {
        let fresh = d.measurement().is_some_and(|m| m.time.elapsed() < STALE);
        if let (true, Some(v)) = (fresh, d.smoothed()) {
            let _ = writeln!(out, "{} = {:.4}", d.path(), v);
        }
    }

    let _ = writeln!(out, "map/tiles = {}", map.len());
    let _ = writeln!(out, "forest/tree_draws = {}", wood.models);
    let _ = writeln!(out, "forest/trees = {}", wood.model_trees);
    let _ = writeln!(out, "forest/card_draws = {}", wood.cards);
    let _ = writeln!(out, "forest/cards = {}", wood.card_trees);

    for (name, group) in [
        ("terrain", &census.terrain),
        ("forest", &census.forest),
        ("actors", &census.actors),
        ("other", &census.other),
    ] {
        let _ = writeln!(out, "census/{name}/draws = {}", group.draws);
        let _ = writeln!(out, "census/{name}/instances = {}", group.instances);
        let _ = writeln!(out, "census/{name}/triangles = {}", group.triangles);
    }
    let _ = writeln!(out, "census/total/triangles = {}", census.total_triangles());
    let _ = writeln!(out, "census/forest/interior_trees = {}", census.interior.trees);
    let _ = writeln!(out, "census/forest/interior_savable_triangles = {}", census.interior.savable);

    // The cover of the player's own mesh region, tile by tile: how dense
    // the wood about the player is, in the terms the interior rule reads.
    if let Some(t) = tile {
        let region = common_bevy::summary::mesh_region_lattice();
        let (mut tiles, mut covered, mut treed, mut scrub_only, mut interior) = (0, 0, 0, 0, 0);
        for (q, r) in region.tiles_in_cell(region.cell_id(t.q, t.r)) {
            tiles += 1;
            let cover = map.cover_at(q, r);
            if cover.is_empty() {
                continue;
            }
            covered += 1;
            if crate::plugins::forest::trees_on(&map, q, r) > 0 {
                treed += 1;
            } else {
                scrub_only += 1;
            }
            if crate::plugins::forest::is_interior(&map, q, r) {
                interior += 1;
            }
        }
        let _ = writeln!(out, "region/tiles = {tiles}");
        let _ = writeln!(out, "region/covered_tiles = {covered}");
        let _ = writeln!(out, "region/tiles_with_trees = {treed}");
        let _ = writeln!(out, "region/scrub_only_tiles = {scrub_only}");
        let _ = writeln!(out, "region/interior_tiles = {interior}");
    }

    let mut named: Vec<_> = history.timings.iter().collect();
    named.sort_by_key(|(name, _)| **name);
    for (name, entry) in named {
        let _ = writeln!(out, "timer/{name} = {:.4}", entry.latest());
    }

    // Appended: see the module doc. A snapshot that replaces the one
    // before it destroys the only comparison worth having.
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(PATH) {
        let _ = file.write_all(out.as_bytes());
    }
}
