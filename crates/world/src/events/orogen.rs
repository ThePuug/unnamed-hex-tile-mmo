//! OrogenEvent — crustal thickening where plates converge.
//!
//! A belt is not a feature placed on a carrier. It is the neighbourhood of a
//! ridge line of the shortening field, and every quantity it needs is answered
//! per position in constant time: [`orogen_field`] resolves the crest with one
//! closed-form expression in the field's own first and second derivatives, then
//! lays an asymmetric wedge across it.
//!
//! # A field, not a feature
//!
//! Empty `deform`, no index, `max_influence` of zero, and a `query` that reads
//! the layer beneath it and returns one number. Nothing originates anywhere and
//! nothing has extent, so there is no ownership to coordinate, no containment
//! to prove and no cell scale to derive — [`OROGEN_CELL_SCALE`] is tile-cache
//! granularity and nothing else.
//!
//! The layer this replaced placed swaths for boundary segments and carried an
//! index of them. The field says the same thing without the bookkeeping: a belt
//! stands where the shortening field has a ridge, which is what a convergent
//! boundary *is*.
//!
//! # What it claims
//!
//! Three things, and nothing else. Convergent shortening thickens crust on
//! ridge lines of the shortening field; thickened crust floats higher, which is
//! the coupling between the ceiling, the half-width and the flank angle; and
//! the wedge is asymmetric because one plate underthrusts the other, so the
//! short steep flank faces vergence.
//!
//! No dissection, no valleys, no peaks. Summits are erosional residuals left by
//! a later layer cutting into this mass — a belt with no valleys is supposed to
//! read as a smooth swath at this stage.

use std::any::Any;

use crate::hex_to_world;
use crate::orogen_field::relief_on;
use super::index::IndexRegistry;
use super::{CellScope, TileOutput, TileView, WorldEvent};

/// Matched to the layers either side of it, so this one shares their cell
/// boundaries and warms with them.
///
/// Scale is pure cache granularity here — `deform` is empty and there is no
/// index — but it is not free: a layer's scale dilates every layer beneath it,
/// and choosing a coarser one would deform more plate cells for no gain.
pub const OROGEN_CELL_SCALE: u32 = 1800;

pub struct OrogenEvent;

impl OrogenEvent {
    pub fn new() -> Self { OrogenEvent }
}

impl Default for OrogenEvent {
    fn default() -> Self { Self::new() }
}

impl WorldEvent for OrogenEvent {
    fn name(&self) -> &str { "orogen" }
    fn scale(&self) -> u32 { OROGEN_CELL_SCALE }

    /// Nothing originates anywhere, so nothing reaches.
    fn max_influence(&self) -> u32 { 0 }

    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Nothing to place. A belt has no origin and no extent — it is a function
    /// of position, and the only thing it reads is the ground directly beneath
    /// the tile being asked about.
    fn deform(&self, _scope: &CellScope) {}

    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        _cell: &(dyn Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let (wx, wy) = hex_to_world(q, r);

        // `below.elevation` decides how much of the thickening reaches the
        // surface: oceanic crust is thin and dense, so a belt built on it floats
        // lower. Reading it here rather than recomputing the substrate is what
        // keeps this layer honest about what it stands on.
        let rise = relief_on(wx, wy, below.elevation, seed);
        if rise <= 0.0 { return None }

        Some(TileOutput { elevation_delta: rise, ..TileOutput::default() })
    }
}
