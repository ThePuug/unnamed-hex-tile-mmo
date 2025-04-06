use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{*,
    common::{
        components::{ *,
            offset::*,
        },
        message::{*, Event},
    },
};

pub fn try_incremental(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
 ) {
    for &message in reader.read() {
        if let Try { event: Event::Incremental { ent, attr } } = message { 
            writer.send(Do { event: Event::Incremental { ent, attr }}); 
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
            if let Some((qrz, ent)) = map.find(qrz, -5) {
                if let Ok((_, &typ)) = query.get(ent) {
                    writer.send(Do { event: Event::Spawn { ent, typ, qrz } });
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
                    EntityType::Decorator(DecoratorDescriptor{ index: 3, is_solid: true }),
                    Transform {
                        translation: map.convert(qrz),
                        ..default()}, 
                )).id();
                map.insert(qrz, ent);
            }
        }
    }
}
