//! The real ground for the hex-ladder POC: every cell of every ladder level
//! around the haven, read through the field and range layers and lithology.
//! For each cell it writes the height at the cell's centre, the mean of the
//! centre and the six midpoints of its edges, and the erodibility of the
//! rock at its centre, as little-endian f32 grids the POC reads.
//!
//! A level's cells sit on the same axial lattice as the world's tiles: cell
//! (i, j) of a level `w` tiles wide is centred on the tile `HAVEN + (i, j)·w`,
//! so the haven is a centre at every level.
//!
//! Run: `LADDER_OUT=<dir> cargo test --release -p world --test ladder_probe -- --ignored --nocapture`

use std::io::Write;
use std::time::Instant;

use world::events::{lithology, motion, plates, thickening, thrusting, tilt, Composite};
use world::WORLD_SEED;

/// The haven's tile, as `common_bevy::haven::HAVEN_LOCATION` holds it.
const HAVEN: (i32, i32) = (107289, -7677);

/// Each level's width in tiles and how far its cells reach from the haven,
/// in tiles: wide enough that a request for the level's own region, and
/// everything the finer levels' requests read of it, stays inside.
const LEVELS: [(i32, i32); 7] = [
    (729, 142_884),
    (243, 57_591),
    (81, 19_197),
    (27, 6_399),
    (9, 2_133),
    (3, 832),
    (1, 481),
];

/// The field and range layers and lithology: the stack below the scarps,
/// which the ladder takes over, and below what stands on the finished tile.
fn ground(seed: u64) -> Composite {
    let mut c = Composite::new(seed);
    c.add_event(Box::new(plates::PlateEvent::new()));
    c.add_event(Box::new(tilt::TiltEvent::new()));
    c.add_event(Box::new(motion::MotionEvent::new()));
    c.add_event(Box::new(thrusting::ThrustingEvent::new()));
    c.add_event(Box::new(thickening::ThickeningEvent::new()));
    c.add_event(Box::new(lithology::LithologyEvent::new()));
    c
}

/// A cell's centre and the midpoints of its six edges, as tile offsets.
fn samples(w: i32) -> Vec<(i32, i32)> {
    if w == 1 {
        return vec![(0, 0)];
    }
    let h = w / 2;
    vec![(0, 0), (h, 0), (-h, 0), (0, h), (0, -h), (h, -h), (-h, h)]
}

#[test]
#[ignore]
fn export_ladder_ground() {
    let out = std::env::var("LADDER_OUT").expect("LADDER_OUT names the directory to write to");
    std::fs::create_dir_all(&out).expect("the output directory");
    let c = ground(WORLD_SEED);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut meta = String::from("w lo n\n");
    for &(w, reach) in &LEVELS {
        let started = Instant::now();
        let half = reach / w + 2;
        let n = 2 * half + 1;
        let offs = samples(w);
        let rows: Vec<i32> = (-half..=half).collect();
        let chunk = rows.len().div_ceil(threads);
        let parts: Vec<Vec<[f32; 3]>> = std::thread::scope(|s| {
            let handles: Vec<_> = rows
                .chunks(chunk)
                .map(|part| {
                    let (c, offs) = (&c, &offs);
                    s.spawn(move || {
                        let mut v = Vec::with_capacity(part.len() * n as usize);
                        for &i in part {
                            for j in -half..=half {
                                let (q, r) = (HAVEN.0 + i * w, HAVEN.1 + j * w);
                                let centre = c.tile_at(q, r);
                                let mut sum = 0.0;
                                for &(dq, dr) in offs.iter() {
                                    sum += if (dq, dr) == (0, 0) { centre.elevation } else { c.tile_at(q + dq, r + dr).elevation };
                                }
                                let erodibility = centre.rock.map_or(f64::NAN, |rock| rock.erodibility());
                                v.push([centre.elevation as f32, (sum / offs.len() as f64) as f32, erodibility as f32]);
                            }
                        }
                        v
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("a reading thread")).collect()
        });
        let mut file = std::fs::File::create(format!("{out}/level_{w}.f32")).expect("a level's file");
        for cell in parts.iter().flatten() {
            for x in cell {
                file.write_all(&x.to_le_bytes()).expect("written");
            }
        }
        meta.push_str(&format!("{w} {} {n}\n", -half));
        println!("level {w}: {n}x{n} cells, {} samples each, {:.1}s", offs.len(), started.elapsed().as_secs_f64());
    }
    std::fs::write(format!("{out}/levels.txt"), meta).expect("the levels' extents");
}
