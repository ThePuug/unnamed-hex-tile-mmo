use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// An actor a player controls, on the server and on every client: the one
/// word for it. An NPC has none.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Reflect)]
#[reflect(Component)]
pub struct PlayerControlled;


/// The side an actor fights on. Actors on different sides are hostile;
/// actors on the same side are allies. This is the one hostility rule:
/// targeting, pursuit, auto-attacks and combat state all ask it.
///
/// The server holds it as a component and sends it with every actor's
/// spawn, so a client targets by the same rule: two NPC parties staged on
/// sides of their own are hostile there too.
#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Side(pub u8);

impl Side {
    pub const PLAYERS: Side = Side(0);
    pub const WILD: Side = Side(1);

    /// The side of an actor that is, or is not, `PlayerControlled`
    pub fn of_player(is_player: bool) -> Side {
        if is_player { Side::PLAYERS } else { Side::WILD }
    }

    pub fn is_hostile_to(self, other: Side) -> bool {
        self != other
    }
}
