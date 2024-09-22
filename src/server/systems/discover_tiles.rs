use bevy::prelude::*;

use crate::{*,
    common::message::{*, Event},
};

pub fn try_discover(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    map: Res<TerrainedMap>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Discover { hx, .. } } => {
                let (loc, ent) = map.find(hx, 3);
                if let Some(hx) = loc {
                    writer.send(Do { event: Event::Spawn { 
                        ent,
                        typ: EntityType::Decorator(DecoratorDescriptor{ index: 1, is_solid: true }), 
                        hx,
                    } }); 
                }
            }
            _ => {}
        }
    }
}
