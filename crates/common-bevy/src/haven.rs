//! Where the haven stands.

use qrz::Qrz;

/// Haven location, in hex coordinates.
///
/// Sited on open ground on the home continent, with a view: level and dry
/// underfoot, no tree within forty tiles, the ground falling some 160
/// z-levels over the next 300 tiles in front, and land seen out past the
/// client's reach across a third of the round, some 700 z-levels of relief
/// in it. `world`'s `haven_probe` scores the continent for such sites. The
/// z is a placeholder — the server resolves the real one from the terrain
/// at startup, because elevation is generated, not authored.
pub const HAVEN_LOCATION: Qrz = Qrz { q: 107289, r: -7677, z: 0 };
