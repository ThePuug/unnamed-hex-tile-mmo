//! Every metric the client keeps, published to `common::metrics::CLIENT_GROUP`
//! twice a second, whether or not anything is listening: the console's
//! client page and the `metrics` CLI read them there.
//!
//! Each topic goes as its own packet, and a field is named `topic/field`:
//! `frame`, `process`, `heap` (debug builds only, see `heap`), `world`,
//! `terrain`, `cover`, `census`, `network`, `diag` (every Bevy diagnostic
//! still being measured, by its path) and `timings` (each timer's p95 and
//! count since the last publication).

use std::{borrow::Cow, collections::VecDeque, time::Duration};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use common::metrics::{self, Cadence, MetricsPacket};
use common_bevy::{
    components::{behaviour::PlayerControlled, Actor},
    resources::map::Map,
};
use qrz::Convert;

use super::{network_ui::NetworkMetrics, RenderCensus};

const MB: f64 = 1_048_576.0;

/// How often the metrics are published.
const EVERY: Duration = Duration::from_millis(500);

/// The span the frame time's p95 is taken over.
const FRAME_WINDOW_SECS: f64 = 2.0;

/// How long a diagnostic may go unmeasured and still be published: a pass
/// that stopped running keeps its last value in the store, which would
/// read as current.
const STALE: Duration = Duration::from_secs(1);

/// The socket, when it opened, and what the publications are built from
/// between them.
#[derive(Resource)]
pub struct Publication {
    socket: Option<metrics::Publisher>,
    due: Duration,
    /// Each frame's time and when it ended, for the last
    /// `FRAME_WINDOW_SECS`.
    frames: VecDeque<(f64, f64)>,
}

impl Default for Publication {
    fn default() -> Self {
        let socket = metrics::publisher(metrics::CLIENT_GROUP)
            .inspect_err(|e| warn!("metrics: no publisher on {}: {e}", metrics::CLIENT_GROUP))
            .ok();
        Self { socket, due: Duration::ZERO, frames: VecDeque::new() }
    }
}

impl Publication {
    fn frame_p95(&self) -> f64 {
        let mut times: Vec<f64> = self.frames.iter().map(|&(_, ms)| ms).collect();
        if times.is_empty() {
            return 0.0;
        }
        times.sort_by(f64::total_cmp);
        times[((times.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)]
    }
}

type Fields = Vec<(Cow<'static, str>, f32)>;

fn field(name: impl Into<Cow<'static, str>>, value: impl Into<f64>) -> (Cow<'static, str>, f32) {
    (name.into(), value.into() as f32)
}

#[allow(clippy::too_many_arguments)]
pub fn publish(
    mut publication: ResMut<Publication>,
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    network: Res<NetworkMetrics>,
    timers: Res<crate::resources::ClientTimers>,
    census: Res<RenderCensus>,
    cover: Res<crate::plugins::cover::CoverDraws>,
    lod: Res<crate::resources::LodTriangleStats>,
    map: Res<Map>,
    origin: Res<crate::resources::RenderOrigin>,
    player: Query<&Transform, (With<Actor>, With<PlayerControlled>)>,
) {
    let now = time.elapsed_secs_f64();
    if let Some(ms) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.value()) {
        publication.frames.push_back((now, ms));
    }
    while publication.frames.front().is_some_and(|&(at, _)| at < now - FRAME_WINDOW_SECS) {
        publication.frames.pop_front();
    }
    publication.due = publication.due.saturating_sub(time.delta());
    if !publication.due.is_zero() {
        return;
    }
    publication.due = EVERY;
    let Some(socket) = &publication.socket else { return };

    let p95 = publication.frame_p95();
    let fps = if p95 > 0.0 { 1000.0 / p95 } else { 0.0 };
    let mut topics: Vec<(&'static str, Cadence, Fields)> = vec![
        ("frame", Cadence::Snapshot, vec![field("p95_ms", p95), field("fps", fps)]),
    ];
    let memory = metrics::memory();
    topics.push(("process", Cadence::Snapshot, vec![field("memory_mb", memory.working_set as f64 / MB), field("committed_mb", memory.committed as f64 / MB)]));
    #[cfg(debug_assertions)]
    topics.push(("heap", Cadence::Snapshot, super::heap::report()));

    let mut world = vec![field("tiles", map.len() as f64)];
    if let Ok(transform) = player.single() {
        let qrz: qrz::Qrz = map.convert(origin.world(transform.translation));
        let z = map.get_by_qr(qrz.q, qrz.r).map_or(qrz.z, |(real, _)| real.z);
        let (q, r) = (qrz.q as f64, qrz.r as f64);
        world.extend([
            field("q", q),
            field("r", r),
            field("z", z as f64),
            field("wx", q + r * 0.5),
            field("wy", r * 3f64.sqrt() / 2.0),
        ]);
    }
    topics.push(("world", Cadence::Snapshot, world));

    let mut terrain = vec![
        field("tris", lod.total_tris as f64),
        field("chunks", lod.mesh_count),
        field("async_mesh", lod.async_mesh),
    ];
    for (&r, &(tris, chunks)) in &lod.per_band {
        terrain.push(field(format!("band/{r}/tris"), tris as f64));
        terrain.push(field(format!("band/{r}/chunks"), chunks));
    }
    topics.push(("terrain", Cadence::Snapshot, terrain));

    topics.push((
        "cover",
        Cadence::Snapshot,
        vec![
            field("models", cover.model_instances),
            field("model_draws", cover.models),
            field("cards", cover.card_instances),
            field("card_draws", cover.cards),
        ],
    ));

    let mut groups = Vec::new();
    for (name, group) in [("terrain", &census.terrain), ("cover", &census.cover), ("actors", &census.actors), ("other", &census.other)] {
        groups.push(field(format!("{name}/draws"), group.draws));
        groups.push(field(format!("{name}/instances"), group.instances));
        groups.push(field(format!("{name}/triangles"), group.triangles as f64));
    }
    groups.push(field("total/triangles", census.total_triangles() as f64));
    topics.push(("census", Cadence::Snapshot, groups));

    topics.push((
        "network",
        Cadence::Snapshot,
        vec![field("bytes_per_sec", network.displayed_bytes_per_sec()), field("messages_per_sec", network.displayed_messages_per_sec())],
    ));

    let measured = diagnostics
        .iter()
        .filter(|d| d.measurement().is_some_and(|m| m.time.elapsed() < STALE))
        .filter_map(|d| d.smoothed().map(|v| field(d.path().as_str().to_owned(), v)))
        .collect();
    topics.push(("diag", Cadence::Snapshot, measured));

    let mut timings = Vec::new();
    for (name, p95, count) in timers.drain() {
        timings.push(field(format!("{name}.p95"), p95));
        timings.push(field(format!("{name}.n"), count));
    }
    topics.push(("timings", Cadence::Event, timings));

    for (group, cadence, fields) in topics {
        socket.send(&MetricsPacket { group: Cow::Borrowed(group), cadence, timestamp_secs: now, fields });
    }
}
