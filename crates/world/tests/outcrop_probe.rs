//! A census of the outcrops about the haven: of the sites on the lattice
//! there, how many show on land, how many tiles of rock each stands, and
//! how many of those no one walks into, by the rock they stand on.
//!
//! Run: `cargo test --release -p world --test outcrop_probe -- --ignored --nocapture`

use common::Rock;
use world::events::outcrop::{site_lattice, site_of};
use world::events::Composite;
use world::world_to_hex;

const SEED: u64 = 0x9E3779B97F4A7C15;

#[test]
#[ignore]
fn census() {
    let c = Composite::standard(SEED);
    let lattice = site_lattice();
    let home = lattice.cell_id(104_289, -4_677);
    let (mut sites, mut on_land, mut showing) = (0, 0, 0);
    let (mut rock_tiles, mut closed_tiles) = (0, 0);
    let mut by_rock = [(0usize, 0usize); 4];
    for id in lattice.cells_within_distance(home, 6) {
        sites += 1;
        let s = site_of(&lattice, id, SEED);
        let (q0, r0) = world_to_hex(s.x, s.y);
        let centre = c.tile_at(q0, r0);
        if centre.water.is_some() || centre.elevation <= 0.0 {
            continue;
        }
        on_land += 1;
        let i = match centre.rock { Some(Rock::Shale) => 0, Some(Rock::Sandstone) => 1, Some(Rock::Limestone) => 2, _ => 3 };
        by_rock[i].0 += 1;
        let reach = s.radius.ceil() as i32 + 1;
        let (mut rocks, mut closed) = (0, 0);
        for dq in -reach..=reach {
            for dr in -reach..=reach {
                let v = c.tile_at(q0 + dq, r0 + dr);
                if v.cover.boulders().count() > 0 {
                    rocks += 1;
                    if v.cover.fullness() >= 4 {
                        closed += 1;
                    }
                }
            }
        }
        if rocks > 0 {
            showing += 1;
            by_rock[i].1 += 1;
            rock_tiles += rocks;
            closed_tiles += closed;
        }
        println!("site {id:?} at ({q0}, {r0}): {:?}, grade {:.2}, z {:.0}: {rocks} rock tiles, {closed} closed", centre.rock, centre.grade(), centre.elevation);
    }
    println!("{sites} sites, {on_land} on land, {showing} showing; per showing site {:.1} rock tiles, {:.1} closed", rock_tiles as f64 / showing.max(1) as f64, closed_tiles as f64 / showing.max(1) as f64);
    for (i, rock) in ["shale", "sandstone", "limestone", "basement"].iter().enumerate() {
        println!("  {rock}: {} on land, {} showing", by_rock[i].0, by_rock[i].1);
    }
}
