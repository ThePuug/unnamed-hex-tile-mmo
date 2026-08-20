//! Erosional faces — where the layers below leave ground too steep to stand.

//! A face is not something a later layer should have to *discover* by reading
//! the surface around a tile. It is deliberate geometry: a cirque headwall, a
//! ravine wall. The layer that cuts one knows exactly where it is, how tall it
//! stands and what ground lies at its foot, so it publishes that and anything
//! above reads it instead of sampling neighbours to infer it back.

//! One question is answered from this index today, at a single tile with no
//! neighbourhood read: how deep the debris shed by nearby faces lies on it.

//! Capping how high ground may stand belongs here too — the only ground within
//! reach that can sit far below a tile is ground a carve took away, and every
//! carve publishes its floor. It is not here yet: driven from these faces the
//! cap cuts rims the composed surface does not, because a face carries one
//! spine's chain while the surface is the maximum over every spine and the sea
//! beneath them. Until a face answers as the composite does, the threshold
//! limiter reads the surface.

//! Consumers past slope form want the same geometry: cliff bands want to know
//! where the walls are, scree and tor siting want their feet.

use common::HexSpatialGrid;

/// A face steeper than the ground can hold, reduced to what a consumer needs.
#[derive(Clone, Copy, Debug)]
pub struct ErosionalFace {
    /// Ground at the foot of the face — where debris comes to rest, and the
    /// level anything above the face may be cut back toward.
    pub wx: f64,
    pub wy: f64,
    /// Altitude at the foot.
    pub floor: f64,
    /// How far the face stands above its foot.
    pub height: f64,
}

/// Faces of one spine, indexed for lookup at a tile.
///
/// Cell size is the reach a consumer may ask about, and each face is inserted
/// across that radius, so a single-cell query returns every face that can act
/// on the point — no ring walk, no distance scan over faces that cannot reach.
pub struct FaceIndex {
    grid: HexSpatialGrid<ErosionalFace>,
    /// The faces as recorded, once each. The grid holds a copy per cell the
    /// face reaches, so it cannot be walked without seeing one face many times.
    faces: Vec<ErosionalFace>,
    reach: f64,
}

impl FaceIndex {
    /// Build an index answering queries out to `reach` world units.
    pub fn new(reach: f64) -> Self {
        Self { grid: HexSpatialGrid::new(reach.max(1.0)), faces: Vec::new(), reach }
    }

    /// Record a face. Ignored when it stands below `min_height`, which is what
    /// keeps the index to the ground that actually fails rather than every
    /// stream bank in the world.
    pub fn insert(&mut self, face: ErosionalFace, min_height: f64) {
        if !(face.height > min_height) || !face.floor.is_finite() {
            return;
        }
        self.grid.insert_radius(face.wx, face.wy, self.reach, face);
        self.faces.push(face);
    }

    /// Copy these faces into `other`, so a layer publishes one index for a cell
    /// rather than one per feature it grew there.
    pub fn extend_into(&self, other: &mut FaceIndex) {
        for face in &self.faces {
            other.insert(*face, f64::NEG_INFINITY);
        }
    }

    /// These faces with their floors read off `surface` — the ground the whole
    /// cell composes, rather than the one feature that cut the face.
    ///
    /// A producer only knows its own chain, and the composed surface is the
    /// maximum over every feature standing on the same ground, so a floor taken
    /// from one of them is never above the truth and is often below it. A face
    /// that understates its floor claims ground was taken that is still there,
    /// and a consumer capping heights against it cuts down to reach it.
    pub fn recomposed(&self, surface: &dyn Fn(f64, f64) -> f64, min_height: f64) -> FaceIndex {
        let mut out = FaceIndex::new(self.reach);
        for face in &self.faces {
            let top = face.floor + face.height;
            let floor = surface(face.wx, face.wy);
            out.insert(
                ErosionalFace { wx: face.wx, wy: face.wy, floor, height: top - floor },
                min_height,
            );
        }
        out
    }

    /// The lowest altitude reachable from (wx, wy) at `critical`, over the faces
    /// that can act here — the cap on how high this ground may stand, or `None`
    /// where no face is in range.
    ///
    /// The threshold limiter without a neighbourhood: the only ground within
    /// reach that can sit far below a tile is ground a carve took away, and
    /// every carve published its floor.
    ///
    /// **Only as good as the floors it is given.** A face published against one
    /// producer's chain claims ground that compositing may have buried, and
    /// capping against that cuts real rims toward a floor the world does not
    /// have. Correct once [`FaceIndex::recomposed`] has been folded over the
    /// whole envelope a query resolves against, and not before.
    pub fn limit_at(&self, wx: f64, wy: f64, critical: f64) -> Option<f64> {
        let mut limit = f64::MAX;
        for f in self.grid.query(wx, wy) {
            let d = (wx - f.wx).hypot(wy - f.wy);
            if d > self.reach {
                continue;
            }
            let allowed = f.floor + critical * d;
            if allowed < limit {
                limit = allowed;
            }
        }
        (limit != f64::MAX).then_some(limit)
    }

    /// Visit the faces that can act at (wx, wy). Every query here folds as it
    /// walks rather than gathering first, so nothing on this path allocates —
    /// a reader that wants the set itself can build one, but the index will not
    /// hand it a reason to.
    pub fn for_each_near(&self, wx: f64, wy: f64, mut visit: impl FnMut(&ErosionalFace, f64)) {
        for f in self.grid.query(wx, wy) {
            let d = (wx - f.wx).hypot(wy - f.wy);
            if d <= self.reach {
                visit(f, d);
            }
        }
    }

    /// Visit every face lying within `radius` of a point — the bulk form of
    /// [`Self::for_each_near`], for a reader settling a whole cell at once.
    /// Faces just outside are included: the caller is gathering, not deciding.
    pub fn for_each_in(&self, wx: f64, wy: f64, radius: f64, visit: impl FnMut(&ErosionalFace)) {
        if self.faces.is_empty() {
            return;
        }
        self.grid.for_each_within(wx, wy, radius, visit);
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Depth of debris standing at (wx, wy): the tallest repose cone thrown
    /// from the foot of any face in range, thinning with distance from it.
    ///
    /// `cap` bounds the pile so a cone always runs out inside `reach`, which is
    /// what stops the index truncating an apron and leaving a step at the edge
    /// of what it can see.
    pub fn apron_at(&self, wx: f64, wy: f64, repose: f64, cap: f64) -> f64 {
        let mut apron = 0.0f64;
        for f in self.grid.query(wx, wy) {
            let d = (wx - f.wx).hypot(wy - f.wy);
            if d > self.reach {
                continue;
            }
            let depth = f.height.min(cap) - repose * d;
            if depth > apron {
                apron = depth;
            }
        }
        apron
    }
}

impl Default for FaceIndex {
    fn default() -> Self {
        Self::new(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REACH: f64 = 3.0;
    const REPOSE: f64 = 1.5;

    fn face(wx: f64, height: f64) -> ErosionalFace {
        ErosionalFace { wx, wy: 0.0, floor: 100.0, height }
    }

    /// Ground with nothing standing over it carries no debris.
    #[test]
    fn no_face_no_apron() {
        let idx = FaceIndex::new(REACH);
        assert!(idx.is_empty());
        assert_eq!(idx.apron_at(0.0, 0.0, REPOSE, REACH * REPOSE), 0.0);
    }

    /// A face shorter than the filter is not one — otherwise every stream bank
    /// in the world lands in the index.
    #[test]
    fn short_faces_are_not_recorded() {
        let mut idx = FaceIndex::new(REACH);
        idx.insert(face(0.0, 1.0), 4.0);
        assert!(idx.is_empty(), "a 1 z step was recorded as a face");
        idx.insert(face(0.0, 9.0), 4.0);
        assert_eq!(idx.len(), 1);
    }

    /// Debris lies deepest at the foot and thins at the repose slope, reaching
    /// nothing by the edge of the reach — a cone that ran out any later would
    /// leave a step where the index stops seeing it.
    #[test]
    fn the_apron_thins_away_from_the_foot() {
        let cap = REACH * REPOSE;
        let mut idx = FaceIndex::new(REACH);
        idx.insert(face(0.0, 500.0), 1.0);

        let mut prev = f64::MAX;
        for step in 0..=REACH as i32 {
            let d = idx.apron_at(step as f64, 0.0, REPOSE, cap);
            assert!(d <= prev + 1e-9, "apron thickened at {step}: {d} > {prev}");
            prev = d;
        }
        assert!(idx.apron_at(0.0, 0.0, REPOSE, cap) > 0.0, "no apron at the foot");
        assert!(prev <= 1e-9, "apron still {prev} deep at the edge of the reach");
        assert_eq!(
            idx.apron_at(REACH + 1.0, 0.0, REPOSE, cap),
            0.0,
            "a face acted past its reach"
        );
    }

    /// Sources composite by max, so which face is recorded first cannot change
    /// the answer — the same discipline the carves themselves keep.
    #[test]
    fn faces_composite_by_max() {
        let cap = REACH * REPOSE;
        let build = |order: [f64; 2]| {
            let mut idx = FaceIndex::new(REACH);
            for h in order {
                idx.insert(face(1.0, h), 1.0);
            }
            idx.apron_at(0.0, 0.0, REPOSE, cap)
        };
        assert_eq!(build([9.0, 500.0]), build([500.0, 9.0]));
    }
}
