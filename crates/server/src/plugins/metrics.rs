use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;

use common_bevy::metrics::{Aggregator, Cadence, MetricsPacket, METRICS_MAGIC, METRICS_VERSION};
use common_bevy::resources::map::Map;
use crate::resources::Lobby;

const DEFAULT_METRICS_PORT: u16 = 5100;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);

// ── Transport (shared UDP socket) ──

#[derive(Clone)]
struct Transport {
    socket: Arc<UdpSocket>,
    target: SocketAddr,
}

impl Transport {
    fn new(port: u16) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind metrics UDP socket");
        socket
            .set_nonblocking(true)
            .expect("failed to set metrics socket non-blocking");
        Self {
            socket: Arc::new(socket),
            target: SocketAddr::from(([127, 0, 0, 1], port)),
        }
    }

    fn send_packet(&self, packet: &MetricsPacket) {
        let Ok(bytes) = bincode::serde::encode_to_vec(packet, bincode::config::legacy()) else {
            return;
        };
        let mut buf = Vec::with_capacity(6 + bytes.len());
        buf.extend_from_slice(&METRICS_MAGIC);
        buf.extend_from_slice(&METRICS_VERSION.to_le_bytes());
        buf.extend_from_slice(&bytes);
        let _ = self.socket.send_to(&buf, self.target);
    }
}

// ── MetricSnapshot ──

struct SnapshotField {
    name: &'static str,
    aggregator: Aggregator,
    value: f32,
}

/// Accumulates field values from multiple systems. Flushes as one UDP
/// packet every 2 seconds. Interior mutability via Mutex so systems
/// access it through `Res<MetricSnapshot>` (no exclusive borrow).
#[derive(Resource)]
pub struct MetricSnapshot {
    group: &'static str,
    field_indices: HashMap<&'static str, usize>,
    state: Mutex<Vec<SnapshotField>>,
    transport: Transport,
    flush_interval: Duration,
    last_flush: Mutex<Duration>,
}

impl MetricSnapshot {
    fn new(group: &'static str, transport: Transport, interval: Duration) -> Self {
        Self {
            group,
            field_indices: HashMap::new(),
            state: Mutex::new(Vec::new()),
            transport,
            flush_interval: interval,
            last_flush: Mutex::new(Duration::ZERO),
        }
    }

    fn register(&mut self, name: &'static str, aggregator: Aggregator) {
        let idx = self.field_indices.len();
        self.field_indices.insert(name, idx);
        self.state.get_mut().unwrap().push(SnapshotField {
            name,
            aggregator,
            value: 0.0,
        });
    }

    /// Record one or more field values. Thread-safe, takes &self.
    pub fn record(&self, fields: &[(&str, f32)]) {
        let mut state = self.state.lock().unwrap();
        for &(name, val) in fields {
            if let Some(&idx) = self.field_indices.get(name) {
                let f = &mut state[idx];
                match f.aggregator {
                    Aggregator::Last => f.value = val,
                    Aggregator::Peak => f.value = f.value.max(val),
                    Aggregator::Sum => f.value += val,
                }
            }
        }
    }

    fn flush(&self, timestamp_secs: f64) {
        let mut state = self.state.lock().unwrap();

        let fields: Vec<(String, f32)> = state
            .iter()
            .map(|f| (f.name.to_string(), f.value))
            .collect();

        let packet = MetricsPacket {
            group: self.group.to_string(),
            cadence: Cadence::Snapshot,
            timestamp_secs,
            fields,
        };
        self.transport.send_packet(&packet);

        // Reset Peak and Sum fields after flush
        for f in state.iter_mut() {
            match f.aggregator {
                Aggregator::Peak | Aggregator::Sum => f.value = 0.0,
                Aggregator::Last => {}
            }
        }
    }
}

// ── SystemTimings (wraps common::timers::SystemTimers + UDP transport) ──

/// Server-side timing accumulator. Wraps `common::timers::SystemTimers` and
/// periodically flushes drained p95/count data to the console via UDP.
#[derive(Resource)]
pub struct SystemTimings {
    timers: common::timers::SystemTimers,
    transport: Transport,
    flush_interval: Duration,
    last_flush: Mutex<Duration>,
}

impl SystemTimings {
    fn new(transport: Transport, interval: Duration) -> Self {
        Self {
            timers: common::timers::SystemTimers::new(),
            transport,
            flush_interval: interval,
            last_flush: Mutex::new(Duration::ZERO),
        }
    }

    /// Create a scoped timer. Records elapsed milliseconds on drop.
    pub fn scope(&self, name: &'static str) -> common::timers::ScopeTimer<'_> {
        self.timers.scope(name)
    }

    fn flush(&self, timestamp_secs: f64) {
        let drained = self.timers.drain();
        if drained.is_empty() { return; }

        let mut fields = Vec::new();
        for (name, p95, count) in drained {
            fields.push((format!("{name}.p95"), p95));
            fields.push((format!("{name}.n"), count));
        }

        self.transport.send_packet(&MetricsPacket {
            group: "timings".to_string(),
            cadence: Cadence::Event,
            timestamp_secs,
            fields,
        });
    }
}

fn maybe_flush_timings(timings: Res<SystemTimings>, time: Res<Time>) {
    let elapsed = time.elapsed();
    let should_flush = {
        let last = *timings.last_flush.lock().unwrap();
        elapsed - last >= timings.flush_interval
    };
    if should_flush {
        timings.flush(elapsed.as_secs_f64());
        *timings.last_flush.lock().unwrap() = elapsed;
    }
}

// ── Plugin ──

pub struct MetricsPlugin {
    pub port: u16,
    pub interval: Duration,
}

impl Default for MetricsPlugin {
    fn default() -> Self {
        Self {
            port: DEFAULT_METRICS_PORT,
            interval: DEFAULT_INTERVAL,
        }
    }
}

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        let transport = Transport::new(self.port);

        // ── Snapshot: server gauges ──
        let mut snapshot = MetricSnapshot::new("server", transport.clone(), self.interval);
        snapshot.register("tick_count", Aggregator::Sum);
        snapshot.register("tick_duration_ms", Aggregator::Last);
        snapshot.register("tick_peak_ms", Aggregator::Peak);
        snapshot.register("tick_overruns", Aggregator::Sum);
        snapshot.register("loaded_hexes", Aggregator::Last);
        snapshot.register("connected_players", Aggregator::Last);
        snapshot.register("npc_count", Aggregator::Last);
        snapshot.register("memory_mb", Aggregator::Last);
        snapshot.register("memory_map_mb", Aggregator::Last);
        snapshot.register("frame_duration_ms", Aggregator::Last);
        snapshot.register("frame_peak_ms", Aggregator::Peak);
        snapshot.register("frame_overruns", Aggregator::Sum);
        snapshot.register("net_sent_bps", Aggregator::Last);
        snapshot.register("net_recv_bps", Aggregator::Last);
        snapshot.register("net_clients", Aggregator::Last);
        snapshot.register("net_ord_buf_pct", Aggregator::Last);
        snapshot.register("net_ord_queue", Aggregator::Last);
        snapshot.register("net_unord_buf_pct", Aggregator::Last);
        snapshot.register("net_unord_queue", Aggregator::Last);
        // Event metrics — deform/query cascade health
        snapshot.register("evt.visible", Aggregator::Last);
        snapshot.register("evt.tile_hits", Aggregator::Last);
        snapshot.register("evt.tile_misses", Aggregator::Last);
        snapshot.register("evt.active", Aggregator::Last);
        // Per-layer metrics
        snapshot.register("evt.plates.index", Aggregator::Last);
        snapshot.register("evt.plates.cell_hits", Aggregator::Last);
        snapshot.register("evt.plates.cell_misses", Aggregator::Last);
        snapshot.register("evt.spines.index", Aggregator::Last);
        snapshot.register("evt.spines.cell_hits", Aggregator::Last);
        snapshot.register("evt.spines.cell_misses", Aggregator::Last);
        snapshot.register("evt.spawner.index", Aggregator::Last);
        snapshot.register("evt.spawner.cell_hits", Aggregator::Last);
        snapshot.register("evt.spawner.cell_misses", Aggregator::Last);
        // Async chunk generation metrics
        snapshot.register("async.task_duration_ms", Aggregator::Peak);
        snapshot.register("async.tasks_in_flight", Aggregator::Last);
        let timings = SystemTimings::new(transport.clone(), self.interval);
        app.insert_resource(snapshot)
            .insert_resource(timings)
            .insert_resource(TickTimer::default())
            .add_systems(FixedFirst, tick_timer_start)
            .add_systems(FixedLast, tick_timer_end)
            .add_systems(Update, (track_frame_time, maybe_flush_snapshot, maybe_flush_timings))
            .add_systems(
                Update,
                (refresh_metric_gauges, drain_event_metrics).run_if(flush_due),
            );
    }
}

fn drain_event_metrics(
    registry: Res<crate::resources::event_registry::EventRegistry>,
    active: Res<crate::systems::engagement_spawner::ActiveSpawners>,
    snapshot: Res<MetricSnapshot>,
) {
    let m = registry.drain_metrics();
    snapshot.record(&[
        ("evt.visible", m.visible as f32),
        ("evt.tile_hits", m.tile_hits as f32),
        ("evt.tile_misses", m.tile_misses as f32),
        ("evt.active", active.0.len() as f32),
    ]);
    for layer in &m.layers {
        let name = &layer.name;
        let k_index = format!("evt.{name}.index");
        let k_hits = format!("evt.{name}.cell_hits");
        let k_misses = format!("evt.{name}.cell_misses");
        snapshot.record(&[
            (k_index.as_str(), layer.indexed as f32),
            (k_hits.as_str(), layer.cell_hits as f32),
            (k_misses.as_str(), layer.cell_misses as f32),
        ]);
    }
}

// ── Systems ──

#[derive(Resource, Default)]
struct TickTimer(Option<Instant>);

fn tick_timer_start(mut timer: ResMut<TickTimer>) {
    timer.0 = Some(Instant::now());
}

fn tick_timer_end(timer: Res<TickTimer>, snapshot: Res<MetricSnapshot>) {
    if let Some(start) = timer.0 {
        let ms = start.elapsed().as_secs_f64() as f32 * 1000.0;
        snapshot.record(&[
            ("tick_duration_ms", ms),
            ("tick_peak_ms", ms),
            ("tick_count", 1.0),
        ]);
        if ms > 125.0 {
            snapshot.record(&[("tick_overruns", 1.0)]);
        }
    }
}

fn track_frame_time(time: Res<Time>, snapshot: Res<MetricSnapshot>) {
    let ms = time.delta().as_secs_f64() as f32 * 1000.0;
    snapshot.record(&[
        ("frame_duration_ms", ms),
        ("frame_peak_ms", ms),
    ]);
    if ms > 125.0 {
        snapshot.record(&[("frame_overruns", 1.0)]);
        warn!("frame took {:.1}ms (budget 125ms)", ms);
    }
}

fn flush_due(snapshot: Res<MetricSnapshot>, time: Res<Time>) -> bool {
    let last = *snapshot.last_flush.lock().unwrap();
    time.elapsed() - last >= snapshot.flush_interval
}

fn refresh_metric_gauges(
    snapshot: Res<MetricSnapshot>,
    map: Res<Map>,
    lobby: Res<Lobby>,
    conn: Res<crate::network::ServerNet>,
    npc_query: Query<(), (With<common_bevy::components::entity_type::EntityType>, Without<common_bevy::components::behaviour::PlayerControlled>)>,
    chunk_tasks: Res<crate::systems::actor::ChunkTaskQueue>,
) {
    snapshot.record(&[
        ("loaded_hexes", map.len() as f32),
        ("connected_players", lobby.len() as f32),
        ("npc_count", npc_query.iter().count() as f32),
        ("memory_mb", process_working_set_bytes() as f32 / 1_048_576.0),
        ("memory_map_mb", map.heap_size_estimate() as f32 / 1_048_576.0),
        ("async.tasks_in_flight", chunk_tasks.in_flight.len() as f32),
    ]);

    // Aggregate network stats across all connected clients
    let mut total_sent = 0.0f64;
    let mut total_recv = 0.0f64;
    let mut client_count = 0u32;
    for client_id in conn.clients_id() {
        if let Ok(info) = conn.network_info(client_id) {
            total_sent += info.bytes_sent_per_second;
            total_recv += info.bytes_received_per_second;
            client_count += 1;
        }
    }
    snapshot.record(&[
        ("net_sent_bps", total_sent as f32),
        ("net_recv_bps", total_recv as f32),
        ("net_clients", client_count as f32),
        ("net_ord_buf_pct", conn.peak_buffer_occupancy(common::network::CH_RELIABLE_ORDERED) * 100.0),
        ("net_ord_queue", conn.p95_queue_depth(|s| &s.ordered.queue) as f32),
        ("net_unord_buf_pct", conn.peak_buffer_occupancy(common::network::CH_RELIABLE_UNORDERED) * 100.0),
        ("net_unord_queue", conn.p95_queue_depth(|s| &s.unordered.queue) as f32),
    ]);
}

fn maybe_flush_snapshot(snapshot: Res<MetricSnapshot>, time: Res<Time>) {
    let elapsed = time.elapsed();
    let should_flush = {
        let last = *snapshot.last_flush.lock().unwrap();
        elapsed - last >= snapshot.flush_interval
    };
    if should_flush {
        snapshot.flush(elapsed.as_secs_f64());
        *snapshot.last_flush.lock().unwrap() = elapsed;
    }
}

#[cfg(windows)]
fn process_working_set_bytes() -> u64 {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    unsafe {
        let mut pmc = std::mem::zeroed::<ProcessMemoryCounters>();
        pmc.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
            pmc.WorkingSetSize as u64
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
fn process_working_set_bytes() -> u64 {
    0
}
