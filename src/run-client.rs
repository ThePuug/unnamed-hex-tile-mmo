#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{
    log::LogPlugin, 
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
        keybits::*, 
        offset::Offset, 
    }, 
    message::{ *, Event }, 
    plugins::nntree, 
    resources::{  *, 
        map::*, 
    }, 
    systems::physics::*
};
use client::{
    resources::*,
    systems::{ *,
        renet::*,
        input::*,
        sprites::*,
    },
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
) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Transform::default(),
        Offset { state: Vec3::new(0., 10., -10.), step: Vec3::ZERO },
        Actor
    ));

    let (_, light_config) = config_store.config_mut::<LightGizmoConfigGroup>();
    light_config.draw_all = false;
    light_config.color = LightGizmoColor::MatchLightColor;

    commands.insert_resource(AmbientLight {
        brightness: 20.,
        ..default()
    });
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
        // HanabiPlugin,
        nntree::NNTreePlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        setup,
    ));

    app.add_systems(PreUpdate, (
        write_do,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        do_input,
        do_incremental,
        generate_input,
        // render_do_gcd,
        try_gcd,
        try_input,
        ready,
        update_camera,
        update_heading,
        update_transforms,
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
