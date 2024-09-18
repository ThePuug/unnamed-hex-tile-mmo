use bevy::prelude::*;

use crate::common::{
    components::{ *,
        keybits::*, 
        message::Event,
    }, 
    hx::*,
    input::*,
    resources::map::*,
};

pub fn handle_input(
    mut events: EventWriter<Event>,
    time: Res<Time>,
    map: Res<Map>,
    mut query: Query<(&KeyBits, &mut Pos, &mut Heading)>,
) {
    for (&keys, mut pos, mut heading) in query.iter_mut() {
        if keys & (KEYBIT_UP | KEYBIT_DOWN | KEYBIT_LEFT | KEYBIT_RIGHT) != default() {
            if keys & KEYBIT_UP != default() {
                if keys & KEYBIT_LEFT != default() || keys & KEYBIT_RIGHT == default()
                    &&(heading.0 == (Hx {q:-1, r: 0, z: -1})
                    || heading.0 == (Hx {q:-1, r: 1, z: -1})
                    || heading.0 == (Hx {q: 1, r:-1, z: -1})) { *heading = Heading { 0:Hx {q:-1, r: 1, z: -1} }; }
                else  { *heading = Heading { 0:Hx {q: 0, r: 1, z: -1} }; }
            } else if keys & KEYBIT_DOWN != default() {
                if keys & KEYBIT_RIGHT != default() || keys & KEYBIT_LEFT == default()
                    &&(heading.0 == (Hx {q: 1, r: 0, z: -1})
                    || heading.0 == (Hx {q: 1, r:-1, z: -1})
                    || heading.0 == (Hx {q:-1, r: 1, z: -1})) { *heading = Heading { 0:Hx {q: 1, r: -1, z: -1} }; }
                else { *heading = Heading { 0:Hx {q: 0, r:-1, z: -1} }; }
            } 
            else if keys & KEYBIT_RIGHT != default() { *heading = Heading { 0:Hx {q: 1, r: 0, z: -1} }; }
            else if keys & KEYBIT_LEFT != default() { *heading = Heading { 0:Hx {q:-1, r: 0, z: -1} }; }

            let target = pos.hx + heading.0;
            let px = Vec3::from(pos.hx);
            let delta = Vec3::from(target).xy() - (px + pos.offset).xy();
            pos.offset += (delta.normalize_or_zero() * 100. * time.delta_seconds()).extend(0.);
            let px_new = px + pos.offset;
            if Hx::from(px_new) != pos.hx {
                pos.hx = Hx::from(px_new);
                pos.offset = px_new - Vec3::from(pos.hx); 
            }

            if !map.0.contains_key(&target) {
                events.send(Event::Spawn { 
                    ent: Entity::PLACEHOLDER,
                    typ: EntityType::Decorator(DecoratorDescriptor { index: 1, is_solid: true }), 
                    hx: target,
                });
            }
        }
    }
}
