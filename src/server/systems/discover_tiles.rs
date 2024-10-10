use std::cmp::{max, min};

use bevy::prelude::*;

use crate::{*,
    common::{
        components::hx::*,
        message::{*, Event},
    },
};

pub fn try_local_events(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    map: Res<Map>,
    terrain: Res<Terrain>,
 ) {
    for &message in reader.read() {
        trace!("Message: {:?}", message);
        match message {
            Try { event: Event::Move { ent, hx, heading } } => { 
                writer.send(Do { event: Event::Move { ent, hx, heading }}); 
                for q in -5..=5 {
                    for r in max(-5, -q-5)..=min(5, -q+5) {
                        let hxn = hx + Hx { q, r, z: hx.z };
                        let (loc, ent) = map.find(hxn, -5);
                        if let Some(hx) = loc {
                            writer.send(Do { event: Event::Spawn { 
                                ent,
                                typ: EntityType::Decorator(DecoratorDescriptor{ index: 1, is_solid: true }), 
                                hx,
                            } }); 
                        } else {
                            let px = Vec3::from(hxn).xy();
                            writer.send(Do { event: Event::Spawn {
                                ent,
                                typ: EntityType::Decorator(DecoratorDescriptor{ index: 3, is_solid: true }),
                                hx: Hx { z: terrain.get(px.x, px.y), ..hxn },
                            } });
                        }
                    }
                }
            },
            _ => {}
        }
    }
 }