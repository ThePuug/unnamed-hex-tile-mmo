use std::time::Duration;

use bevy::prelude::*;

use crate::{client::components::*,
    common::components::{ position::VisualPosition, * },
};

pub fn update(
    query: Query<(Entity, &AirTime, &Animates, &VisualPosition)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (entity, &airtime, &animates, vis_pos) in &query {
        // ADR-019: Entity is moving if VisualPosition is actively interpolating
        let is_moving = !vis_pos.is_complete() && vis_pos.from.distance_squared(vis_pos.to) > 0.001;

        let (mut player, mut transitions) = q_anim.get_mut(animates.0).unwrap();
        if is_moving || airtime.step.is_some() {
            if transitions.get_main_animation() != Some(3.into()) {
                transitions.play(&mut player, 3.into(), Duration::from_millis(300)).set_speed(1.).repeat();                            
            }
        } else if transitions.get_main_animation() != Some(2.into()) {
            transitions.play(&mut player, 2.into(), Duration::from_millis(300)).set_speed(1.).repeat();
        }
    }
}