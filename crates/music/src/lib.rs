//! Music, composed from a seed and rendered through a SoundFont: a piece
//! is a pure function of its seed that returns a score, and the renderer
//! turns a score into samples, so a program composes as it plays — the
//! client and the music player do, and nothing rendered ships. The assets
//! repo's musicgen proofs and checks the pieces.

pub mod banks;
pub mod cue;
pub mod hall;
pub mod ladder;
pub mod master;
pub mod perform;
pub mod pieces;
pub mod render;
pub mod rock;
pub mod score;
pub mod solo;
pub mod teller;
pub mod theory;
pub mod tune;
pub mod variation;
pub mod voices;

pub use audio::rng;

/// Seeds are drawn under this so they can be read and noted, and one
/// heard anywhere played again in the music player; a piece keys nothing
/// to a seed's value, so the range costs no variety.
pub const SEEDS: u64 = 1_000_000;
