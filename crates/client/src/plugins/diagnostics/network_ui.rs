use bevy::prelude::*;

/// EMA weight of a frame's rate: 0.03 averages about a second at 60 fps.
const ALPHA: f32 = 0.03;

/// Tracks network metrics for bandwidth analysis
#[derive(Resource, Debug, Clone, Default)]
pub struct NetworkMetrics {
    /// Smoothed bytes/sec using exponential moving average (updated every frame)
    bytes_per_sec: f32,
    /// Smoothed messages/sec using exponential moving average (updated every frame)
    messages_per_sec: f32,
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
}

impl NetworkMetrics {
    /// Record a received message
    pub fn record_received(&mut self, _message_type: &'static str, bytes: usize) {
        self.frame_bytes += bytes;
        self.frame_messages += 1;
    }

    /// Call this at the end of each frame to update exponential moving averages
    pub fn end_frame(&mut self, delta_time: f32) {
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

        self.bytes_per_sec = ALPHA * frame_bytes_per_sec + (1.0 - ALPHA) * self.bytes_per_sec;
        self.messages_per_sec = ALPHA * frame_messages_per_sec + (1.0 - ALPHA) * self.messages_per_sec;

        self.time_since_display_update += delta_time;
        if self.time_since_display_update >= 1.0 {
            self.displayed_bytes_per_sec = self.bytes_per_sec;
            self.displayed_messages_per_sec = self.messages_per_sec;
            self.time_since_display_update = 0.0;
        }

        self.frame_bytes = 0;
        self.frame_messages = 0;
    }

    #[cfg(feature = "admin")]
    pub fn displayed_bytes_per_sec(&self) -> f32 {
        self.displayed_bytes_per_sec
    }

    #[cfg(feature = "admin")]
    pub fn displayed_messages_per_sec(&self) -> f32 {
        self.displayed_messages_per_sec
    }
}

/// End-of-frame system to update exponential moving averages
pub fn update_network_metrics(
    mut metrics: ResMut<NetworkMetrics>,
    time: Res<Time>,
) {
    metrics.end_frame(time.delta_secs());
}
