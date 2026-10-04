//! Dens as the server tells of them: each stood with the cover of every
//! level that draws one (`cover::place_dens`), and its pieces' circles
//! stood on the map, so prediction goes round them as the server does.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use common::den::Piece;
use common_bevy::{
    den::DenLook,
    message::{Do, Event},
    resources::map::Map,
    summary::{mesh_region_lattice, summary_lattice, LOD_LEVELS},
    summary_mesh::MeshRegionKey,
};
use qrz::{Convert, Qrz};

use crate::resources::SummaryMeshes;

/// The levels a den is drawn at: the tiles, the first summary level, and
/// the next, where it stands past the tiles.
pub const DRAWN_AT: [u32; 3] = [LOD_LEVELS[0], LOD_LEVELS[1], LOD_LEVELS[2]];

/// Every den the server has told of, by the tile its pack stands on. A
/// snapshot goes to each region built, so a build reads the dens as they
/// were when it was dispatched.
#[derive(Resource, Default, Clone)]
pub struct Dens(pub Arc<HashMap<Qrz, DenLook>>);

/// Each piece of the den on `at` as `look` draws it: the piece, where its
/// origin stands in the world along the ground, and the tile it stands in.
pub fn standing(at: Qrz, look: DenLook, map: &Map) -> impl Iterator<Item = (&'static Piece, Vec2, (i32, i32))> + '_ {
    let centre = map.convert(Qrz { z: 0, ..at }).xz();
    let yaw = look.yaw;
    look.pieces().iter().map(move |piece| {
        let origin = centre + Vec2::from(piece.placed(yaw));
        let tile = map.convert(Vec3::new(origin.x, 0.0, origin.y));
        (piece, origin, (tile.q, tile.r))
    })
}

/// The mesh region of level `radius` a piece standing in tile `(q, r)` is
/// drawn with.
pub fn region_of(radius: u32, (q, r): (i32, i32)) -> MeshRegionKey {
    let cell = summary_lattice(radius).cell_id(q, r);
    let (mn, mm) = mesh_region_lattice().cell_id(cell.0, cell.1);
    MeshRegionKey { r: radius, mn, mm }
}

/// Takes each den the server tells of: keeps it, stands its circles on the
/// map, and has every region its pieces stood in or now stand in built
/// again at each level a den is drawn at, as a change to the map does.
pub fn apply(mut reader: MessageReader<Do>, map: Res<Map>, mut dens: ResMut<Dens>, mut meshes: ResMut<SummaryMeshes>) {
    for message in reader.read() {
        let Do { event: Event::Den { at, den, .. } } = message else { continue };
        let (at, den) = (*at, *den);
        let was = dens.0.get(&at).copied();
        if was == den {
            continue;
        }
        let mut next = (*dens.0).clone();
        match den {
            Some(look) => next.insert(at, look),
            None => next.remove(&at),
        };
        dens.0 = Arc::new(next);
        info!("den at ({}, {}): {den:?}", at.q, at.r);
        map.set_solids((at.q, at.r), &den.map_or(Vec::new(), |look| look.circles()));
        for look in [was, den].iter().flatten() {
            for (_, _, tile) in standing(at, *look, &map) {
                for radius in DRAWN_AT {
                    if let Some(state) = meshes.states.get_mut(&region_of(radius, tile)) {
                        state.stale = true;
                    }
                }
            }
        }
        map.force_changed();
    }
}
