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
    mut query: Query<(&Hx, Option<&Offset>, Option<&Heading>, Option<&KeyBits>, &mut Transform)>,
) {
    for (&hx, offset, heading, keybits, mut transform0) in &mut query {
        let target = match (keybits, offset, heading) {
            (Some(&keybits), Some(&offset), _) if keybits != KeyBits::default() => (hx, offset).calculate(),
            (_, None, Some(&heading)) => (hx, heading.into()).calculate(),
            _ => (hx, Offset::default()).calculate(),
        };
        let transform = transform0.translation.lerp(target,1.-0.01f32.powf(time.delta_seconds()));
        transform0.translation = transform;
    }
}