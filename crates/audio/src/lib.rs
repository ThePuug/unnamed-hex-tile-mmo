//! What every sound generator stands on: a seeded stream of draws, the
//! measurements an ear would make of rendered audio, and the files it
//! goes out as. No generator's own vocabulary lives here.

pub mod encode;
pub mod measure;
pub mod rng;

pub const SAMPLE_RATE: u32 = 48000;
