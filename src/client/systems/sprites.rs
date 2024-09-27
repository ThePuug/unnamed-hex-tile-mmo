use std::time::Duration;

use bevy::prelude::*;

use crate::{ *,
    common::components::{
        hx::*,
        keybits::*,
    },
};

pub fn update_animations(
    time: Res<Time>,
    mut query: Query<(&Heading, &mut Sprite, &mut AnimationConfig, &mut TextureAtlas)>,
) {
    for (heading, mut sprite, mut config, mut atlas) in &mut query {
        config.frame_timer.tick(time.delta());
        if config.frame_timer.just_finished() {
            let opt = &config.opts[config.selected];
            if atlas.index >= opt.end || atlas.index < opt.start { 
                atlas.index = opt.start + (atlas.index+1) % (1 + opt.end - opt.start);
            } else {
                atlas.index += 1;
                config.frame_timer = AnimationConfig::timer_from_fps(config.fps);
            }
        }

        let fps = config.fps as f32;
        let selected0 = config.selected;
        config.selected = match heading.0 {
            Hx { q: _, r: 1, z: _ } => 0,
            Hx { q: _, r: -1, z: _ } => 1,
            Hx { q: -1, r: 0, z: _ } => 2,
            Hx { q: 1, r: 0, z: _ } => 3,
            _ => config.selected,
        };
        if selected0 != config.selected { config.frame_timer.set_elapsed(Duration::from_secs_f32(1. / fps)); }

        sprite.flip_x = config.opts[config.selected].flip;
    }
}

pub fn update_transforms(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Hx, &Heading, &mut Offset, Option<&KeyBits>)>,
) {
    for (mut transform, &hx, &heading, mut offset0, key_bits) in &mut query {
        let px = Vec3::from(hx);
        let target = px.lerp(Vec3::from(hx + heading.0),
            if key_bits.is_some() && key_bits.unwrap().any_pressed([KB_HEADING_Q, KB_HEADING_R]) { 1.25 }
            else { 0.25 });
        let dist = (px + offset0.0).distance(target);
        let ratio = 0_f32.max((dist - 100. * time.delta_seconds()) / dist);
        offset0.0 = offset0.0.lerp(target - px, 1. - ratio);
        transform.translation = (hx, *offset0).into_screen();
    }
}
