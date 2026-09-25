use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    #[default] Unset,
    Controlled,
}

/// Marker component for player-controlled entities (vs NPC/AI-controlled)

/// Used for ally/enemy distinction in targeting, health bars, and other gameplay systems.

/// # Distinction from Behaviour::Controlled

/// On the **server**:
/// - `Behaviour::Controlled` = entity responds to player input
/// - `PlayerControlled` = entity is controlled by a human player (same as Behaviour::Controlled)

/// On the **client**:
/// - `Behaviour::Controlled` = entity movement is interpolated via server updates (ALL actors)
/// - `PlayerControlled` = entity represents a human player (for ally/enemy logic)

/// This separation allows:
/// - All client entities to use `Behaviour::Controlled` for smooth movement interpolation
/// - Only player entities to have `PlayerControlled` for ally targeting and UI
/// - Future faction/allegiance systems without changing movement code
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Reflect)]
#[reflect(Component)]
pub struct PlayerControlled;


/// The side an actor fights on. Actors on different sides are hostile;
/// actors on the same side are allies. This is the one hostility rule:
/// targeting, pursuit, auto-attacks and combat state all ask it.
///
/// The server holds it as a component. The client does not receive it and
/// derives it from `PlayerControlled` with [`Side::of_player`], since every
/// actor it sees is either a player or wild.
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
