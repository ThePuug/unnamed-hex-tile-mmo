//! How the stack's memory grows with the tiles read: rows of tiles about
//! the spawn, read in steps, the process's private bytes after each.
//!
//! Run: cargo test -p world --release --test memory_probe -- --ignored --nocapture

use world::events::Composite;

const SEED: u64 = 0x9E3779B97F4A7C15;
const WIDTH: i32 = 1000;
const LIMIT: u64 = 8 << 30;

#[cfg(windows)]
fn private_bytes() -> u64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_non_paged: usize,
        quota_non_paged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.pagefile as u64
}

#[cfg(not(windows))]
fn private_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap();
    s.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() * 4096
}

#[test]
#[ignore]
fn memory_grows_with_cells_not_tiles() {
    let c = Composite::standard(SEED);
    let (q0, r0) = (104_289 - WIDTH / 2, -4_677);
    c.tile_at(q0, r0);
    let base = private_bytes();
    println!("after the first tile: {} MB", base >> 20);
    let mut row = 0;
    let mut last = (0u64, base);
    for rows in [200, 400, 800] {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as i32;
        std::thread::scope(|s| {
            for t in 0..threads {
                let c = &c;
                s.spawn(move || {
                    let mut k = row + t;
                    while k < rows {
                        for dq in 0..WIDTH {
                            c.tile_at(q0 + dq, r0 + k);
                        }
                        k += threads;
                    }
                });
            }
        });
        row = rows;
        let tiles = (row * WIDTH) as u64;
        let now = private_bytes();
        let per = (now as f64 - last.1 as f64) / (tiles - last.0) as f64;
        let m = c.drain_metrics();
        let cells: Vec<String> = m.layers.iter().map(|l| format!("{}={}", l.name, l.indexed)).collect();
        println!(
            "{tiles:>7} tiles: {:>6} MB, {per:>7.0} B per tile this step; cached tiles {}; cells {}",
            now >> 20,
            m.visible,
            cells.join(" ")
        );
        last = (tiles, now);
        assert!(now < LIMIT, "past 8 GB");
    }
}
