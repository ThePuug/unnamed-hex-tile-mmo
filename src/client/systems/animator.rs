use std::time::Duration;

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{client::components::*,
    common::{
        components::{ heading::*, offset::*, position::VisualPosition, * },
        resources::{map::Map, *}
    }
};

pub fn update(
    query: Query<(Entity, &Loc, &Offset, &Heading, &Transform, &AirTime, &Animates, Option<&VisualPosition>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    for (entity, &loc, &offset, &heading, &transform, &airtime, &animates, vis_pos) in &query {
        let is_moving = if let Some(vis) = vis_pos {
            // ADR-019: Entity is moving if VisualPosition is actively interpolating
            !vis.is_complete() && vis.from.distance_squared(vis.to) > 0.001
        } else {
            // Legacy fallback: calculate target and compare to current position
            let is_local_player = buffers.get(&entity).is_some();
            let target = match (is_local_player, offset, heading) {
                (true, offset, _) if offset.step.length_squared() > 0.01 => map.convert(*loc) + offset.step,
                (_, _, heading) if *heading != Qrz::default() => {
                    let dir = map.convert(*loc + *heading) - map.convert(*loc);
                    map.convert(*loc) + dir * HERE
                },
                _ => map.convert(*loc),
            };
            transform.translation.distance_squared(target) > 0.0
        };

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