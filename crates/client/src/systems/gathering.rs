//! Gathering on the client: G asks the server to gather what the keys
//! act on (`aim`); the tiles the server changes are laid over the map as
//! they arrive.

use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::{
    components::{
        entity_type::{decorator::Decorator, EntityType},
        position::Position,
        Actor,
    },
    message::{Do, Event, Try},
    resources::map::Map,
    summary::{mesh_region_lattice, summary_lattice, LOD_LEVELS},
    summary_mesh::MeshRegionKey,
};
use qrz::Qrz;

use crate::resources::SummaryMeshes;

/// The key that gathers, or opens a pile.
pub const KEYCODE_GATHER: KeyCode = KeyCode::KeyG;

/// The loot window's keys: take everything, and close.
pub const KEYCODE_TAKE_ALL: KeyCode = KeyCode::NumpadEnter;
pub const KEYCODE_CLOSE: KeyCode = KeyCode::Numpad0;

/// The digit keys that take an entry of the loot window's row.
pub const ENTRY_KEYS: [KeyCode; 9] = [
    KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3,
    KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6,
    KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
];

/// The loot window the local player has open, as the server last said: a
/// cell to each slot of its tile, the digit that takes it the cell's
/// place, or None where it has none open.
#[derive(Resource, Default)]
pub struct LootWindow {
    pub cells: Option<[Option<common_bevy::components::equipment::Lot>; common::TILE_SLOTS as usize]>,
}

/// Every tile the server has changed in the chunks the client holds, as its
/// cover now stands. A tile arriving in a chunk takes its change from here,
/// since a change may reach the client before the chunk it lies in.
#[derive(Resource, Default)]
pub struct CoverChanges(pub HashMap<(i32, i32), common::Cover>);

impl CoverChanges {
    /// The tile `typ` at `qrz` as the server has changed it.
    pub fn laid_over(&self, qrz: Qrz, typ: EntityType) -> EntityType {
        match (typ, self.0.get(&(qrz.q, qrz.r))) {
            (EntityType::Decorator(d), Some(&cover)) => EntityType::Decorator(Decorator { cover, ..d }),
            _ => typ,
        }
    }
}

/// Asks the server to gather what the keys act on (`aim`) when G is
/// pressed, where G is the key that acts on it; the menu holds it as it
/// holds every gameplay key.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<crate::systems::focus::NumpadFocus>,
    window: Res<LootWindow>,
    loading: Res<crate::systems::settlers::Loading>,
    aiming: crate::systems::aim::Aiming,
    claims: Res<crate::systems::building::Claims>,
    player: Query<Entity, (With<Actor>, With<Position>)>,
    mut writer: MessageWriter<Try>,
    mut settling: ResMut<crate::systems::input::Settling>,
) {
    if menu.open {
        return;
    }
    // A view has no character, and nothing to gather with
    let Ok(ent) = player.single() else { return };
    // An open loot window works the numpad while it is the panel opened
    // last. One open on a storehouse is the storehouse's to work
    // (`settlers::load`).
    let stored = loading.what() == Some(crate::systems::settlers::Kept::Store);
    if window.cells.is_some() && !stored && focus.has(crate::systems::focus::Panel::Loot) {
        for (slot, key) in ENTRY_KEYS.iter().take(common::TILE_SLOTS as usize).enumerate() {
            if keys.take(*key, "Take the pile in this cell") {
                writer.write(Try { event: Event::Take { ent, slot: Some(slot as u8) } });
            }
        }
        if keys.take(KEYCODE_TAKE_ALL, "Take everything in the loot window") {
            writer.write(Try { event: Event::Take { ent, slot: None } });
        }
        if keys.take(KEYCODE_CLOSE, "Close the loot window") {
            writer.write(Try { event: Event::CloseLoot { ent } });
        }
    }
    if !keys.pressed(KEYCODE_GATHER, "Gather the tree, boulder or reeds outlined, or open the pile") {
        return;
    }
    let Some((_, aim)) = aiming.aim(claims.lives) else {
        info!("gather: nothing in reach");
        return;
    };
    if aim.acts != crate::systems::aim::Acts::Gather {
        info!("gather: {:?} on {:?} is the interact key's", aim.acts, aim.stands);
        return;
    }
    info!("gather: asking for slot {} of {:?}", aim.slot, aim.tile);
    // The server turns the player to face what it gathers and steps it to
    // where the work reaches, which the input opened behind the press
    // brings back confirmed.
    settling.begin();
    writer.write(Try { event: Event::Gather { ent, q: aim.tile.q, r: aim.tile.r, slot: aim.slot as u8 } });
}

/// What an actor is doing at a gather, as the server says: at its work,
/// looping the work's clip with the tool in hand, or stooped to its pile.
#[derive(Component, Clone, Copy, Debug)]
pub struct Gathering(pub common::gathering::Activity);

/// Keeps each actor's gathering as the server says, and drops it when the
/// actor stops.
pub fn do_activity(mut commands: Commands, mut reader: MessageReader<Do>) {
    for message in reader.read() {
        let Do { event: Event::Activity { ent, activity } } = message else { continue };
        let Ok(mut actor) = commands.get_entity(*ent) else { continue };
        match activity {
            Some(activity) => { actor.insert(Gathering(*activity)); }
            None => { actor.remove::<Gathering>(); }
        }
    }
}

/// Keeps the local player's loot window as the server sends it.
pub fn do_loot(mut reader: MessageReader<Do>, mut window: ResMut<LootWindow>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Loot { ent, cells } } = message else { continue };
        if player.contains(*ent) {
            window.cells = *cells;
        }
    }
}

/// Lays each tile the server changed over the map, keeps it for the chunk
/// it lies in, and has the regions standing that tile's trees built again:
/// the tiles' and the first summary level's, both stood from the map. The
/// levels past them draw the summaries, which the server revises itself.
pub fn apply(
    mut reader: MessageReader<Do>,
    map: Res<Map>,
    mut changes: ResMut<CoverChanges>,
    mut meshes: ResMut<SummaryMeshes>,
) {
    for message in reader.read() {
        let Do { event: Event::CoverChanged { q, r, cover, .. } } = message else { continue };
        let (q, r, cover) = (*q, *r, *cover);
        changes.0.insert((q, r), cover);
        let Some((qrz, typ)) = map.get_by_qr(q, r) else { continue };
        map.insert(qrz, changes.laid_over(qrz, typ));
        for &radius in &LOD_LEVELS[..2] {
            let cell = summary_lattice(radius).cell_id(q, r);
            let (mn, mm) = mesh_region_lattice().cell_id(cell.0, cell.1);
            if let Some(state) = meshes.states.get_mut(&MeshRegionKey { r: radius, mn, mm }) {
                state.stale = true;
            }
        }
    }
}
