//! Where the haven stands.

use qrz::Qrz;

/// Haven location, in hex coordinates.
///
/// Sited on the brink of a belt's plateau on the home continent: the
/// front falls away to a basin plain some 560 z-levels below over the next
/// 300 tiles in the -r direction, with the next belt rising beyond it, the
/// plateau top lies a few levels up behind, and no water stands within six
/// hundred tiles. The z is a placeholder — the server resolves the real one
/// from the terrain at startup, because elevation is generated, not
/// authored.
pub const HAVEN_LOCATION: Qrz = Qrz { q: 104289, r: -4677, z: 0 };
