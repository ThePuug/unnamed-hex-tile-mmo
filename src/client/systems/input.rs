use bevy::prelude::*;

use crate::{*,
    common::components::hx::*,
};

pub fn handle_input(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Hx, &mut Offset, &mut Heading), With<Actor>>,
) {
    if let Ok((&hx0, mut offset0, mut heading0)) = query.get_single_mut() {
        let mut heading = *heading0;
        if keyboard.any_pressed([
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
        ])
        {
            if keyboard.any_pressed([KeyCode::ArrowUp]) {
                if keyboard.any_pressed([KeyCode::ArrowLeft]) || !keyboard.any_pressed([KeyCode::ArrowRight])
                    &&(heading.0 == Hx {q:-1, r: 0, z: -1}
                    || heading.0 == Hx {q:-1, r: 1, z: -1}
                    || heading.0 == Hx {q: 1, r:-1, z: -1}) { heading = Heading(Hx {q:-1, r: 1, z: -1}); }
                else  { heading = Heading { 0:Hx {q: 0, r: 1, z: -1} }; }
            } else if keyboard.any_pressed([KeyCode::ArrowDown]) {
                if keyboard.any_pressed([KeyCode::ArrowRight]) || !keyboard.any_pressed([KeyCode::ArrowLeft])
                    &&(heading.0 == Hx {q: 1, r: 0, z: -1}
                    || heading.0 == Hx {q: 1, r:-1, z: -1}
                    || heading.0 == Hx {q:-1, r: 1, z: -1}) { heading = Heading { 0:Hx {q: 1, r: -1, z: -1} }; }
                else { heading = Heading { 0:Hx {q: 0, r:-1, z: -1} }; }
            } 
            else if keyboard.any_pressed([KeyCode::ArrowRight]) { heading = Heading { 0:Hx {q: 1, r: 0, z: -1} }; }
            else if keyboard.any_pressed([KeyCode::ArrowLeft]) { heading = Heading { 0:Hx {q:-1, r: 0, z: -1} }; }
        
            let target = hx0 + heading.0;
            let px = Vec3::from(hx0);
            let delta = Vec3::from(target).xy() - (px + offset0.0).xy();
            let offset = Offset(offset0.0 + (delta.normalize_or_zero() * 100. * time.delta_seconds()).extend(0.));

            if heading0.0 != heading.0 { *heading0 = heading };
            if offset0.0 != offset.0 { *offset0 = offset };
        }
    }
}

pub fn camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), (With<Actor>, Without<Hx>, Without<Offset>)>,
    actor: Query<&Transform, (With<Hx>, With<Offset>, With<Actor>)>,
) {
    if let Ok(a_transform) = actor.get_single() {
        let (mut c_transform, mut projection) = camera.single_mut();
        c_transform.translation = a_transform.translation + Vec3 { x: 0., y: 24., z: 0. };
        if keyboard.any_pressed([KeyCode::Minus]) { projection.scale *= 1.05; }
        if keyboard.any_pressed([KeyCode::Equal]) { projection.scale /= 1.05; }
    }
}

