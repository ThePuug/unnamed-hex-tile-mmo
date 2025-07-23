use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{ 
    common::{
        components::{ *,
            behaviour::*, 
            entity_type::{ *,
                decorator::*
            }, 
            offset::*
        }, 
        message::{Event, *}, 
        plugins::nntree::*, resources::map::Map 
    },
    server::{resources::terrain::*, *
    }
};

pub fn tick(
    mut writer: EventWriter<Tick>,
    query: Query<&Behaviour>,
    nntree: Res<NNTree>,
) {
    for (it,_) in nntree.iter() {
        let ent = Entity::from_bits(it);
        let &behaviour = query.get(ent).expect("actor without behaviour!");
        writer.write(Tick { ent, behaviour });
    }
}

pub fn try_incremental(
    mut reader: EventReader<Try>,  
    mut writer: EventWriter<Do>,
 ) {
    for &message in reader.read() {
        if let Try { event: Event::Incremental { ent, attr } } = message { 
            writer.write(Do { event: Event::Incremental { ent, attr }}); 
        }
    }
 }

 pub fn try_discover(
    mut commands: Commands,
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
            if let Some((qrz, ent)) = map.find(qrz + Qrz{q:0,r:0,z:5}, -10) {
                if let Ok((_, &typ)) = query.get(ent) {
                    writer.write(Do { event: Event::Spawn { ent, typ, qrz } });
                } else {
                    warn!("Invalid entity: {ent} at {qrz:?}");
                }
            } else {
                let px = map.convert(qrz).xy();
                let qrz = Qrz { q:qrz.q, r:qrz.r, z:terrain.get(px.x, px.y)};
                if map.get(qrz).is_some() { continue; }
                let ent = commands.spawn((
                    Loc::new(qrz),
                    Offset::default(),
                    EntityType::Decorator(DecoratorImpl{ index: 3, is_solid: true }),
                    Transform {
                        translation: map.convert(qrz),
                        ..default()}, 
                )).id();
                map.insert(qrz, ent);
            }
        }
    }
}
