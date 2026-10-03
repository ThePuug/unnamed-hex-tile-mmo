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
