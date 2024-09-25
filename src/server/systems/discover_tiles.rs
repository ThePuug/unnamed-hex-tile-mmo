use bevy::prelude::*;

use crate::{*,
    common::{
        components::hx::*,
        message::{*, Event},
    },
};

pub fn try_local_events(
    mut writer: EventWriter<Do>,
    mut reader: EventReader<Try>,
    map: Res<Map>,
    terrain: Res<Terrain>,
 ) {
    for &message in reader.read() {
        trace!("Message: {:?}", message);
        match message {
            Try { event: Event::Discover { hx, .. } } => {
                let (loc, ent) = map.find(hx, 10);
                if let Some(hx) = loc {
                    writer.send(Do { event: Event::Spawn { 
                        ent,
                        typ: EntityType::Decorator(DecoratorDescriptor{ index: 1, is_solid: true }), 
                        hx,
                    } }); 
                } else {
                    let px = Vec3::from(hx);
                    writer.send(Do { event: Event::Spawn {
                        ent,
                        typ: EntityType::Decorator(DecoratorDescriptor{ index: 3, is_solid: true }),
                        hx: Hx { z: terrain.get(px.x, px.y), ..hx },
                    } });
                }
            },
            Try { event: Event::Move { ent, hx, heading } } => { 
                writer.send(Do { event: Event::Move { ent, hx, heading }}); 
            },
            _ => {}
        }
    }
 }