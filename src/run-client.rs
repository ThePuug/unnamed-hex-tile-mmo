#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::{f32::consts::PI, time::SystemTime};
use std::net::UdpSocket;

use bevy::{
    log::LogPlugin, 
    math::ops::*, 
    pbr::*, 
    prelude::* 
};
use bevy_easings::*;
use bevy_renet::{
    renet::ConnectionConfig,
    netcode::{NetcodeClientPlugin, NetcodeClientTransport, NetcodeTransportError},
    RenetClientPlugin,
};

use common::{
    components::{ *, 
        hx::*, 
        keybits::*, 
        offset::Offset,
    }, 
    message::{ Event, * }, 
    plugins::nntree, 
    resources::{ *, 
        map::*, 
    }, 
    systems::physics::*
};
use client::{
    resources::*, 
    systems::{ *,
        assets, 
        input::*, 
        renet::*,
    }
};

const PROTOCOL_ID: u64 = 7;

fn panic_on_error_system(
    mut renet_error: EventReader<NetcodeTransportError>
) {
    if let Some(e) = renet_error.read().next() {
        panic!("{:?}", e);
    }
}

fn setup(
    mut commands: Commands,
    mut config_store: ResMut<GizmoConfigStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Transform::default(),
        Offset { state: Vec3::new(0., 10., -20.), step: Vec3::ZERO },
        Actor
    ));

    let (_, light_config) = config_store.config_mut::<LightGizmoConfigGroup>();
    light_config.draw_all = false;
    light_config.color = LightGizmoColor::MatchLightColor;

    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.,
    });

    commands.spawn((DirectionalLight::default(), Transform::default(), Sun::default()));
    commands.spawn((DirectionalLight::default(), Transform::default(), Moon::default()));

    let mesh = meshes.add(Extrusion::new(RegularPolygon::new(TILE_SIZE, 6),TILE_RISE));
    // let material = materials.add(Color::hsl(90., 0.3, 0.7));
    let material = materials.add(StandardMaterial {
        base_color: Color::hsl(105., 0.75, 0.1),
        perceptual_roughness: 1.,
        ..default()
    });

    commands.insert_resource(Tmp{mesh, material});
}

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins
        .set(AssetPlugin {
            file_path: "../assets".into(),
            ..default()
        })
        .set(LogPlugin {
            level: bevy::log::Level::TRACE,
            filter:  "wgpu=error,bevy=warn,naga=warn,polling=warn,winit=warn,offset_allocator=warn,gilrs=warn,".to_owned()
                    +"client=trace,"
                    ,
            custom_layer: |_| None,
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
        EasingsPlugin::default(),
        nntree::NNTreePlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        assets::setup,
        setup,
    ));

    app.add_systems(PreUpdate, (
        write_do,
    ));

    app.add_systems(FixedUpdate, (
        do_input,
        do_incremental,
        update_sun,
        ready,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        assets::try_gcd,
        assets::update_transforms,
        generate_input,
        try_input,
        update_camera,
        update_heading,
        update_keybits,
    ));

    app.add_systems(PostUpdate, (
        send_try,
    ));

    let (client, transport) = renet::setup();
    app.insert_resource(client);
    app.insert_resource(transport);
    
    let mut buffer = InputQueue::default();
    buffer.queue.push_front(Event::Input { ent: Entity::PLACEHOLDER, key_bits: KeyBits::default(), dt: 0, seq: 1 });
    app.insert_resource(buffer);

    app.init_resource::<EntityMap>();
    app.init_resource::<Map>();

    app.run();
}
