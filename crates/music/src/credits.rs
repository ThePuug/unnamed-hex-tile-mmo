//! What the game and its music were made with, and whose recordings the
//! music sounds through, as each one's licence asks it be named. Whatever
//! plays the music shows these where a listener sees them: the game on its
//! credits, the music player in its own. A bank or a cabinet added to
//! `voices` or `rigs` is credited here.

/// One work the music sounds through.
#[derive(Clone, Copy, Debug)]
pub struct Credit {
    /// What it is in the music.
    pub what: &'static str,
    /// The work, whose it is, and what was changed, as its licence asks.
    pub work: &'static str,
    pub licence: &'static str,
    pub licence_link: &'static str,
}

/// The tools the game, its music and the code that composes it were
/// made with: each, whose it is, and where it is.
pub const MADE_WITH: &[(&str, &str, &str)] = &[("Claude Code", "Claude, by Anthropic", "https://claude.com")];

const BY: &str = "https://creativecommons.org/licenses/by/4.0/";
const ZERO: &str = "https://creativecommons.org/publicdomain/zero/1.0/";

/// The recordings, those whose licences require credit first.
pub const SOUNDS: &[Credit] = &[
    Credit {
        what: "Guitar cabinets",
        work: "speaker impulse responses by jesterdyne (freesound.org/people/jesterdyne), resampled and trimmed",
        licence: "CC BY 4.0",
        licence_link: BY,
    },
    Credit {
        what: "Rock kit",
        work: "MuldjordKit by Lars Muldjord (drumgizmo.org), FreePats stereo version by Roberto (freepats.zenvoid.org), converted to SF2 by FreePats",
        licence: "CC BY 4.0",
        licence_link: BY,
    },
    Credit { what: "Guitars, finger bass, piano, organ, tenor sax", work: "FreePats (freepats.zenvoid.org)", licence: "CC0", licence_link: ZERO },
    Credit { what: "Picked bass, brush kit", work: "Karoryfer Samples (github.com/sfzinstruments)", licence: "CC0", licence_link: ZERO },
    Credit { what: "Harmonica", work: "Versilian Community Sample Library (github.com/sgossner/VCSL)", licence: "CC0", licence_link: ZERO },
    Credit {
        what: "Every other instrument",
        work: "GeneralUser GS by S. Christian Collins (schristiancollins.com)",
        licence: "its licence",
        licence_link: "https://www.schristiancollins.com/generaluser",
    },
];
