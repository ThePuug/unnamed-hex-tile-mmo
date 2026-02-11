use bevy::prelude::*;
use bevy::picking::Pickable;
use std::collections::HashMap;

use super::DiagnosticsRoot;
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
}

// ============================================================================
// Components
// ============================================================================

/// Marker component for the network diagnostics UI root
#[derive(Component)]
pub struct NetworkUiRootMarker;

#[derive(Component)]
pub(super) struct BandwidthText;

#[derive(Component)]
pub(super) struct MessagesText;

// ============================================================================
// Systems
// ============================================================================

const FONT_SIZE: f32 = 16.0;
const LABEL_COLOR: Color = Color::srgba(0.7, 0.7, 0.7, 1.0);

fn metric_row(label: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(format!("{label}: --")),
        TextFont {
            font_size: FONT_SIZE,
            ..default()
        },
        TextColor(LABEL_COLOR),
    )
}

/// Creates the network diagnostics UI panel as a child of the diagnostics root container
pub fn setup_network_ui(
    mut commands: Commands,
    state: Res<DiagnosticsState>,
    root_q: Query<Entity, With<DiagnosticsRoot>>,
) {
    let root = root_q.single().unwrap();

    let panel = commands
        .spawn((
            NetworkUiRootMarker,
            Pickable::IGNORE,
            Node {
                display: if state.network_ui_visible { Display::Flex } else { Display::None },
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|parent| {
            parent.spawn((BandwidthText, metric_row("Bandwidth")));
            parent.spawn((MessagesText, metric_row("Messages")));
        })
        .id();

    commands.entity(root).add_child(panel);
}

/// Updates the network UI text from NetworkMetrics
pub fn update_network_ui(
    metrics: Res<NetworkMetrics>,
    mut bandwidth_q: Query<&mut Text, (With<BandwidthText>, Without<MessagesText>)>,
    mut messages_q: Query<&mut Text, (With<MessagesText>, Without<BandwidthText>)>,
) {
    if let Ok(mut text) = bandwidth_q.single_mut() {
        let bps = metrics.displayed_bytes_per_sec();
        if bps > 1024.0 {
            **text = format!("Bandwidth: {:.1} KB/s", bps / 1024.0);
        } else {
            **text = format!("Bandwidth: {:.0} B/s", bps);
        }
    }

    if let Ok(mut text) = messages_q.single_mut() {
        **text = format!("Messages: {:.0} msg/s", metrics.displayed_messages_per_sec());
    }
}

/// End-of-frame system to update exponential moving averages
pub fn update_network_metrics(
    mut metrics: ResMut<NetworkMetrics>,
    time: Res<Time>,
) {
    metrics.end_frame(time.delta_secs());
}
