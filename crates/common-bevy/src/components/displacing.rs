use bevy::prelude::*;
use qrz::Qrz;

/// The entity is sliding to `destination` under an ability (a lunge, a
/// knockback). Client only: while present, tile updates short of the
/// destination leave the slide running, and the one that reaches it ends
/// the slide and re-anchors the position there.
#[derive(Component, Clone, Copy, Debug)]
pub struct Displacing {
    /// Standing-height tile the slide ends on.
    pub destination: Qrz,
    pub duration_ms: u16,
}
