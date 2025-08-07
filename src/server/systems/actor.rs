use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{ 
    common::{
        components::{ 
            entity_type::{ decorator::*, *}, 
            offset::Offset, *
        }, 
        message::{Component, Event, *}, 
        plugins::nntree::*, 
        resources::map::* 
    },
    server::resources::terrain::*
};

 pub fn try_discover(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut map: ResMut<Map>,
    terrain: Res<Terrain>,
    query: Query<(&Loc, &EntityType)>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Discover { ent, qrz } } = message {
            let (&loc, _) = query.get(ent).unwrap();
            if loc.flat_distance(&qrz) > 25 { continue; }
            if let Some((qrz, typ)) = map.find(qrz + Qrz{q:0,r:0,z:5}, -10) {
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz } }); 
            } else {
                let px = map.convert(qrz).xy();
                let qrz = Qrz { q:qrz.q, r:qrz.r, z:terrain.get(px.x, px.y)};
                let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
                map.insert(qrz, typ);
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz } });
            }
        }
    }
}

pub fn update(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Loc, &Offset), Changed<Offset>>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (ent, &loc0, &offset) in &mut query {
        let px = map.convert(*loc0);
        let qrz = map.convert(px + offset.state);
        if *loc0 != qrz {
            let loc = Loc::new(qrz);
            let count = nntree.within_unsorted_iter::<Hexhattan>(&loc.into(), 1_i16.into()).count();
            if count >= 7 { warn!("tried to move loc to full tile"); continue }
            let component = Component::Loc(loc);
            writer.write(Try { event: Event::Incremental { ent, component } }); 
        }
    }
}
