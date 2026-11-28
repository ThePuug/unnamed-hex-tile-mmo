//! What the bodies actors are drawn with declare that the game reads as
//! code, by the stem of each body's rig under `actors/`.

// How high the water stands over each body's root as its swim and tread
// float it, read from them at build time (`build.rs`). The root is the
// floating body's lowest point, so water this deep holds it clear of the
// bed.
include!(concat!(env!("OUT_DIR"), "/waterlines.rs"));

/// The waterline the body `stem` floats at, or None for a body whose asset
/// declares no swim: it walks the bed at any depth.
pub fn waterline(stem: &str) -> Option<f32> {
    WATERLINES.iter().find(|(body, _)| *body == stem).map(|&(_, line)| line)
}
