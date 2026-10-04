//! The phrase: four bars that state a theme and end it, either as a
//! period — two bars answered by two — or a sentence — one bar, the
//! same again a step up, then a continuation that closes. A phrase
//! sits on one row of the harmony's cycle and is open or closed as
//! that row is, so a cycle is a question and its answer, and a piece's
//! bars come in whole phrases so every boundary is a cadence.

pub const BARS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Period,
    Sentence,
}

pub const FORMS: [Form; 2] = [Form::Period, Form::Sentence];

/// What a bar of a phrase is: one of the theme's two bars, the first
/// again a step up, or a cadence stepping to the phrase's end. A
/// period's second and fourth bars take the phrase's endings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Idea(usize),
    Sequence,
    Cadence,
}

impl Form {
    pub fn slots(self) -> [Slot; 4] {
        match self {
            Form::Period => [Slot::Idea(0), Slot::Idea(1), Slot::Idea(0), Slot::Idea(1)],
            Form::Sentence => [Slot::Idea(0), Slot::Sequence, Slot::Idea(1), Slot::Cadence],
        }
    }
}
