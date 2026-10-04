//! Music, composed from a seed and rendered through a SoundFont: a piece
//! is a pure function of its seed that returns a score, and the renderer
//! turns a score into samples, so a program can compose as it plays —
//! the music player does, and a client can one day. The assets repo's
//! musicgen is the offline generator around it — proofs, checks, files.

pub mod cue;
pub mod hall;
pub mod ladder;
pub mod master;
pub mod perform;
pub mod pieces;
pub mod render;
pub mod score;
pub mod teller;
pub mod theory;
pub mod tune;
pub mod voices;

pub use audio::rng;
