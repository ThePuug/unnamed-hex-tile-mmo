use bevy::prelude::*;

use crate::*;

pub fn update_animations(
    time: Res<Time>,
    mut query: Query<(&mut AnimationConfig, &mut TextureAtlas, &mut Sprite, &KeyBits)>,
) {
    for (mut config, mut atlas, mut sprite, keys) in &mut query {
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
        if *keys & (KEYBIT_UP | KEYBIT_DOWN) != default() {
            if *keys & KEYBIT_UP != default() && config.selected != 0 { 
                config.selected = 0;
                config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
            } else if *keys & KEYBIT_DOWN != default() && config.selected != 1 { 
                config.selected = 1;
                config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
            }
        } else if *keys & KEYBIT_LEFT != default() && config.selected != 2 {
            config.selected = 2;
            config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
        } else if *keys & KEYBIT_RIGHT != default() && config.selected != 3 {
            config.selected = 3;
            config.frame_timer.set_elapsed(Duration::from_secs_f32(1./fps));
        }
        sprite.flip_x = config.opts[config.selected].flip;
    }
}