use bevy::prelude::*;

use crate::*;

pub fn update_animations(
    time: Res<Time>,
    mut query: Query<(&KeyBits, &mut Sprite, &mut AnimationConfig, &mut TextureAtlas)>,
) {
    for (key_bits, mut sprite, mut config, mut atlas) in &mut query {
        config.frame_timer.tick(time.delta());
        if config.frame_timer.just_finished() {
            if atlas.index >= config.opts[config.selected].end || atlas.index < config.opts[config.selected].start { 
                atlas.index = config.opts[config.selected].start; 
            } else {
                atlas.index += 1;
                config.frame_timer = AnimationConfig::timer_from_fps(config.fps);
            }
        }

        let fps = config.fps as f32;
        if *key_bits & (KEYBIT_UP | KEYBIT_DOWN) != default() {
            if *key_bits & KEYBIT_UP != default() && config.selected != 0 { 
                config.selected = 0;
                config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
            } else if *key_bits & KEYBIT_DOWN != default() && config.selected != 1 { 
                config.selected = 1;
                config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
            }
        } else if *key_bits & KEYBIT_LEFT != default() && config.selected != 2 {
            config.selected = 2;
            config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
        } else if *key_bits & KEYBIT_RIGHT != default() && config.selected != 3 {
            config.selected = 3;
            config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
        }
        sprite.flip_x = config.opts[config.selected].flip;
    }
}

pub fn update_transforms(
    mut query: Query<(&mut Transform, &Pos)>,
) {
    for (mut transform, pos) in &mut query {
        transform.translation = pos.into_screen();
    }
}