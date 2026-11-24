//! The numbers an admin may change without a build. They are rows of the
//! store's `setting` table, each by its number, and are changed there by
//! the admin console (`crates/console`), with or without a server
//! running. The server, which the console starts and stops, reads them
//! as it starts and hears of each change.
//!
//! A setting is hot or cold ([`Setting::hot`]). A hot one is read from
//! `common_bevy::settings::Settings` each time it is used, so a change
//! holds within a frame of being made, on the server and on every
//! client. A cold one is read once, as the server starts: a change waits
//! for the next start. A setting may be hot only where nothing built
//! from it is kept around, or what was built and what is built next
//! disagree.

use serde::{Deserialize, Serialize};

/// One number an admin may change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Setting {
    /// How many dens stand on the compass at once: the nearest of those
    /// the server has told of.
    DensShown,
    /// How far a player is told of a den from, in tiles.
    DenSight,
    /// How many times as fast settlers come and work.
    SettlerHaste,
    /// How many players every hearth counts as at home besides those
    /// who are.
    HearthPlayers,
    /// How many clients the server takes at once.
    PlayerSlots,
}

/// The furthest a den is drawn from, in tiles: the outer edge of the
/// level past the first summary level, which `common_bevy::settings`
/// checks this against.
pub const DEN_REACH: i32 = 1083;

impl Setting {
    /// Every setting, in the order a `Settings` holds them and the
    /// admin console lists them.
    pub const ALL: [Setting; 5] = [Setting::DensShown, Setting::DenSight, Setting::SettlerHaste, Setting::HearthPlayers, Setting::PlayerSlots];

    /// The number the store keeps it by: given once and never again, so
    /// a rename or a new order of [`Setting::ALL`] keeps it.
    pub fn id(self) -> i32 {
        match self {
            Setting::DensShown => 1,
            Setting::DenSight => 2,
            Setting::SettlerHaste => 3,
            Setting::HearthPlayers => 4,
            Setting::PlayerSlots => 5,
        }
    }

    /// The setting the store keeps by `id`, where this build has it.
    pub fn of(id: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|setting| setting.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Setting::DensShown => "Dens on the Compass",
            Setting::DenSight => "Den Sight",
            Setting::SettlerHaste => "Settler Haste",
            Setting::HearthPlayers => "Players per Hearth",
            Setting::PlayerSlots => "Player Slots",
        }
    }

    /// What it does, in a line.
    pub fn says(self) -> &'static str {
        match self {
            Setting::DensShown => "The compass pins this many dens, the nearest",
            Setting::DenSight => "A player is told of a den from this many tiles",
            Setting::SettlerHaste => "Settlers come and work this many times as fast",
            Setting::HearthPlayers => "Every hearth counts as this many more at home",
            Setting::PlayerSlots => "The server takes this many clients at once",
        }
    }

    /// Whether a change holds at once, for the server and every client.
    /// Otherwise it holds from the server's next start.
    pub fn hot(self) -> bool {
        !matches!(self, Setting::PlayerSlots)
    }

    /// What it is where no admin has changed it.
    pub fn default(self) -> i32 {
        match self {
            Setting::DensShown => 10,
            Setting::DenSight => DEN_REACH,
            Setting::SettlerHaste => 1,
            Setting::HearthPlayers => 0,
            Setting::PlayerSlots => 64,
        }
    }

    /// The least and the most it may be.
    pub fn range(self) -> (i32, i32) {
        match self {
            Setting::DensShown => (0, 50),
            // No den is drawn further out, so none is worth telling of
            Setting::DenSight => (0, DEN_REACH),
            Setting::SettlerHaste => (1, 1000),
            Setting::HearthPlayers => (0, 1000),
            // The transport takes no more
            Setting::PlayerSlots => (1, 1024),
        }
    }

    /// `value` held within its range.
    pub fn held(self, value: i32) -> i32 {
        let (least, most) = self.range();
        value.clamp(least, most)
    }
}

/// The store a server keeps to where `DATABASE_URL` names none: a role
/// and a database of its own on the machine's PostgreSQL, made once by
/// `CREATE ROLE mmo LOGIN PASSWORD 'mmo' CREATEDB` and
/// `CREATE DATABASE mmo OWNER mmo`.
pub const LOCAL_STORE: &str = "postgres://mmo:mmo@localhost:5432/mmo";

/// Where the store is: for the server, and for the admin console that
/// changes its settings there.
pub fn store_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| LOCAL_STORE.to_owned())
}

/// The store's channel a server listens on: a setting changed.
pub const SETTING_CHANNEL: &str = "setting";

/// Set in the environment of a server the admin console started. A
/// server finds it or does not run: the console is the one thing that
/// starts a server, holds it as its child and stops it.
pub const CONSOLE_ENV: &str = "MMO_ADMIN_CONSOLE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_is_held_in_its_place_and_kept_by_a_number_of_its_own() {
        for (place, setting) in Setting::ALL.into_iter().enumerate() {
            assert_eq!(setting as usize, place, "{setting:?} stands where a Settings holds it");
            assert_eq!(Setting::of(setting.id()), Some(setting));
        }
    }

    #[test]
    fn a_setting_starts_within_its_range() {
        for setting in Setting::ALL {
            let (least, most) = setting.range();
            assert!((least..=most).contains(&setting.default()), "{setting:?} starts within its range");
            assert_eq!(setting.held(i32::MAX), most);
            assert_eq!(setting.held(i32::MIN), least);
        }
    }
}
