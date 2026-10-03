//! What the den layer costs the tile path: a dense region and a sparse
//! sample read through the served stack, and through the same stack
//! without the den layer. Run with
//! `cargo test -p world --release --test den_cost_probe -- --ignored --nocapture`.

use std::time::Instant;

use world::events::{den, dissection, drainage, forest, lithology, migration, motion, outcrop, plates, thickening, thrusting, tilt, Composite};
use world::WORLD_SEED;

const HAVEN: (i32, i32) = (104289, -4677);

fn without_dens() -> Composite {
    let mut composite = Composite::new(WORLD_SEED);
    composite.add_event(Box::new(plates::PlateEvent::new()));
    composite.add_event(Box::new(tilt::TiltEvent::new()));
    composite.add_event(Box::new(motion::MotionEvent::new()));
    composite.add_event(Box::new(thrusting::ThrustingEvent::new()));
    composite.add_event(Box::new(thickening::ThickeningEvent::new()));
    composite.add_event(Box::new(lithology::LithologyEvent::new()));
    composite.add_event(Box::new(drainage::DrainageEvent::new()));
    composite.add_event(Box::new(migration::MigrationEvent::new()));
    composite.add_event(Box::new(dissection::DissectionEvent::new()));
    composite.add_event(Box::new(outcrop::OutcropEvent::new()));
    composite.add_event(Box::new(forest::ForestEvent::new()));
    composite
}

fn with_dens() -> Composite {
    let mut composite = without_dens();
    composite.add_event(Box::new(den::DenEvent::new()));
    composite
}

fn time(label: &str, composite: &Composite, tiles: &[(i32, i32)]) {
    let started = Instant::now();
    for &(q, r) in tiles {
        std::hint::black_box(composite.tile_at(q, r));
    }
    println!("{label}: {} tiles in {:.2}s", tiles.len(), started.elapsed().as_secs_f64());
}

#[test]
#[ignore]
fn the_den_layers_cost_on_the_tile_path() {
    let dense: Vec<(i32, i32)> = (-50..50).flat_map(|dq| (-50..50).map(move |dr| (HAVEN.0 + dq, HAVEN.1 + dr))).collect();
    let sparse: Vec<(i32, i32)> = (-20..20).flat_map(|i| (-20..20).map(move |j| (HAVEN.0 + i * 200, HAVEN.1 + j * 200))).collect();
    time("dense, without dens", &without_dens(), &dense);
    time("dense, with dens", &with_dens(), &dense);
    time("sparse, without dens", &without_dens(), &sparse);
    time("sparse, with dens", &with_dens(), &sparse);
}
