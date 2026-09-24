//! Gathering: a player takes what stands in a slot, the tile keeps the
//! change, and every client holding the tile learns it.

use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::{
    chunk::loc_to_chunk,
    components::{
        entity_type::{decorator::Decorator, EntityType},
        equipment::{Equipment, Inventory},
        heading::Heading,
        Loc,
    },
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Qrz;

use crate::systems::actor::VisibleChunkCache;

/// Every tile players have changed, as its cover now stands. A tile is its
/// generated cover with its change laid over: whatever builds a tile for
/// the map or the wire takes it through [`WorldChanges::laid_over`].
#[derive(Resource, Default)]
pub struct WorldChanges(HashMap<(i32, i32), common::Cover>);

impl WorldChanges {
    /// The tile `typ` at `qrz` as players have left it.
    pub fn laid_over(&self, qrz: Qrz, typ: EntityType) -> EntityType {
        match (typ, self.0.get(&(qrz.q, qrz.r))) {
            (EntityType::Decorator(d), Some(&cover)) => EntityType::Decorator(Decorator { cover, ..d }),
            _ => typ,
        }
    }
}

/// Applies each gather a player asks for: the slot must lie in its reach,
/// its own tile or the one its heading faces, and hold something
/// gatherable. The tile's new cover goes to the map, to the changes, and
/// to every player holding its chunk; the yield goes to the bag.
pub fn try_gather(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut changes: ResMut<WorldChanges>,
    map: Res<Map>,
    mut players: Query<(&Loc, &Heading, &Equipment, &mut Inventory)>,
    holders: Query<(Entity, &VisibleChunkCache)>,
) {
    for message in reader.read() {
        let Try { event: Event::Gather { ent, q, r, slot } } = message else { continue };
        let (ent, q, r, slot) = (*ent, *q, *r, *slot as usize);
        let Ok((loc, heading, worn, mut bag)) = players.get_mut(ent) else {
            warn!("gather: {ent} is not a player");
            continue;
        };
        let here = **loc;
        let ahead = here + heading.hex_dir();
        if (q, r) != (here.q, here.r) && (q, r) != (ahead.q, ahead.r) || slot >= common::TILE_SLOTS as usize {
            info!("gather: {ent} at {here:?} facing {ahead:?} asked out of reach for slot {slot} of ({q}, {r})");
            continue;
        }
        let Some((qrz, EntityType::Decorator(decorator))) = map.get_by_qr(q, r) else {
            info!("gather: {ent} asked for ({q}, {r}), which the map does not hold");
            continue;
        };
        let Some(harvest) = common::gathering::harvest(decorator.cover, slot) else {
            info!("gather: {ent} asked for slot {slot} of ({q}, {r}), which holds nothing gatherable");
            continue;
        };
        // Until a loot window can leave the rest on the ground, a yield the
        // bag cannot take whole is not taken at all.
        if !bag.has_room_for(worn, harvest.material, harvest.amount) {
            info!("gather: {ent} has no room for {} {:?}", harvest.amount, harvest.material);
            continue;
        }
        info!("gather: {ent} took {} {:?} from slot {slot} of ({q}, {r})", harvest.amount, harvest.material);

        map.insert(qrz, EntityType::Decorator(Decorator { cover: harvest.cover, ..decorator }));
        changes.0.insert((q, r), harvest.cover);
        let chunk = loc_to_chunk(qrz);
        for (holder, cache) in &holders {
            if cache.sent.contains(&chunk) {
                writer.write(Do { event: Event::CoverChanged { ent: holder, q, r, cover: harvest.cover } });
            }
        }

        bag.add_material(harvest.material, harvest.amount);
        writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
    }
}
