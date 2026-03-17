use bevy::prelude::*;
use std::collections::HashMap;

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
            alpha: 0.03,
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

        self.bytes_per_sec = self.alpha * frame_bytes_per_sec + (1.0 - self.alpha) * self.bytes_per_sec;
        self.messages_per_sec = self.alpha * frame_messages_per_sec + (1.0 - self.alpha) * self.messages_per_sec;

        self.time_since_display_update += delta_time;
        if self.time_since_display_update >= 1.0 {
            self.displayed_bytes_per_sec = self.bytes_per_sec;
            self.displayed_messages_per_sec = self.messages_per_sec;
            self.time_since_display_update = 0.0;
        }

        self.frame_bytes = 0;
        self.frame_messages = 0;
        self.bytes_received_per_type.clear();
        self.messages_received_per_type.clear();
    }

    pub fn displayed_bytes_per_sec(&self) -> f32 {
        self.displayed_bytes_per_sec
    }

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
