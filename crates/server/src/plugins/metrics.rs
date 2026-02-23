use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bevy::prelude::*;

use common_bevy::metrics::{METRICS_MAGIC, METRICS_VERSION, ServerMetrics};
use common_bevy::resources::map::Map;
use crate::resources::Lobby;

const DEFAULT_METRICS_PORT: u16 = 5100;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);

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
        app.init_resource::<ServerMetrics>()
            .insert_resource(MetricsTransport::new(self.port))
            .insert_resource(TickTimer::default())
            .insert_resource(MetricsInterval(self.interval))
            .add_systems(FixedFirst, tick_timer_start)
            .add_systems(FixedLast, tick_timer_end)
            .add_systems(
                Update,
                (refresh_metric_gauges, push_metrics_snapshot)
                    .chain()
                    .run_if(snapshot_due),
            )
            .add_systems(Update, track_frame_time);
    }
}

#[derive(Resource, Default)]
struct TickTimer(Option<Instant>);

#[derive(Resource)]
struct MetricsInterval(Duration);

#[derive(Resource)]
struct MetricsTransport {
    socket: UdpSocket,
    target: SocketAddr,
    last_push: Duration,
}

impl MetricsTransport {
    fn new(port: u16) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind metrics UDP socket");
        socket
            .set_nonblocking(true)
            .expect("failed to set metrics socket non-blocking");

        Self {
            socket,
            target: SocketAddr::from(([127, 0, 0, 1], port)),
            last_push: Duration::ZERO,
        }
    }
}

fn tick_timer_start(mut timer: ResMut<TickTimer>) {
    timer.0 = Some(Instant::now());
}

fn tick_timer_end(timer: Res<TickTimer>, mut metrics: ResMut<ServerMetrics>) {
    if let Some(start) = timer.0 {
        let us = start.elapsed().as_micros() as u64;
        metrics.tick_duration_us = us;
        metrics.tick_duration_max_us = metrics.tick_duration_max_us.max(us);
        if us > 125_000 {
            metrics.tick_overrun_count += 1;
        }
        metrics.tick_count += 1;
    }
}

/// Track total frame time (all schedules combined) as a server load metric.
fn track_frame_time(time: Res<Time>, mut metrics: ResMut<ServerMetrics>) {
    let us = time.delta().as_micros() as u64;
    metrics.frame_duration_us = us;
    metrics.frame_duration_max_us = metrics.frame_duration_max_us.max(us);

    if us > 125_000 {
        metrics.frame_overrun_count += 1;
        warn!("frame took {:.1}ms (budget 125ms)", us as f64 / 1000.0);
    }
}

fn snapshot_due(
    transport: Res<MetricsTransport>,
    interval: Res<MetricsInterval>,
    time: Res<Time>,
) -> bool {
    time.elapsed() - transport.last_push >= interval.0
}

fn refresh_metric_gauges(
    mut metrics: ResMut<ServerMetrics>,
    map: Res<Map>,
    lobby: Res<Lobby>,
    npc_query: Query<(), (With<common_bevy::components::entity_type::EntityType>, Without<common_bevy::components::behaviour::PlayerControlled>)>,
) {
    metrics.loaded_hexes = map.len() as u32;
    metrics.connected_players = lobby.len() as u32;
    metrics.npc_count = npc_query.iter().count() as u32;
    metrics.memory_bytes = process_working_set_bytes();
    metrics.memory_map_bytes = map.heap_size_estimate() as u64;
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

fn push_metrics_snapshot(
    mut metrics: ResMut<ServerMetrics>,
    mut transport: ResMut<MetricsTransport>,
    time: Res<Time>,
) {
    transport.last_push = time.elapsed();

    let mut snapshot = metrics.clone();
    snapshot.snapshot_time_secs = transport.last_push.as_secs_f64();

    let Ok(bytes) =
        bincode::serde::encode_to_vec(&snapshot, bincode::config::legacy())
    else {
        return;
    };

    let mut buf = Vec::with_capacity(6 + bytes.len());
    buf.extend_from_slice(&METRICS_MAGIC);
    buf.extend_from_slice(&METRICS_VERSION.to_le_bytes());
    buf.extend_from_slice(&bytes);

    let _ = transport.socket.send_to(&buf, transport.target);

    metrics.tick_duration_max_us = 0;
    metrics.frame_duration_max_us = 0;
}
