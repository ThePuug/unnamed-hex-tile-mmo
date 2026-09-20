//! How much the server's summary dispatch enumerates per pass at the reach.
use common_bevy::summary::{compute_active_bands, mesh_region_lattice, reach_wu};
use common_bevy::summary_mesh::visible_lod_regions;
use std::time::Instant;

fn main() {
    let bands = compute_active_bands(reach_wu());
    let t = Instant::now();
    let regions = visible_lod_regions(&bands, 1234.5, -987.0, common_bevy::chunk::FIXED_STREAM_APOTHEM_WU);
    let enumerate = t.elapsed();
    let lat = mesh_region_lattice();
    let t = Instant::now();
    let mut summaries = 0usize;
    let mut per_level = std::collections::BTreeMap::new();
    for rk in &regions {
        *per_level.entry(rk.r).or_insert(0usize) += 1;
        summaries += lat.tiles_in_cell((rk.mn, rk.mm)).count();
    }
    let walk = t.elapsed();
    println!("reach {:.0} WU, bands {:?}", reach_wu(), bands.iter().map(|b| (b.r, b.outer_wu as i32)).collect::<Vec<_>>());
    println!("regions {} {:?}, summaries {}, enumerate {:?}, walk {:?}", regions.len(), per_level, summaries, enumerate, walk);
}
