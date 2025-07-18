use std::{
    collections::{ BTreeMap, HashMap }, 
    f64::consts::SQRT_3
};

use glam::Vec3;
use derive_more::*;

use crate::qrz::{ self, Qrz };

const ORIENTATION: ([f64; 4], [f64; 4]) = (
    [SQRT_3, SQRT_3/2., 0., 3./2.],
    [SQRT_3/3., -1./3., 0., 2./3.],
);

pub trait Convert<T,U> {
    fn convert(&self, it: T) -> U;
}

#[derive(Clone, Debug, Default, IntoIterator)]
pub struct Map<T> {
    radius: f32,
    rise: f32,
    #[into_iterator(owned)]
    tree: BTreeMap<Qrz, T>,
    hash: HashMap<Qrz, T>,
}

impl<T> Map<T> 
where T : Copy {
    pub fn new(radius: f32, rise: f32) -> Self {
        Self { radius, rise, tree: BTreeMap::new(), hash: HashMap::new() }
    }

    pub fn radius(&self) -> f32 { self.radius }
    pub fn rise(&self) -> f32 { self.rise }

    pub fn line(&self, a: &Qrz, b: &Qrz) -> Vec<Qrz> { 
        let dist = a.flat_distance(b); 
        let step = 1. / (dist+1) as f32;
        (1..=dist+1).map(|i| {
            self.convert(self.convert(*a).lerp(self.convert(*b), i as f32 * step))
        }).collect()
    }

    pub fn find(&self, qrz: Qrz, dist: i8) -> Option<(Qrz, T)> {
        for i in 0..=dist.abs() {
            let z = if dist < 0 { -i as i16 } else { i as i16 };
            let qrz = qrz + Qrz { q: 0, r: 0, z };
            if let Some(obj) = self.get(qrz) { return Some((qrz, *obj)); }
        }
        None
    }

    pub fn get(&self, qrz: Qrz) -> Option<&T> {
        self.hash.get(&qrz)
    }

    pub fn insert(&mut self, qrz: Qrz, obj: T) {
        self.tree.insert(qrz, obj);
        self.hash.insert(qrz, obj);
    }

    pub fn remove(&mut self, qrz: Qrz) -> Option<T> {
        self.tree.remove(&qrz);
        self.hash.remove(&qrz)
    }

    pub fn vertices(&self, qrz: Qrz) -> Vec<Vec3> {
        let center = self.convert(qrz);
        let w = (self.radius as f64 * SQRT_3 / 2.) as f32;
        let h = self.radius / 2.;
        vec![
            center + Vec3 { x: 0., y: self.rise, z: -self.radius },
            center + Vec3 { x: w,  y: self.rise, z: -h },
            center + Vec3 { x: w,  y: self.rise, z: h },
            center + Vec3 { x: 0., y: self.rise, z: self.radius },
            center + Vec3 { x: -w, y: self.rise, z: h },
            center + Vec3 { x: -w, y: self.rise, z: -h },
            center + Vec3 { x: 0., y: self.rise, z: 0. },
        ]
    }
}

impl<T> Convert<Vec3,Qrz> for Map<T> {
    fn convert(&self, other: Vec3) -> Qrz {
        let q = (ORIENTATION.1[0] * other.x as f64 + ORIENTATION.1[1] * other.z as f64) / self.radius as f64;
        let r = (ORIENTATION.1[2] * other.x as f64 + ORIENTATION.1[3] * other.z as f64) / self.radius as f64;
        let z = other.y as f64 / self.rise as f64;
        qrz::round(q, r, z)
    }
}

impl<T> Convert<Qrz,Vec3> for Map<T> {
    fn convert(&self, other: Qrz) -> Vec3 {
        let x = (ORIENTATION.0[0] * other.q as f64 + ORIENTATION.0[1] * other.r as f64) * self.radius as f64;
        let z = (ORIENTATION.0[2] * other.q as f64 + ORIENTATION.0[3] * other.r as f64) * self.radius as f64;
        let y = other.z as f64 * self.rise as f64;
        Vec3 { x: x as f32, y: y as f32, z: z as f32 }
    }
}
