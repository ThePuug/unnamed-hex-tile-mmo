//! The server's settings (`common_bevy::settings`): an admin's change is
//! taken here, kept by the store, and told to every client.

use bevy::prelude::*;
use common_bevy::{
    message::{Do, Event, Try},
    settings::{Kept, Settings},
};

/// Takes an admin's change to a setting: kept at once, in force at once
/// where the setting is hot, and every client told of both.
pub fn try_set(mut reader: MessageReader<Try>, mut settings: ResMut<Settings>, mut kept: ResMut<Kept>, mut writer: MessageWriter<Do>) {
    for message in reader.read() {
        let Try { event: Event::Set { setting, value } } = *message else { continue };
        let value = setting.held(value);
        if kept.0.get(setting) == value {
            continue;
        }
        kept.0.set(setting, value);
        if setting.hot() {
            settings.set(setting, value);
            info!("settings: {} is {value}", setting.name());
        } else {
            info!("settings: {} is {} and will be {value} once the server starts again", setting.name(), settings.get(setting));
        }
        writer.write(Do { event: Event::Settings { running: *settings, kept: kept.0 } });
    }
}
