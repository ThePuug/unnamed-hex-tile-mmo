use bevy::prelude::*;

use crate::{*,
    common::{
        components::hx::*,
        message::{*, Event},
    },
};

pub fn try_move(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
 ) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Move { ent, hx, heading } } => { 
                // trace!("Message: {:?}", message);
                writer.send(Do { event: Event::Move { ent, hx, heading }}); 
            },
            _ => {}
        }
    }
 }

 pub fn try_discover(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut map: ResMut<Map>,
    terrain: Res<Terrain>,
    query: Query<(&Hx, &EntityType)>,
 ) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Discover { ent, hx } } => {
                if let Ok((&loc, _)) = query.get(ent) {
                    if loc.distance(&hx) > 5 { return; }
                    let (hxn, entn) = map.find(hx, -5);
                    if let Some(hx) = hxn {
                        if let Ok((_, &typ)) = query.get(entn) {
                            writer.send(Do { event: Event::Spawn { ent, typ, hx, } });
                        } else {
                            warn!("Invalid entity: {:?} at {:?}", entn, hx);
                        }
                    } else {
                        let px = Vec3::from(hx).xy();
                        let hx = Hx { z: terrain.get(px.x, px.y), ..hx };
                        if map.get(hx) != Entity::PLACEHOLDER { return; }
                        let ent = commands.spawn((
                            hx,
                            Offset::default(),
                            EntityType::Decorator(DecoratorDescriptor{ index: 3, is_solid: true }),
                            Transform {
                                translation: (hx,Offset::default()).into_screen(),
                                ..default()}, 
                        )).id();
                        map.insert(hx, ent);
                    }
                } else {
                    warn!("Invalid entity: {:?}", ent);
                }
            },
            _ => {}
        }
    }
}