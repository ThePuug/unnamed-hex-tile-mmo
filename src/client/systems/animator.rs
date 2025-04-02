use std::time::Duration;

use bevy::prelude::*;

use crate::{client::components::*,
    common::components::{
        heading::*,
        hx::*,
        keybits::*,
        offset::*,
    }
};

pub fn update(
    query: Query<(&Hx, &Offset, &Heading, &KeyBits, &Transform, &Animator)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (&hx, &offset, &heading, &keybits, &transform, &animator) in &query {
        let target = match (keybits, offset, heading) {
            (keybits, offset, _) if keybits != KeyBits::default() => Vec3::from(hx) + offset.step,
            (_, _, heading) => Vec3::from(hx) + Vec3::from(*heading) * HERE,
        };

        let (mut player, mut transitions) = q_anim.get_mut(*animator).unwrap();
        let dist = transform.translation.distance_squared(target);
        if dist > 0. {
            if transitions.get_main_animation() != Some(3.into()) {
                transitions.play(&mut player, 3.into(), Duration::ZERO).set_speed(1.).repeat();                            
            }
        } else {
            if transitions.get_main_animation() != Some(2.into()) {
                transitions.play(&mut player, 2.into(), Duration::ZERO).set_speed(1.).repeat();
            }
        }
    }
}