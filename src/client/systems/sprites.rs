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
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &mut Transform, &Hx, &Offset, &Heading), Changed<Offset>>,
) {
    for (ent, mut transform, &hx0, &offset, &heading) in &mut query {
        transform.translation = (hx0, offset).into_screen();
        let hx = Hx::from(Vec3::from(hx0) + offset.0);
        if hx != hx0 {
            writer.send(Try { event: Event::Move { ent, hx, heading } });
            writer.send(Try { event: Event::Discover { hx } });
        }
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading), Changed<Heading>>,
) {
    for (ent, &hx, &heading) in &mut query {
        writer.send(Try { event: Event::Move { ent, hx, heading } });
        writer.send(Try { event: Event::Discover { hx } });
    }
}

pub fn update_positions(
    mut reader: EventReader<Do>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Move { ent, hx, heading } } => {
                trace!("Move: {:?} {:?} {:?}", ent, hx, heading);
                if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                    *hx0 = hx; 
                    *offset0 = Offset::default(); // Offset(offset0.0 - Vec3::from(hx));
                    *heading0 = heading;
                }
            }
            _ => {}
        }
    }
}