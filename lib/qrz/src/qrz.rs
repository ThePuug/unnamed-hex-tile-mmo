use std::ops::{Add, Mul, Sub};

use serde::{Deserialize, Serialize};

// q, r represent whether a 1 or not (regardless of sign)
// if neg && r { (q,-r) } else if neg || q && r { (-q,r) }
const DIRECTIONS: [Qrz; 6] = [
        Qrz { q: -1, r: 0, z: 0 }, // west
        Qrz { q: -1, r: 1, z: 0 }, // south-west
        Qrz { q: 0, r: 1, z: 0 }, // south-east
        Qrz { q: 1, r: 0, z: 0 }, // east
        Qrz { q: 1, r: -1, z: 0 }, // north-east
        Qrz { q: 0, r: -1, z: 0 }, // north-west
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Qrz {
    pub q: i16,
    pub r: i16,
    pub z: i16,
}

impl Qrz {
    pub fn flat_distance(&self, other: &Qrz) -> i16 {
        *[
            (self.q - other.q).abs(),
            (self.r - other.r).abs(),
            (-self.q-self.r - (-other.q-other.r)).abs()
        ].iter().max().unwrap()
    }

    pub fn normalize(&self) -> Qrz {
        let max = [self.q, self.r, -self.q-self.r].into_iter().max_by_key(|a| a.abs()).unwrap();
        Qrz { 
            q: if max == self.q { self.q.signum() } else { 0 }, 
            r: if max == self.r { self.r.signum() } else { 0 }, 
            z: 0 } }

    pub fn distance(&self, other: &Qrz) -> i16 {
        self.flat_distance(other) + (self.z-other.z).abs()
    }

    pub fn arc(&self, dir: &Qrz, radius: u8) -> Vec<Qrz> {
        assert!(self.flat_distance(dir) == 1);
        let start = dir.normalize() * radius as i16;
        let idx = DIRECTIONS.iter().position(|i| *i == dir.normalize()).unwrap();
        (1..=radius).map(|i| start + DIRECTIONS[(idx + 2) % 6] * i as i16)
            .chain((0..=radius).map(|i| start + DIRECTIONS[(idx - 2) % 6] * i as i16))
            .collect()
    }

    pub fn fov(&self, dir: &Qrz, dist: u8) -> Vec<Qrz> {
        (1..=dist).map(|i| self.arc(dir, i)).flatten().collect::<Vec<Qrz>>()
    }
}

impl Mul<i16> for Qrz {
    type Output = Qrz;
    fn mul(self, rhs: i16) -> Self::Output {
        Qrz { q: self.q * rhs, r: self.r * rhs, z: self.z * rhs }
    }
}

impl Add<Qrz> for Qrz {
    type Output = Qrz;
    fn add(self, rhs: Qrz) -> Self::Output {
        Qrz { q: self.q + rhs.q, r: self.r + rhs.r, z: self.z + rhs.z }
    }
}

impl Sub<Qrz> for Qrz {
    type Output = Qrz;
    fn sub(self, rhs: Qrz) -> Self::Output {
        Qrz { q: self.q - rhs.q, r: self.r - rhs.r, z: self.z - rhs.z }
    }
}

pub fn round(q0: f64, r0: f64, z0: f64) -> Qrz {
    let s0 = -q0-r0;
    let mut q = q0.round();
    let mut r = r0.round();
    let s = s0.round();

    let q_diff = (q - q0).abs();
    let r_diff = (r - r0).abs();
    let s_diff = (s - s0).abs();

    if q_diff > r_diff && q_diff > s_diff {
        q = -r-s;
    } else if r_diff > s_diff {
        r = -q-s;
    }

    Qrz { q: q as i16, r: r as i16, z: z0.round() as i16 }
}    
