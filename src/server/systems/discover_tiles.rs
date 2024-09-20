use bevy::prelude::*;

use crate::{*, Event};

pub fn discover_tiles(
    mut writer: EventWriter<Event>,
    map: Res<Map>,
    query: Query<(Entity, &Pos, &Heading)>,
) {
    for (ent, pos, heading) in query.iter() {
        let target = pos.hx + heading.0;
        if !map.0.contains_key(&target) {
            writer.send(Event::Spawn { 
                ent: Entity::PLACEHOLDER,
                typ: EntityType::Decorator(DecoratorDescriptor { index: 1, is_solid: true }), 
                hx: target,
            });
        }

        writer.send(Event::Move { ent, pos: *pos, heading: *heading });
    }
}
