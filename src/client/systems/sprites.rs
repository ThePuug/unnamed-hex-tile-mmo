use std::time::Duration;

use bevy::prelude::*;

use crate::{ *,
    common::{
        message::{*, Event},
        components::hx::*,
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
    mut query: Query<(&mut Transform, &Hx, &Heading, &mut Offset), Without<Actor>>,
) {
    for (mut transform, &hx, &heading, mut offset0) in &mut query {
        let px = Vec3::from(hx);
        let curr = px + offset0.0;
        let target = px.lerp(Vec3::from(hx + heading.0),0.25);
        let dist = curr.distance(target);
        let ratio = 0_f32.max((dist - 100. * time.delta_seconds()) / dist);
        offset0.0 = offset0.0.lerp(target - px, 1. - ratio);
        transform.translation = (hx, *offset0).into_screen();
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading), Changed<Heading>>,
) {
    for (_ent, &hx, &heading) in &mut query {
        writer.send(Try { event: Event::Discover { hx: hx + heading.0 + Hx { q: 0, r: 0, z: -1 } } });
    }
}

pub fn update_positions(
    mut reader: EventReader<Do>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Move { ent, hx, heading } } => {
                if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                    *offset0 = Offset(Vec3::from(*hx0) + offset0.0 - Vec3::from(hx));
                    *hx0 = hx; 
                    *heading0 = heading;
                }
            }
            _ => {}
        }
    }
}