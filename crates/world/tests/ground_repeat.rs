//! The base layers read alike at a tile whatever order its neighbourhood is
//! read in and whichever process reads it: each layer's own output, exactly,
//! at the samples of cells that once read differently between two runs.

use std::process::Command;

use world::events::Composite;
use world::WORLD_SEED;

/// Cells (w, i, j) two runs of the ladder's gate once read differently.
const CELLS: [(i64, i64, i64); 8] = [(729, -49, 145), (729, -48, 141), (729, -48, 142), (243, 451, -1), (243, 453, -3), (81, 1338, -4), (81, 1340, -8), (27, 4105, -68)];

/// The ladder's samples of each cell: its centre and the midpoints of its
/// six edges.
fn tiles() -> Vec<(i32, i32)> {
    CELLS
        .iter()
        .flat_map(|&(w, i, j)| {
            let h = w / 2;
            [(0, 0), (h, 0), (-h, 0), (0, h), (0, -h), (h, -h), (-h, h)].map(move |(dq, dr)| ((i * w + dq) as i32, (j * w + dr) as i32))
        })
        .collect()
}

/// Each tile's height and every layer's own output there, exactly.
fn read(base: &Composite, tiles: &[(i32, i32)]) -> Vec<String> {
    tiles
        .iter()
        .map(|&(q, r)| {
            let (view, outputs) = base.outputs_at(q, r);
            let layers: Vec<String> = outputs.iter().map(|(name, o)| format!("{name}={:?}", o.as_ref().map(|o| o.elevation_delta))).collect();
            format!("sample ({q}, {r}) elevation={:?} {}", view.elevation, layers.join(" "))
        })
        .collect()
}

#[test]
fn the_base_layers_read_alike_in_any_order_and_any_process() {
    let tiles = tiles();
    let forward = read(&Composite::base(WORLD_SEED), &tiles);
    if std::env::var("GROUND_REPEAT_CHILD").is_ok() {
        forward.iter().for_each(|line| println!("{line}"));
        return;
    }
    let mut reversed = tiles.clone();
    reversed.reverse();
    let mut backward = read(&Composite::base(WORLD_SEED), &reversed);
    backward.reverse();
    assert_eq!(forward, backward, "a tile reads differently as its neighbourhood is read in another order");
    let child = || {
        let out = Command::new(std::env::current_exe().expect("this test's binary"))
            .args(["--exact", "the_base_layers_read_alike_in_any_order_and_any_process", "--nocapture", "--test-threads", "1"])
            .env("GROUND_REPEAT_CHILD", "1")
            .output()
            .expect("a child process");
        // The harness may print its own words ahead of the first line.
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.find("sample (").map(|at| l[at..].to_string())).collect::<Vec<_>>()
    };
    let (one, two) = (child(), child());
    assert_eq!(one.len(), tiles.len(), "a child read every sample");
    assert_eq!(one, two, "two processes read a tile differently");
    assert_eq!(one, forward, "another process reads a tile differently from this one");
}
