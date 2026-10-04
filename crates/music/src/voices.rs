//! Which bank plays each instrument. A score names its players as
//! General MIDI programs, and the default bank — GeneralUser GS — plays
//! every one; but banks specialise, and where another plays a program
//! better, this table says which file and which of its presets. A piece
//! never names a bank: the choice is made once here for every piece that
//! uses the instrument, so a better guitar is every guitar.
//!
//! A bank this table names sits in the default bank's directory; a
//! machine without it plays the program on the default bank, so a build
//! there sounds as the table was before the bank was added.

/// A program played by another bank: its file, and the bank number and
/// preset the program is under there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// The General MIDI program it plays, 0-based; for a drum kit, the
    /// kit's program on the percussion channel.
    pub program: u8,
    pub percussion: bool,
    /// The bank's file name, beside the default bank's.
    pub file: &'static str,
    pub bank: u8,
    pub preset: u8,
}

/// Every program another bank plays.
pub const VOICES: &[Voice] = &[];

/// The voice for `program`, where another bank plays it.
pub fn voice(program: u8, percussion: bool) -> Option<&'static Voice> {
    VOICES.iter().find(|v| v.program == program && v.percussion == percussion)
}
