use bevy::prelude::*;

use crate::common::{
    components::{ *,
        keybits::*, 
        message::Event,
    }, 
    hxpx::*,
    input::*,
    resources::map::*,
};

pub fn handle_input(
    mut events: EventWriter<Event>,
    time: Res<Time>,
    map: Res<Map>,
    mut query: Query<(&KeyBits, &mut Transform, &mut Heading)>,
) {
    for (&keys, mut transform, mut heading) in query.iter_mut() {
        if keys & (KEYBIT_UP | KEYBIT_DOWN | KEYBIT_LEFT | KEYBIT_RIGHT) != default() {
            if keys & KEYBIT_UP != default() {
                if keys & KEYBIT_LEFT != default() || keys & KEYBIT_RIGHT == default()
                    &&(heading.0 == (Hx {q:-1,r: 0,z: 0})
                    || heading.0 == (Hx {q:-1,r: 1,z: 0})
                    || heading.0 == (Hx {q: 1,r:-1,z: 0})) { *heading = Heading { 0:Hx {q:-1,r: 1,z: 0} }; }
                else  { *heading = Heading { 0:Hx {q: 0,r: 1,z: 0} }; }
            } else if keys & KEYBIT_DOWN != default() {
                if keys & KEYBIT_RIGHT != default() || keys & KEYBIT_LEFT == default()
                    &&(heading.0 == (Hx {q: 1,r: 0,z: 0})
                    || heading.0 == (Hx {q: 1,r:-1,z: 0})
                    || heading.0 == (Hx {q:-1,r: 1,z: 0})) { *heading = Heading { 0:Hx {q: 1,r: -1,z: 0} }; }
                else { *heading = Heading { 0:Hx {q: 0,r:-1,z: 0} }; }
            } 
            else if keys & KEYBIT_RIGHT != default() { *heading = Heading { 0:Hx {q: 1,r: 0,z: 0} }; }
            else if keys & KEYBIT_LEFT != default() { *heading = Heading { 0:Hx {q:-1,r: 0,z: 0} }; }

            let loc = Hx::from(transform.translation);
            let target = loc + heading.0;
            let target_vec = Vec3::from(target);
            let delta = target_vec.xy() - transform.translation.xy();
            trace!("loc: {:?}, target: {:?}, delta: {:?}", loc, target, delta);
            transform.translation += (delta.normalize_or_zero() * 100. * time.delta_seconds()).extend(0.);

            if !map.0.contains_key(&target) {
                events.send(Event::Spawn { 
                    ent: Entity::PLACEHOLDER,
                    typ: EntityType::Decorator(DecoratorDescriptor { index: 1, is_solid: true }), 
                    translation: target_vec,
                });
            }
        }
    }
}
