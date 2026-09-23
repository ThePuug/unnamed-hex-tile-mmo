//! The kinds of rock a continent is built of.

/// A kind of rock, by what it does under water: how fast a river cuts it,
/// how it stands against its neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rock {
    Shale,
    Sandstone,
    Limestone,
    Basement,
}

impl Rock {
    /// How fast a river cuts this rock, as a share of the rate it cuts
    /// shale: shale the fastest, the sandstones and limestones of a cover
    /// about half as fast, crystalline basement a third. Everything a
    /// river does is scaled by this: where its channel begins, how deep it
    /// has cut, how far it has swept its banks.
    pub fn erodibility(self) -> f64 {
        match self {
            Rock::Shale => 1.0,
            Rock::Sandstone => 0.6,
            Rock::Limestone => 0.5,
            Rock::Basement => 0.3,
        }
    }

    /// How much of the rain the rock keeps where roots reach it, as a
    /// share of what shale keeps: shale's clays hold it, a sandstone lets
    /// it through, basement carries a thin soil over sound rock, and a
    /// limestone drains it underground.
    pub fn retention(self) -> f64 {
        match self {
            Rock::Shale => 1.0,
            Rock::Sandstone => 0.75,
            Rock::Basement => 0.7,
            Rock::Limestone => 0.6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rock::Shale => "shale",
            Rock::Sandstone => "sandstone",
            Rock::Limestone => "limestone",
            Rock::Basement => "basement",
        }
    }
}
