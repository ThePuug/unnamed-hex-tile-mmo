//! The ground a den stands on. The world classifies each den site by what
//! its lower layers published there; each archetype dens on one habitat,
//! so the world knows ground and never archetypes.

/// What a den site's ground is, the most specific that holds there.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Habitat {
    /// Beside a river's channel
    River,
    /// Inside a stand that is mostly trees
    Woods,
    /// Inside a stand that is mostly brush
    Scrub,
    /// On a range, the high ground its wedge raises
    Range,
    /// On hard rock, harder than limestone
    Rock,
    /// Land none of the others holds
    Open,
}

impl Habitat {
    pub const ALL: [Habitat; 6] = [Habitat::River, Habitat::Woods, Habitat::Scrub, Habitat::Range, Habitat::Rock, Habitat::Open];
}

/// One piece of a den model as its GLB declares it, in the GLB's axes:
/// x and z along the ground, y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// Its mesh's index in the GLB, the variation it is drawn as
    pub mesh: usize,
    /// Where its origin stands from the den's centre, x and z
    pub at: [f32; 2],
    /// Its turn about the vertical, as a rotation about +y
    pub yaw: f32,
    /// Whether it lies turned to the ground, rather than standing upright
    pub lies: bool,
    /// The steepest ground it lies or stands on, in degrees
    pub slope: f32,
    /// Where it is solid, in its own frame: each circle's x, z, radius,
    /// and the height of what stands over it
    pub solid: &'static [[f32; 4]],
}

/// A den model: its file's stem, `models/den-<stem>-active.glb` and its
/// cleared twin, and the pieces of each scene of each, one scene a seed.
#[derive(Debug)]
pub struct Model {
    pub stem: &'static str,
    pub active: &'static [&'static [Piece]],
    pub cleared: &'static [&'static [Piece]],
}

include!(concat!(env!("OUT_DIR"), "/dens.rs"));

impl Model {
    /// The den model named `stem`.
    pub fn find(stem: &str) -> Option<&'static Model> {
        MODELS.iter().find(|model| model.stem == stem)
    }

    /// How many seeds it is built in: its scenes, the same in both states.
    pub fn seeds(&self) -> usize {
        self.active.len()
    }

    /// The pieces of the den at `seed`, active or `cleared`; `seed` wraps.
    pub fn pieces(&self, seed: usize, cleared: bool) -> &'static [Piece] {
        let scenes = if cleared { self.cleared } else { self.active };
        scenes[seed % scenes.len()]
    }
}

/// `(x, z)` turned by `yaw` about +y, as a rotation about the vertical
/// turns a point on the ground.
pub fn turned([x, z]: [f32; 2], yaw: f32) -> [f32; 2] {
    let (s, c) = yaw.sin_cos();
    [x * c + z * s, z * c - x * s]
}

impl Piece {
    /// Where the piece's origin stands about a den's centre with the den
    /// turned `yaw`.
    pub fn placed(&self, yaw: f32) -> [f32; 2] {
        turned(self.at, yaw)
    }

    /// The piece's circles about a den's centre with the den turned `yaw`:
    /// each centre, radius and height.
    pub fn circles(&self, yaw: f32) -> impl Iterator<Item = ([f32; 2], f32, f32)> + '_ {
        let at = self.placed(yaw);
        self.solid.iter().map(move |&[x, z, radius, height]| {
            let [dx, dz] = turned([x, z], self.yaw + yaw);
            ([at[0] + dx, at[1] + dz], radius, height)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every den model the assets hold is built active and cleared in the
    /// same seeds, each seed some pieces, each piece on ground it names.
    #[test]
    fn every_den_model_declares_its_pieces() {
        assert!(!MODELS.is_empty());
        for model in MODELS {
            assert_eq!(model.active.len(), model.cleared.len(), "{}", model.stem);
            for seed in 0..model.seeds() {
                for cleared in [false, true] {
                    let pieces = model.pieces(seed, cleared);
                    assert!(!pieces.is_empty(), "{} {seed} {cleared}", model.stem);
                    assert!(pieces.iter().all(|p| p.slope > 0.0 && p.solid.iter().all(|c| c[2] > 0.0)));
                }
            }
            assert!(model.active.iter().any(|pieces| pieces.iter().any(|p| !p.solid.is_empty())), "{} stands nothing solid", model.stem);
        }
    }

    /// A quarter turn takes the front, -z, to the side a rotation about +y
    /// takes it: toward -x.
    #[test]
    fn a_turn_is_about_the_vertical() {
        let [x, z] = turned([0.0, -1.0], std::f32::consts::FRAC_PI_2);
        assert!((x + 1.0).abs() < 1e-6 && z.abs() < 1e-6);
    }
}
