use std::time::Duration;

use bevy::prelude::*;
use qrz::Qrz;

/// The entity is sliding to `destination` under an ability (a lunge, a
/// knockback, a flank's circle). Client only: while present nothing else
/// moves what is drawn, the tile that reaches the destination re-anchors
/// the position there, and the slide ends when its time is up — the server
/// sends that tile with the slide, so it cannot be what ends it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Displacing {
    /// Standing-height tile the slide ends on.
    pub destination: Qrz,
    pub duration_ms: u16,
    /// The client's elapsed time the slide ends at.
    pub ends_at: Duration,
}
