use std::time::Duration;

use bevy::prelude::*;
use qrz::Convert;

use crate::{client::components::*,
    common::{
        components::{ heading::*, keybits::*, offset::*, * }, 
        resources::map::Map
    }
};

pub fn update(
    query: Query<(&Loc, &Offset, &Heading, &KeyBits, &Transform, &AirTime, &Animates)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    map: Res<Map>,
) {
    for (&loc, &offset, &heading, &keybits, &transform, &airtime, &animates) in &query {
        let target = match (keybits, offset, heading) {
            (keybits, offset, _) if keybits != KeyBits::default() => map.convert(*loc) + offset.step,
            (_, _, heading) => map.convert(*loc) + map.convert(*heading) * HERE,
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