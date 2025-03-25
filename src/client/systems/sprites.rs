use std::time::Duration;

use bevy::prelude::*;

use crate::{
    client::components::*,
    common::{
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        message::{*, Event},
    },
};

pub fn update_transforms(
    time: Res<Time>,
    mut query: Query<(&Hx, &Offset, &Heading, &KeyBits, &mut Transform, &Animator)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (&hx, &offset, &heading, &keybits, mut transform0, animator) in &mut query {
        let target = match (keybits, offset, heading) {
            (keybits, offset, _) if keybits != KeyBits::default() => Vec3::from(hx) + offset.step,
            (_, _, heading) => Vec3::from(hx) + Vec3::from(heading.0) * HERE,
        };
        transform0.translation = transform0.translation.lerp(target,1.-0.05f32.powf(time.delta_secs()));
        transform0.rotation = heading.into();

        if let Ok((mut player, mut transitions)) = q_anim.get_mut(animator.0) {
            let dist = transform0.translation.distance_squared(target);
            if dist > 0.2 {
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
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            debug!("try gcd {ent} {typ:?}");
            writer.send(Do { event: Event::Gcd { ent, typ }});
        }
    }
}