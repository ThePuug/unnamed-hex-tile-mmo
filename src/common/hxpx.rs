use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Eq, PartialEq, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct Hx {
    pub q: i16,
    pub r: i16,
    pub z: i16
}

impl From<Hx> for (i16,i16,i16) {
    fn from(hx: Hx) -> (i16,i16,i16) {
        (hx.q,hx.r,hx.z)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct Px {
    pub x: i16,
    pub y: i16,
    pub z: i16
}

impl From<Px> for (i16,i16,i16) {
    fn from(px: Px) -> (i16,i16,i16) {
        (px.x,px.y,px.z)
    }
}

impl From<Hx> for Px {
    fn from(_hx: Hx) -> Px {
        Px { x:0, y:0, z:0 }
    }
}