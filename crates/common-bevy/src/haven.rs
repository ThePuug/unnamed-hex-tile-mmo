//! Where the haven stands.

use bevy::prelude::Resource;
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

/// The haven a process is launched with: `HAVEN=<q>,<r>`, else
/// [`HAVEN_LOCATION`]. The server stands its spawn and its dens' difficulty
/// there, and the world viewer centres there, so one setting moves both to
/// another continent.
pub fn launch() -> Qrz {
    let Ok(spec) = std::env::var("HAVEN") else { return HAVEN_LOCATION };
    let (q, r) = spec.split_once(',').expect("HAVEN is <q>,<r>");
    Qrz { q: q.trim().parse().expect("HAVEN's q is a whole number"), r: r.trim().parse().expect("HAVEN's r is a whole number"), z: 0 }
}

/// Where the haven stands in the world a client is in, as the server said
/// in `Event::Init`.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Haven(pub Qrz);

impl Default for Haven {
    fn default() -> Self {
        Haven(HAVEN_LOCATION)
    }
}
