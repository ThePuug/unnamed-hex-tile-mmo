use bevy::prelude::*;
use bevy::ecs::system::lifetimeless::SRes;
use iyes_perf_ui::prelude::*;
use iyes_perf_ui::entry::PerfUiEntry;
use iyes_perf_ui::utils::next_sort_key;
use std::collections::HashMap;

use super::config::DiagnosticsState;

// ============================================================================
// Resources
// ============================================================================

/// Tracks network metrics by message type for bandwidth analysis
#[derive(Resource, Debug, Clone)]
pub struct NetworkMetrics {
    /// Total bytes received this frame by message type
    pub bytes_received_per_type: HashMap<String, usize>,
    /// Total messages received this frame by message type
    pub messages_received_per_type: HashMap<String, usize>,
    /// Smoothed bytes/sec using exponential moving average (updated every frame)
    pub bytes_per_sec: f32,
    /// Smoothed messages/sec using exponential moving average (updated every frame)
    pub messages_per_sec: f32,
    /// Displayed bytes/sec (only updated once per second for readable UI)
    displayed_bytes_per_sec: f32,
    /// Displayed messages/sec (only updated once per second for readable UI)
    displayed_messages_per_sec: f32,
    /// Current frame's total bytes (before smoothing)
    frame_bytes: usize,
    /// Current frame's total messages (before smoothing)
    frame_messages: usize,
    /// Time since last display update
    time_since_display_update: f32,
    /// EMA alpha for smoothing (lower = smoother, 0.03 = ~1 second average at 60fps)
    alpha: f32,
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self {
            bytes_received_per_type: HashMap::new(),
            messages_received_per_type: HashMap::new(),
            bytes_per_sec: 0.0,
            messages_per_sec: 0.0,
            displayed_bytes_per_sec: 0.0,
            displayed_messages_per_sec: 0.0,
            frame_bytes: 0,
            frame_messages: 0,
            time_since_display_update: 0.0,
            alpha: 0.03, // ~1 second smoothing window for stable, readable metrics
        }
    }
}

impl NetworkMetrics {
    /// Record a received message
    pub fn record_received(&mut self, message_type: String, bytes: usize) {
        *self.bytes_received_per_type.entry(message_type.clone()).or_insert(0) += bytes;
        *self.messages_received_per_type.entry(message_type).or_insert(0) += 1;
        self.frame_bytes += bytes;
        self.frame_messages += 1;
    }

    /// Call this at the end of each frame to update exponential moving averages
    /// Uses EMA: new_avg = alpha * current + (1 - alpha) * old_avg
    /// Display values are only updated once per second for readability
    pub fn end_frame(&mut self, delta_time: f32) {
        // Convert frame totals to per-second rates
        let frame_bytes_per_sec = if delta_time > 0.0 {
            self.frame_bytes as f32 / delta_time
        } else {
            0.0
        };
        let frame_messages_per_sec = if delta_time > 0.0 {
            self.frame_messages as f32 / delta_time
        } else {
            0.0
        };

        // Apply exponential moving average (every frame)
        self.bytes_per_sec = self.alpha * frame_bytes_per_sec + (1.0 - self.alpha) * self.bytes_per_sec;
        self.messages_per_sec = self.alpha * frame_messages_per_sec + (1.0 - self.alpha) * self.messages_per_sec;

        // Update displayed values only once per second for readable UI
        self.time_since_display_update += delta_time;
        if self.time_since_display_update >= 1.0 {
            self.displayed_bytes_per_sec = self.bytes_per_sec;
            self.displayed_messages_per_sec = self.messages_per_sec;
            self.time_since_display_update = 0.0;
        }

        // Clear per-frame counters
        self.frame_bytes = 0;
        self.frame_messages = 0;
        self.bytes_received_per_type.clear();
        self.messages_received_per_type.clear();
    }

    /// Get the displayed bytes/sec (updated once per second for readable UI)
    pub fn displayed_bytes_per_sec(&self) -> f32 {
        self.displayed_bytes_per_sec
    }

    /// Get the displayed messages/sec (updated once per second for readable UI)
    pub fn displayed_messages_per_sec(&self) -> f32 {
        self.displayed_messages_per_sec
    }

    /// Get breakdown of bandwidth by message type (sorted by bytes, descending)
    /// Note: Currently returns current frame data; could be enhanced to track history per type
    pub fn get_breakdown(&self) -> Vec<(String, f32)> {
        let mut breakdown: Vec<(String, f32)> = self.bytes_received_per_type
            .iter()
            .map(|(msg_type, bytes)| (msg_type.clone(), *bytes as f32))
            .collect();

        breakdown.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        breakdown
    }
}

// ============================================================================
// Components
// ============================================================================

/// Marker component for the network diagnostics UI root
#[derive(Component)]
pub struct NetworkUiRootMarker;

/// Custom perf UI entry for total network bandwidth
#[derive(Component, Debug, Clone)]
#[require(PerfUiRoot)]
pub struct PerfUiNetworkBandwidth {
    pub label: String,
    pub sort_key: i32,
}

impl Default for PerfUiNetworkBandwidth {
    fn default() -> Self {
        Self {
            label: String::from("Network (bytes/s)"),
            sort_key: next_sort_key(),
        }
    }
}

impl PerfUiEntry for PerfUiNetworkBandwidth {
    type SystemParam = SRes<NetworkMetrics>;
    type Value = f32;

    fn label(&self) -> &str {
        if self.label.is_empty() {
            "Network (bytes/s)"
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
        Some(param.displayed_bytes_per_sec())
    }

    fn format_value(&self, value: &Self::Value) -> String {
        if *value > 1024.0 {
            format!("{:.1} KB/s", value / 1024.0)
        } else {
            format!("{:.0} B/s", value)
        }
    }
}

/// Custom perf UI entry for message rate
#[derive(Component, Debug, Clone)]
#[require(PerfUiRoot)]
pub struct PerfUiNetworkMessages {
    pub label: String,
    pub sort_key: i32,
}

impl Default for PerfUiNetworkMessages {
    fn default() -> Self {
        Self {
            label: String::from("Network (msg/s)"),
            sort_key: next_sort_key(),
        }
    }
}

impl PerfUiEntry for PerfUiNetworkMessages {
    type SystemParam = SRes<NetworkMetrics>;
    type Value = f32;

    fn label(&self) -> &str {
        if self.label.is_empty() {
            "Network (msg/s)"
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
        Some(param.displayed_messages_per_sec())
    }

    fn format_value(&self, value: &Self::Value) -> String {
        format!("{:.0} msg/s", value)
    }
}

// ============================================================================
// Systems
// ============================================================================

/// Creates the network diagnostics UI on startup
pub fn setup_network_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
) {
    commands.spawn((
        NetworkUiRootMarker,
        PerfUiRoot::default(),
        PerfUiNetworkBandwidth::default(),
        PerfUiNetworkMessages::default(),
        if state.network_ui_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    ));
}

/// End-of-frame system to update exponential moving averages
pub fn update_network_metrics(
    mut metrics: ResMut<NetworkMetrics>,
    time: Res<Time>,
) {
    metrics.end_frame(time.delta_secs());
}
