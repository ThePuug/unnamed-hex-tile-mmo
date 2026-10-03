//! What the served world's den sites are around the haven: how many of each
//! habitat, each read off its site's tile, and how long that takes. Run with
//! `cargo test -p world --release --test den_probe -- --ignored --nocapture`.

use std::collections::HashMap;
use std::time::Instant;

use common::den::Habitat;
use world::events::den::{den_lattice, site_of};
use world::events::Composite;
use world::WORLD_SEED;

/// The haven's tile, as `common_bevy::haven::HAVEN_LOCATION` holds it.
const HAVEN: (i32, i32) = (104289, -4677);

#[test]
#[ignore]
fn habitats_around_the_haven() {
    let composite = Composite::standard(WORLD_SEED);
    let lattice = den_lattice();
    let home = lattice.cell_id(HAVEN.0, HAVEN.1);
    let started = Instant::now();
    let mut counts: HashMap<Habitat, usize> = HashMap::new();
    let mut sites = 0;
    for cell in lattice.cells_within_distance(home, 12) {
        let (q, r) = site_of(&lattice, cell, WORLD_SEED);
        sites += 1;
        if let Some(habitat) = composite.tile_at(q, r).den {
            *counts.entry(habitat).or_default() += 1;
        }
    }
    let total: usize = counts.values().sum();
    println!("{total} dens of {sites} sites within 12 cells of the haven, in {:.1}s", started.elapsed().as_secs_f64());
    for habitat in Habitat::ALL {
        let n = counts.get(&habitat).copied().unwrap_or(0);
        println!("{habitat:?}: {n} ({:.0}%)", 100.0 * n as f64 / total.max(1) as f64);
    }
    assert!(total > 0);
}
