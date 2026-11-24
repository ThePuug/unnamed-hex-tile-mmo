//! The settings in force (`common::settings`): on the server as the store
//! has them, a cold one as it was when the server started; on a client
//! as the server tells of them.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub use common::settings::Setting;

/// Every setting's value, as it is in force.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_den_is_told_of_as_far_as_one_is_drawn() {
        let edge = crate::summary::threshold_horiz(crate::summary::LOD_LEVELS[2]);
        let reach = (edge / (common::grid::HEX_RADIUS * 3f32.sqrt())).ceil() as i32;
        assert_eq!(common::settings::DEN_REACH, reach);
    }

    #[test]
    fn a_setting_stands_within_its_range_however_it_is_set() {
        let mut settings = Settings::default();
        for setting in Setting::ALL {
            let (least, most) = setting.range();
            settings.set(setting, i32::MAX);
            assert_eq!(settings.get(setting), most);
            settings.set(setting, i32::MIN);
            assert_eq!(settings.get(setting), least);
        }
    }
}
