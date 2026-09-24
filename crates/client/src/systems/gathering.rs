//! Gathering on the client: G asks the server to gather the nearest thing
//! in the player's reach, which is marked while it is; the tiles the
//! server changes are laid over the map as they arrive.

use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::{
    components::{
        entity_type::{decorator::Decorator, EntityType},
        heading::Heading,
        position::Position,
        Actor,
    },
    geometry::{boulder_center, slot_center},
    message::{Do, Event, Try},
    resources::map::Map,
    summary::{mesh_region_lattice, summary_lattice, LOD_LEVELS},
    summary_mesh::MeshRegionKey,
};
use qrz::Qrz;

use crate::resources::{RenderOrigin, SummaryMeshes};

/// The key that gathers, or opens a pile.
pub const KEYCODE_GATHER: KeyCode = KeyCode::KeyG;

/// The loot window's keys: take everything, move to the next row of
/// entries, and close.
pub const KEYCODE_TAKE_ALL: KeyCode = KeyCode::NumpadEnter;
pub const KEYCODE_NEXT_ROW: KeyCode = KeyCode::NumpadDecimal;
pub const KEYCODE_CLOSE: KeyCode = KeyCode::Numpad0;

/// The digit keys that take an entry of the loot window's row.
pub const ENTRY_KEYS: [KeyCode; 9] = [
    KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3,
    KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6,
    KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
];

/// The loot window the local player has open, a stack to an entry, as the
/// server last said, None where it has none open; and the row of entries
/// the digits take from.
#[derive(Resource, Default)]
pub struct LootWindow {
    pub entries: Option<Vec<common::Stack>>,
    pub row: usize,
}

impl LootWindow {
    /// How many rows of [`ENTRY_KEYS`] the entries fill, never less than
    /// one.
    pub fn rows(&self) -> usize {
        self.entries.as_ref().map_or(0, |e| e.len()).div_ceil(ENTRY_KEYS.len()).max(1)
    }
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

/// What G would gather: slot `slot` of the tile at `tile`, standing at
/// `at`, xz in world units from the centre of the tile `position` is
/// measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GatherTarget {
    pub tile: Qrz,
    pub slot: usize,
    pub at: Vec2,
}

/// The nearest gatherable thing in reach of an entity at `position`
/// facing `heading`: in the tile it stands on or one of the three in its
/// front half. Measured in the frame of `position`'s tile, so it picks the same
/// however far out the world is.
pub fn gather_target(map: &Map, position: &Position, heading: Heading) -> Option<GatherTarget> {
    let here = position.reached(map);
    let from = position.offset.xz();
    std::iter::once(here)
        .chain(heading.front_dirs().map(|d| here + d))
        .filter_map(|tile| map.get_by_qr(tile.q, tile.r))
        .flat_map(|(tile, typ)| {
            let EntityType::Decorator(decorator) = typ else { return Vec::new() };
            let cover = decorator.cover;
            let (dq, dr) = (tile.q - position.tile.q, tile.r - position.tile.r);
            (0..common::TILE_SLOTS as usize)
                .filter(|&k| common::gathering::anchor(cover, k) == k)
                .filter(|&k| common::gathering::reachable(cover, k))
                .map(|k| {
                    let tree = cover.content(k).slots() == 2;
                    let (x, z) = match common::SITE_SLOTS.iter().position(|s| s[0] == k).filter(|_| tree) {
                        Some(site) => slot_center(dq, dr, site, &common::sway(tile.q, tile.r, site)),
                        None => boulder_center(dq, dr, k, &common::boulder_sway(tile.q, tile.r, k)),
                    };
                    GatherTarget { tile, slot: k, at: Vec2::new(x, z) }
                })
                .collect()
        })
        .min_by(|a, b| a.at.distance_squared(from).total_cmp(&b.at.distance_squared(from)))
}

/// Asks the server to gather the target when G is pressed; the menu holds
/// it as it holds every gameplay key.
pub fn request(
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<crate::systems::focus::NumpadFocus>,
    mut window: ResMut<LootWindow>,
    map: Res<Map>,
    player: Query<(Entity, &Position, &Heading), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    if menu.open {
        return;
    }
    let Ok((ent, position, heading)) = player.single() else {
        warn!("gather: no single local player to gather with");
        return;
    };
    // An open loot window works the numpad while it is the panel opened
    // last.
    if window.entries.is_some() && focus.has(crate::systems::focus::Panel::Loot) {
        let row = window.row;
        for (i, key) in ENTRY_KEYS.iter().enumerate() {
            if keyboard.clear_just_pressed(*key) {
                let entry = (row * ENTRY_KEYS.len() + i) as u8;
                writer.write(Try { event: Event::Take { ent, entry: Some(entry) } });
            }
        }
        if keyboard.clear_just_pressed(KEYCODE_TAKE_ALL) {
            writer.write(Try { event: Event::Take { ent, entry: None } });
        }
        if keyboard.clear_just_pressed(KEYCODE_NEXT_ROW) {
            window.row = (row + 1) % window.rows();
        }
        if keyboard.clear_just_pressed(KEYCODE_CLOSE) {
            writer.write(Try { event: Event::CloseLoot { ent } });
        }
    }
    if !keyboard.just_pressed(KEYCODE_GATHER) {
        return;
    }
    let here = position.reached(&map);
    let Some(target) = gather_target(&map, position, *heading) else {
        info!("gather: nothing in reach at {here:?} facing {:?}", heading.hex_dir());
        return;
    };
    info!("gather: asking for slot {} of {:?}", target.slot, target.tile);
    writer.write(Try { event: Event::Gather { ent, q: target.tile.q, r: target.tile.r, slot: target.slot as u8 } });
}

/// Rings what G would gather.
pub fn mark(
    mut gizmos: Gizmos,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
    player: Query<(&Position, &Heading), With<Actor>>,
) {
    let Ok((position, heading)) = player.single() else { return };
    let Some(target) = gather_target(&map, position, *heading) else { return };
    let rise = common_bevy::systems::movement::surface_y_from(position.tile, target.at, target.tile, &map);
    let at = origin.render_tile(&map, position.tile) + Vec3::new(target.at.x, rise + 0.05, target.at.y);
    gizmos.circle(Isometry3d::new(at, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 0.35, Color::srgb(1.0, 0.85, 0.3));
}

/// Keeps the local player's loot window as the server sends it.
pub fn do_loot(mut reader: MessageReader<Do>, mut window: ResMut<LootWindow>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Loot { ent, entries } } = message else { continue };
        if player.contains(*ent) {
            window.entries = entries.clone();
            window.row = window.row.min(window.rows() - 1);
        }
    }
}

/// Lays each tile the server changed over the map, keeps it for the chunk
/// it lies in, and has the regions standing that tile's trees built again.
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
        for &radius in &LOD_LEVELS[..3] {
            let cell = summary_lattice(radius).cell_id(q, r);
            let (mn, mm) = mesh_region_lattice().cell_id(cell.0, cell.1);
            if let Some(state) = meshes.states.get_mut(&MeshRegionKey { r: radius, mn, mm }) {
                state.stale = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Content;

    fn map_with(tiles: &[(Qrz, common::Cover)]) -> Map {
        let map = Map::new(qrz::Map::new(common::camera::HEX_RADIUS, common::camera::RISE, qrz::HexOrientation::FlatTop));
        for &(qrz, cover) in tiles {
            map.insert(qrz, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
        map
    }

    /// G reaches the tile underfoot and the three of the front half, never
    /// one behind, and takes the nearest thing there.
    #[test]
    fn the_target_is_the_nearest_in_reach() {
        let here = Qrz { q: 0, r: 0, z: 0 };
        let heading = Heading::NORTH;
        let ahead = here + heading.hex_dir();
        let behind = here + heading.reversed().hex_dir();
        let trees = common::Cover::NONE.with(0, Content::Pine).with(1, Content::Pine);
        let crag = common::Cover::NONE.with_boulder(0).with_rock(common::Rock::Sandstone);

        let map = map_with(&[(here, common::Cover::NONE), (ahead, crag), (behind, trees)]);
        let target = gather_target(&map, &Position::at_tile(here), heading).unwrap();
        assert_eq!((target.tile, target.slot), (ahead, 0), "the boulder ahead, not the trees behind");

        let map = map_with(&[(here, trees), (ahead, crag), (behind, trees)]);
        let target = gather_target(&map, &Position::at_tile(here), heading).unwrap();
        assert_eq!(target.tile, here, "a tree underfoot is nearer than the boulder ahead");
        assert!(common::SITE_SLOTS.iter().any(|s| s[0] == target.slot), "a tree is named by its first slot");

        let map = map_with(&[(here, common::Cover::NONE), (behind, trees)]);
        assert_eq!(gather_target(&map, &Position::at_tile(here), heading), None);
    }
}
