use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*
};
use qrz::Convert;

pub const TILE_RISE: f32 = 0.8;
pub const TILE_SIZE: f32 = 1.;

use crate::{
    client::resources::*,
    common::{
        components::{ *,
            offset::*,
        },
        message::{*, Event},
        resources::{*, 
            map::*
        },
        systems::*,
    }
};

pub fn setup(    
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 10.,
    });

    commands.spawn((DirectionalLight {
            shadows_enabled: true,
            ..default()},
        Transform::default(), 
        Sun::default()));
    commands.spawn((DirectionalLight {
            shadows_enabled: false,
            color: Color::WHITE,
            ..default()},
        Transform::default(), 
        Moon::default()));

    let mesh = meshes.add(Extrusion::new(RegularPolygon::new(TILE_SIZE, 6),TILE_RISE));
    let material = materials.add(StandardMaterial {
        base_color: Color::hsl(105., 0.75, 0.1),
        perceptual_roughness: 1.,
        ..default()
    });

    commands.insert_resource(Tmp{mesh, material});
}

pub fn do_init(
    mut reader: EventReader<Do>,
    mut server: ResMut<Server>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Init { dt, .. } } = message {
            server.elapsed_offset = dt;
        }
    }
}

pub fn do_spawn(
    mut commands: Commands,
    query: Query<&EntityType>,
    mut reader: EventReader<Do>,
    mut map: ResMut<Map>,
    tmp: Res<Tmp>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Spawn { qrz, typ: EntityType::Decorator(DecoratorDescriptor { index, is_solid }), .. } } = message {
            if let Some(&loc) = map.get(qrz) {
                match query.get(loc) {
                    Ok(&EntityType::Decorator(DecoratorDescriptor { index: index0, .. })) if index0 == index => { continue; },
                    _ => (),
                }
                commands.entity(loc).despawn();
            }
            let loc = commands.spawn((
                Loc::new(qrz),
                Mesh3d(tmp.mesh.clone()),
                MeshMaterial3d(tmp.material.clone()),
                Transform {
                    translation: map.convert(qrz)+Vec3::Y*TILE_RISE/2.,
                    rotation: Quat::from_rotation_x(-PI/2.),
                    // scale: Vec3::ONE*0.99,
                    ..default()},
                EntityType::Decorator(DecoratorDescriptor { index, is_solid }),
                Offset::default(),
            )).id();
            map.insert(qrz, loc);
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>,Without<Moon>)>,
    mut q_moon: Query<(&mut DirectionalLight, &mut Transform), (With<Moon>,Without<Sun>)>,
    mut a_light: ResMut<AmbientLight>,
    server: Res<Server>,
) {
    let dt = time.elapsed().as_millis() + server.elapsed_offset;
    let dtd = (dt % DAY_MS) as f32 / DAY_MS as f32;
    let dtm = (dt % SEASON_MS) as f32 / SEASON_MS as f32;
    let dty = (dt % YEAR_MS) as f32 / YEAR_MS as f32;

    // sun
    let (mut s_light, mut s_transform) = q_sun.single_mut();
    let mut s_rad_d = dtd * 2. * PI;
    let s_rad_y = dty * 2. * PI;

    // days are longer than nights
    s_rad_d = s_rad_d.clamp(PI/3., 5.*PI/3.);

    let s_illuminance = 1.-cos(0.75*s_rad_d + 3.*PI/4.).powf(16.);
    s_light.color = Color::linear_rgb(1., s_illuminance, s_illuminance);
    s_light.illuminance = 10_000.*s_illuminance;
    a_light.brightness = 100.*s_illuminance;
    s_transform.translation.x = 1_000.*cos(0.75*s_rad_d + 3.*PI/4.);
    s_transform.translation.y = 1_000.*sin(0.75*s_rad_d + 3.*PI/4.).powf(2.);
    s_transform.translation.z = 1_000.*cos(s_rad_y);
    s_transform.look_at(Vec3::ZERO, Vec3::Y);

    // moon
    let (mut m_light, mut m_transform) = q_moon.single_mut();
    let mut m_rad_d = dtd * 2. * PI;
    let m_rad_m = dtm * 2. * PI;

    // overlap sun cycle by PI/6 to avoid no lightsource at dusk/dawn
    if PI/2. < m_rad_d && m_rad_d < 3.*PI/2. { m_rad_d = 3.*PI/2. };

    m_light.illuminance = 200.                  // max illuminance at full moon
        *(0.1+0.9*cos(0.5*m_rad_m).powf(2.))    // phase moon through month
        *(1.-cos(m_rad_d+3.*PI/2.).powf(16.));  // moon rise/fall
    m_transform.translation.x = 1_000.*cos(m_rad_d+3.*PI/2.);
    m_transform.translation.y = 1_000.*sin(m_rad_d+3.*PI/2.).powf(2.);
    m_transform.look_at(Vec3::ZERO, Vec3::Y);
}
