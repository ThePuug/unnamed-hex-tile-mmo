//! The metrics, written where something other than an eye can read
//! them.
//!
//! A metric belongs on the overlay: that is what it is for, and what it
//! is watched on. This writes those same numbers to a file so a run can
//! be measured without a person reading the screen — the overlay draws
//! through egui, which a screen capture does not reliably carry, and a
//! number nobody can read is a number nobody can check.
//!
//! Nothing here defines a metric. It reads the sources the overlay
//! reads, so a metric added there arrives here by being added there.

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use std::fmt::Write as _;
use std::time::Duration;

use super::{DiagnosticsState, RenderCensus};

/// How often a snapshot is written while dumping.
const EVERY: Duration = Duration::from_millis(500);

/// Where the snapshot lands. Gitignored, beside the other proofs.
const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/client/metrics.txt");

/// Whether the metrics are being written, and where they have got to.
/// Off unless asked for, by `--dump-metrics` at launch or the console.
#[derive(Resource, Default)]
pub struct MetricsDump {
    pub on: bool,
    due: Duration,
}

impl MetricsDump {
    /// On when the binary was launched with `--dump-metrics`.
    pub fn from_args() -> Self {
        Self { on: std::env::args().any(|a| a == "--dump-metrics"), due: Duration::ZERO }
    }
}

/// Write every metric the overlay shows, whether or not it is open: a
/// run being measured should not have to be watched as well.
pub fn dump_metrics(
    mut dump: ResMut<MetricsDump>,
    state: Res<DiagnosticsState>,
    census: Res<RenderCensus>,
    wood: Res<crate::plugins::forest::ForestDraws>,
    diagnostics: Res<DiagnosticsStore>,
    map: Res<common_bevy::resources::map::Map>,
    history: Res<super::metrics_overlay::MetricsHistory>,
    time: Res<Time>,
) {
    if !dump.on {
        return;
    }
    dump.due = dump.due.saturating_sub(time.delta());
    if !dump.due.is_zero() {
        return;
    }
    dump.due = EVERY;

    let mut out = String::new();
    let _ = writeln!(out, "# msaa={} shadows={}", state.samples.label(), state.shadows.label());

    for d in diagnostics.iter() {
        if let Some(v) = d.smoothed() {
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

    let mut named: Vec<_> = history.timings.iter().collect();
    named.sort_by_key(|(name, _)| **name);
    for (name, entry) in named {
        let _ = writeln!(out, "timer/{name} = {:.4}", entry.latest());
    }

    let _ = std::fs::write(PATH, out);
}
