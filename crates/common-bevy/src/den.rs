//! A den as it is drawn and walked round: which archetype's model, which
//! seed of it, its turn, and whether its pack was killed. The server and
//! every client read its pieces from the model's own declarations
//! (`common::den`), so both stand the same circles.

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use common::den::{Model, Piece};

use crate::archetype::EnemyArchetype;

/// How far past its origin a piece that stands no circles reaches, in
/// world units: a clump of reeds, a scatter of bones.
pub const REACH: f32 = 1.0;

/// A den as drawn, about the centre of the tile its pack stands on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DenLook {
    pub archetype: EnemyArchetype,
    /// Which of its model's seeds it is; wraps
    pub seed: u8,
    /// Its turn about the vertical, a rotation about +y
    pub yaw: f32,
    /// Whether its pack was killed
    pub cleared: bool,
}

impl DenLook {
    /// Its model, none where the assets hold none for its archetype.
    pub fn model(&self) -> Option<&'static Model> {
        Model::find(self.archetype.den_model())
    }

    /// Its pieces, none where it has no model.
    pub fn pieces(&self) -> &'static [Piece] {
        self.model().map_or(&[], |model| model.pieces(self.seed as usize, self.cleared))
    }

    /// How many tiles out from its own the den's ground is cleared, active
    /// or cleared alike: as far as its active pieces reach, each out to its
    /// furthest circle, or [`REACH`] past its origin where it has none.
    pub fn clearing(&self) -> u32 {
        let active = DenLook { cleared: false, ..*self };
        let reach = active
            .pieces()
            .iter()
            .map(|piece| {
                let [x, z] = piece.placed(self.yaw);
                let extent = piece.solid.iter().map(|c| c[0].hypot(c[1]) + c[2]).fold(REACH, f32::max);
                x.hypot(z) + extent
            })
            .fold(0.0, f32::max);
        (reach / (common::camera::HEX_RADIUS * 3f32.sqrt())).ceil() as u32
    }

    /// The circles its pieces stand: each centre from the centre of its
    /// tile in world units along the ground, its radius and its height.
    pub fn circles(&self) -> Vec<(Vec2, f32, f32)> {
        self.pieces()
            .iter()
            .flat_map(|piece| piece.circles(self.yaw))
            .map(|(centre, radius, height)| (Vec2::from(centre), radius, height))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every archetype's den has a model, solid where it stands.
    #[test]
    fn every_archetype_dens_in_a_model_it_walks_round() {
        for archetype in EnemyArchetype::ALL {
            let look = DenLook { archetype, seed: 0, yaw: 0.0, cleared: false };
            assert!(look.model().is_some(), "{archetype:?}");
            assert!(!look.circles().is_empty(), "{archetype:?}");
        }
    }

    /// Turning a den turns its circles with it, about its own centre.
    #[test]
    fn a_den_turns_its_circles_about_its_centre() {
        let look = DenLook { archetype: EnemyArchetype::Defender, seed: 1, yaw: 0.0, cleared: false };
        let turned = DenLook { yaw: 1.0, ..look };
        for ((a, ra, _), (b, rb, _)) in look.circles().into_iter().zip(turned.circles()) {
            assert!((a.length() - b.length()).abs() < 1e-4);
            assert_eq!(ra, rb);
        }
    }
}
