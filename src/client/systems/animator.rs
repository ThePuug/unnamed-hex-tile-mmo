use std::time::Duration;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{client::components::*,
    common::{
        components::{ heading::*, offset::*, * },
        resources::{map::Map, *}
    }
};

pub fn update(
    query: Query<(Entity, &Loc, &Offset, &Heading, &Transform, &AirTime, &Animates)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    for (entity, &loc, &offset, &heading, &transform, &airtime, &animates) in &query {
        // Only local player (with input buffer) uses offset.step for physics movement
        let is_local_player = buffers.get(&entity).is_some();
        
        let target = match (is_local_player, offset, heading) {
            // Local player actively moving: use physics position
            (true, offset, _) if offset.step.length_squared() > 0.01 => map.convert(*loc) + offset.step,
            // Player has a heading set: position them in that triangle of the hex
            (_, _, heading) if *heading != Qrz::default() => {
                let dir = map.convert(*loc + *heading) - map.convert(*loc);
                map.convert(*loc) + dir * HERE
            },
            // Default: center of tile
            _ => map.convert(*loc),
        };

        let (mut player, mut transitions) = q_anim.get_mut(animates.0).unwrap();
        let dist = transform.translation.distance_squared(target);
        if dist > 0. || airtime.step.is_some() {
            if transitions.get_main_animation() != Some(3.into()) {
                transitions.play(&mut player, 3.into(), Duration::from_millis(300)).set_speed(1.).repeat();                            
            }
        } else if transitions.get_main_animation() != Some(2.into()) {
            transitions.play(&mut player, 2.into(), Duration::from_millis(300)).set_speed(1.).repeat();
        }
    }
}