use bevy::prelude::*;

use crate::{*,
    common::{
        components::hx::*,
        message::{*, Event},
    },
};

pub fn handle_input(
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Entity, &Hx, &mut Offset, &mut Heading, &mut Transform), With<Actor>>,
) {
    if let Ok((ent, &hx0, mut offset0, mut heading0, mut transform0)) = query.get_single_mut() {
        let px = Vec3::from(hx0);
        let curr = px + offset0.0;
        let mut heading = Heading::default();
        if keyboard.any_pressed([KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD,]) {
            if keyboard.any_pressed([KeyCode::KeyW]) {
                if keyboard.any_pressed([KeyCode::KeyA]) || !keyboard.any_pressed([KeyCode::KeyD])
                    &&(heading0.0 == Hx {q:-1, r: 0, z: 0}
                    || heading0.0 == Hx {q:-1, r: 1, z: 0}
                    || heading0.0 == Hx {q: 1, r:-1, z: 0}) { heading = Heading(Hx {q:-1, r: 1, z: 0}); }
                else  { heading = Heading(Hx {q: 0, r: 1, z: 0}); }
            } else if keyboard.any_pressed([KeyCode::KeyS]) {
                if keyboard.any_pressed([KeyCode::KeyD]) || !keyboard.any_pressed([KeyCode::KeyA])
                    &&(heading0.0 == Hx {q: 1, r: 0, z: 0}
                    || heading0.0 == Hx {q: 1, r:-1, z: 0}
                    || heading0.0 == Hx {q:-1, r: 1, z: 0}) { heading = Heading(Hx {q: 1, r: -1, z: 0}); }
                else { heading = Heading(Hx {q: 0, r:-1, z: 0}); }
            } 
            else if keyboard.any_pressed([KeyCode::KeyD]) { heading = Heading(Hx {q: 1, r: 0, z: 0}); }
            else if keyboard.any_pressed([KeyCode::KeyA]) { heading = Heading(Hx {q:-1, r: 0, z: 0}); }
        }

        let target = match heading.0 {
            Hx { q: 0, r: 0, z: 0} => px.lerp(Vec3::from(hx0 + heading0.0),0.25),
            _ => px.lerp(Vec3::from(hx0 + heading.0),1.25),
        };

        let dist = curr.distance(target);
        let ratio = 0_f32.max((dist - 100_f32 * time.delta_seconds()) / dist);
        offset0.0 = curr.lerp(target, 1. - ratio) - px;
        transform0.translation = (hx0, *offset0).into_screen();

        let hx = Hx::from(px + offset0.0);
        if hx != hx0 || heading.0 != Hx::default() && heading.0 != heading0.0 { 
            heading0.0 = heading.0;
            writer.send(Try { event: Event::Move { ent, hx, heading } }); 
        }
    }
}

pub fn camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), (With<Actor>, Without<Hx>, Without<Offset>)>,
    actor: Query<&Transform, (With<Actor>, With<Hx>, With<Offset>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let (mut c_transform, mut projection) = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 24., z: 0. };
        if keyboard.any_pressed([KeyCode::Minus]) { projection.scale *= 1.05; }
        if keyboard.any_pressed([KeyCode::Equal]) { projection.scale /= 1.05; }
    }
}

