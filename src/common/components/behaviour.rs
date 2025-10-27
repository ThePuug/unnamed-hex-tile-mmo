use tinyvec::ArrayVec;
use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    #[default] Unset,
    Controlled,
}

/// Defines how PathTo approaches its destination
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PathLimit {
    /// Move N tiles towards dest, then succeed (even if not at dest)
    By(u16),
    /// Move towards dest until N tiles away, then succeed
    Until(u16),
    /// Move all the way to dest (complete path)
    Complete,
}

impl Default for PathLimit {
    fn default() -> Self {
        PathLimit::Complete
    }
}

#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct PathTo {
    pub dest: Qrz,
    pub path: ArrayVec<[Qrz; 20]>,
    pub limit: PathLimit,
}