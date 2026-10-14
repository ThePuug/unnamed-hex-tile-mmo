//! A level kept on disk: the top, built once per site and read back while
//! its inputs stand. A file is named by the hash of everything the top is
//! made from: the seed, the site, its bounds, the parameters, and the
//! ground's own values at a spread of the top's cells, so a change to any
//! layer beneath it reads as a different top.

use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::PathBuf;

use super::paged::{Paged, PAGE};
use super::{Bounds, Ground, Level, Params};

/// Raised whenever what a kept level holds changes shape.
const FORMAT: u32 = 1;

/// A value a level keeps, as bytes.
pub trait Bin: Sized + Clone {
    fn put(&self, out: &mut Vec<u8>);
    fn take(inp: &mut &[u8]) -> Option<Self>;
}

fn bytes<const N: usize>(inp: &mut &[u8]) -> Option<[u8; N]> {
    let (head, rest) = inp.split_at_checked(N)?;
    *inp = rest;
    head.try_into().ok()
}

impl Bin for f64 {
    fn put(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn take(inp: &mut &[u8]) -> Option<Self> {
        Some(f64::from_le_bytes(bytes(inp)?))
    }
}

impl Bin for i64 {
    fn put(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_le_bytes());
    }
    fn take(inp: &mut &[u8]) -> Option<Self> {
        Some(i64::from_le_bytes(bytes(inp)?))
    }
}

impl Bin for bool {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
    fn take(inp: &mut &[u8]) -> Option<Self> {
        Some(bytes::<1>(inp)?[0] != 0)
    }
}

impl<const N: usize> Bin for [f64; N] {
    fn put(&self, out: &mut Vec<u8>) {
        self.iter().for_each(|x| x.put(out));
    }
    fn take(inp: &mut &[u8]) -> Option<Self> {
        let mut v = [0.0; N];
        for x in &mut v {
            *x = f64::take(inp)?;
        }
        Some(v)
    }
}

fn put_paged<T: Bin>(out: &mut Vec<u8>, p: &Paged<T>) {
    p.default_value().put(out);
    let pages: Vec<usize> = p.written().collect();
    (pages.len() as i64).put(out);
    for first in pages {
        (first as i64).put(out);
        p.page(first).expect("a written page").iter().for_each(|x| x.put(out));
    }
}

fn take_paged<T: Bin>(inp: &mut &[u8], len: usize) -> Option<Paged<T>> {
    let mut p = Paged::new(T::take(inp)?, len);
    for _ in 0..i64::take(inp)? {
        let first = i64::take(inp)? as usize;
        let values: Option<Vec<T>> = (0..PAGE).map(|_| T::take(inp)).collect();
        p.set_page(first, values?.into_boxed_slice());
    }
    Some(p)
}

/// Every field a level keeps, each passed to `$m`.
macro_rules! fields {
    ($m:ident, $lv:expr) => {
        $m!($lv.erod, 1);
        $m!($lv.age, 1);
        $m!($lv.p, 1);
        $m!($lv.ground, 1);
        $m!($lv.wpred, 1);
        $m!($lv.cut, 1);
        $m!($lv.wfull, 1);
        $m!($lv.wdist, 1);
        $m!($lv.wborn, 1);
        $m!($lv.wgov, 1);
        $m!($lv.wfade, 1);
        $m!($lv.water, 1);
        $m!($lv.wdepth, 1);
        $m!($lv.z, 1);
        $m!($lv.zd, 1);
        $m!($lv.a3, 1);
        $m!($lv.b3, 1);
        $m!($lv.cc, 1);
        $m!($lv.elev, 3);
        $m!($lv.psurf, 3);
        $m!($lv.gelev, 3);
        $m!($lv.uelev, 3);
        $m!($lv.base, 3);
        $m!($lv.born, 3);
        $m!($lv.rec, 3);
        $m!($lv.trough, 3);
        $m!($lv.acc, 3);
        $m!($lv.floor, 3);
        $m!($lv.line_on, 3);
        $m!($lv.line_rec, 3);
        $m!($lv.line_acc, 3);
        $m!($lv.line_fl, 3);
        $m!($lv.line_base, 3);
        $m!($lv.line_born, 3);
        $m!($lv.line_anchor, 3);
        $m!($lv.line_corr, 3);
        $m!($lv.line_dev, 3);
        $m!($lv.line_pin, 3);
        $m!($lv.line_wall, 3);
        $m!($lv.line_hollow, 3);
        $m!($lv.predicted, 1);
        $m!($lv.supplied, 1);
        $m!($lv.lined, 1);
        $m!($lv.watered, 1);
        $m!($lv.built, 1);
        $m!($lv.routed, 1);
        $m!($lv.corners, 1);
        $m!($lv.finished, 1);
    };
}

/// A level's every value as bytes.
fn save(lv: &Level) -> Vec<u8> {
    let mut out = Vec::new();
    for layer in &lv.layers {
        put_paged(&mut out, layer);
    }
    macro_rules! put {
        ($f:expr, $k:expr) => {
            put_paged(&mut out, &$f)
        };
    }
    fields!(put, lv);
    out
}

/// A level's values read back over `lv`, which has its bounds and nothing
/// else; false where the bytes are not a whole level.
fn load(lv: &mut Level, mut inp: &[u8]) -> bool {
    let cells = lv.cap();
    let mut read = || -> Option<()> {
        let inp = &mut inp;
        for l in 0..lv.layers.len() {
            lv.layers[l] = take_paged(inp, cells)?;
        }
        macro_rules! take {
            ($f:expr, $k:expr) => {
                $f = take_paged(inp, $k * cells)?
            };
        }
        fields!(take, lv);
        inp.is_empty().then_some(())
    };
    read().is_some()
}

/// Where kept levels live: `LADDER_CACHE`, or the system's temporary
/// directory.
fn dir() -> PathBuf {
    std::env::var_os("LADDER_CACHE").map_or_else(|| std::env::temp_dir().join("yasno-ladder"), PathBuf::from)
}

/// The name of the top over `bounds` of a ground, from everything it is
/// made of.
pub fn key(seed: u64, site: (i32, i32), bounds: &Bounds, p: &Params, ground: &impl Ground) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (FORMAT, seed, site, bounds).hash(&mut h);
    for x in [p.channel_head, p.catchment_full, p.cut_from, p.cut_full, p.wall, p.ring] {
        x.to_bits().hash(&mut h);
    }
    p.water.hash(&mut h);
    let (w, lo, hi) = *bounds;
    let mut out = vec![0.0; ground.layers()];
    for a in 0..4 {
        for b in 0..4 {
            let (i, j) = (lo.0 + (hi.0 - lo.0) * (2 * a + 1) / 8, lo.1 + (hi.1 - lo.1) * (2 * b + 1) / 8);
            ground.read(w, i, j, &mut out);
            out.iter().for_each(|x| x.to_bits().hash(&mut h));
            ground.erodibility(w, i, j).to_bits().hash(&mut h);
            ground.age(w, i, j).to_bits().hash(&mut h);
        }
    }
    format!("top-{:016x}", h.finish())
}

/// The kept level named `key` read over `lv`; false where none is kept or
/// it does not read whole.
pub fn read(key: &str, lv: &mut Level) -> bool {
    let mut bytes = Vec::new();
    match std::fs::File::open(dir().join(key)) {
        Ok(mut f) => f.read_to_end(&mut bytes).is_ok() && load(lv, &bytes),
        Err(_) => false,
    }
}

/// `lv` kept as `key`; a failure to write leaves the next run to build it.
pub fn write(key: &str, lv: &Level) {
    let dir = dir();
    let tmp = dir.join(format!("{key}.part"));
    let ok = std::fs::create_dir_all(&dir).is_ok()
        && std::fs::File::create(&tmp).and_then(|mut f| f.write_all(&save(lv))).is_ok()
        && std::fs::rename(&tmp, dir.join(key)).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
}
