//! Viewing: the client sees the world as an actor it does not control, an
//! admin's view of any fighter, with a player's camera and HUD. The server
//! makes the actor the connection's (`Presence::View`) and takes the
//! character out of the world; here `Viewed` moves onto the actor. Once it
//! is gone, or the view is stopped, the client leaves and comes back as a
//! fresh character, as if it had just logged in.

use bevy::prelude::*;

use common_bevy::message::{Do, Event, Try};

use super::{leave, play, Entered, Stage};
use crate::components::Viewed;

/// The actor the client sees as while it has no character in the world.
#[derive(Resource, Default)]
pub struct Viewing(pub Option<Entity>);

/// Play again as soon as the character screen is reached.
#[derive(Resource, Default)]
pub struct Rejoin(bool);

/// Moves `Viewed` onto the actor the server says the client now sees as.
pub fn do_view(
    mut reader: MessageReader<Do>,
    viewed: Query<Entity, With<Viewed>>,
    mut viewing: ResMut<Viewing>,
    mut commands: Commands,
) {
    for message in reader.read() {
        let Do { event: Event::View { ent } } = message else { continue };
        for e in &viewed {
            if e != *ent {
                commands.entity(e).try_remove::<Viewed>();
            }
        }
        commands.entity(*ent).try_insert(Viewed);
        viewing.0 = Some(*ent);
    }
}

/// Stops viewing: out of the world, then straight back in.
pub fn stop(writer: &mut MessageWriter<Try>, next: &mut NextState<Stage>, entered: &mut Entered, rejoin: &mut Rejoin, viewing: &mut Viewing) {
    viewing.0 = None;
    rejoin.0 = true;
    leave(writer, next, entered);
}

/// Ends the view once its actor is gone: dead and despawned, or out of
/// the world.
pub fn end_when_gone(
    mut viewing: ResMut<Viewing>,
    actors: Query<()>,
    mut writer: MessageWriter<Try>,
    mut next: ResMut<NextState<Stage>>,
    mut entered: ResMut<Entered>,
    mut rejoin: ResMut<Rejoin>,
) {
    let Some(actor) = viewing.0 else { return };
    if actors.contains(actor) {
        return;
    }
    info!("view: {actor} is gone; back as a fresh character");
    stop(&mut writer, &mut next, &mut entered, &mut rejoin, &mut viewing);
}

/// Plays again on reaching the character screen after a view.
pub fn rejoin(
    mut rejoin: ResMut<Rejoin>,
    mut writer: MessageWriter<Try>,
    mut next: ResMut<NextState<Stage>>,
    mut entered: ResMut<Entered>,
) {
    if std::mem::take(&mut rejoin.0) {
        play(&mut writer, &mut next, &mut entered);
    }
}
