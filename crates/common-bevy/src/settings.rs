//! The numbers an admin may change without a build. The server owns them:
//! it keeps each in the store, tells every client of them as it enters
//! and whenever one changes, and takes a change from the admin console.
//!
//! A setting is hot or cold ([`Setting::hot`]). A hot one is read from
//! [`Settings`] each time it is used, so a change holds from the next
//! frame, on the server and on every client. A cold one is read once, as
//! the server starts: a change is kept ([`Kept`]) and waits for the next
//! start. A setting may be hot only where nothing built from it is kept
//! around, or what was built and what is built next disagree.

use bevy::prelude::*;
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

impl Setting {
    /// Every setting, in the order a [`Settings`] holds them and the
    /// console lists them.
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
    /// Otherwise it is kept and holds from the server's next start.
    pub fn hot(self) -> bool {
        !matches!(self, Setting::PlayerSlots)
    }

    /// What it is where no admin has changed it.
    pub fn default(self) -> i32 {
        match self {
            Setting::DensShown => 10,
            Setting::DenSight => den_reach(),
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
            Setting::DenSight => (0, den_reach()),
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

/// The furthest a den is drawn from, in tiles: the outer edge of the
/// level past the first summary level.
fn den_reach() -> i32 {
    let edge = crate::summary::threshold_horiz(crate::summary::LOD_LEVELS[2]);
    (edge / (common::grid::HEX_RADIUS * 3f32.sqrt())).ceil() as i32
}

/// Every setting's value: on the server and on a client, the ones in
/// force.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings([i32; Setting::ALL.len()]);

impl Default for Settings {
    fn default() -> Self {
        Self(Setting::ALL.map(Setting::default))
    }
}

impl Settings {
    pub fn get(&self, setting: Setting) -> i32 {
        self.0[setting as usize]
    }

    /// Sets `setting` to `value`, held within its range.
    pub fn set(&mut self, setting: Setting, value: i32) {
        self.0[setting as usize] = setting.held(value);
    }
}

/// Every setting as the store keeps it. A cold setting changed since the
/// server started differs here from the [`Settings`] in force.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kept(pub Settings);

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
    fn a_setting_stands_within_its_range_however_it_is_set() {
        let mut settings = Settings::default();
        for setting in Setting::ALL {
            let (least, most) = setting.range();
            assert!((least..=most).contains(&setting.default()), "{setting:?} starts within its range");
            settings.set(setting, i32::MAX);
            assert_eq!(settings.get(setting), most);
            settings.set(setting, i32::MIN);
            assert_eq!(settings.get(setting), least);
        }
    }
}
