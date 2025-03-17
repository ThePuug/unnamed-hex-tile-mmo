#![feature(let_chains)]
#![feature(more_float_constants)]

mod common;
mod client;

use std::time::SystemTime;
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_hanabi::prelude::*;
use bevy_easings::*;
use bevy_renet::{
    renet::ConnectionConfig,
    netcode::{NetcodeClientPlugin, NetcodeClientTransport, NetcodeTransportError},
    RenetClientPlugin,
};

use common::{
    message::{ *, Event },
    components::{ *, keybits::* },
    resources::{ *, map::* },
    systems::physics::*
};
use client::{
    components::animationconfig::*,
    resources::*,
    systems::{ *,
        renet::*,
        input::*,
        sprites::*,
        effect::*,
    },
};
use kiddo::fixed::kdtree::KdTree;

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
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Camera2d,
        Actor
    ));
    commands.insert_resource(TextureHandles {
        actor: (
            asset_server.load("sprites/blank.png"),
            texture_atlas_layouts.add(TextureAtlasLayout::from_grid(UVec2{x:32,y:44}, 4, 3, None, None))
        ),
        decorator: (
            asset_server.load("sprites/biomes.png"),
            texture_atlas_layouts.add(TextureAtlasLayout::from_grid(UVec2{x:83,y:136}, 7, 1, None, None))
        ),
    });
}

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins
        .set(ImagePlugin::default_nearest())
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
        HanabiPlugin,
    ));

    app.add_event::<Do>();
    app.add_event::<Try>();

    app.add_systems(Startup, (
        setup,
        effect::setup,
    ));

    app.add_systems(PreUpdate, (
        write_do,
    ));

    app.add_systems(Update, (
        panic_on_error_system,
        do_input,
        do_incremental,
        generate_input,
        render_do_gcd,
        try_gcd,
        try_input,
        update_animations,
        update_camera,
        update_headings,
        update_offsets,
        update_transforms,
        update_keybits,
    ));

    app.add_systems(PostUpdate, (
        send_try,
    ));

    let (client, transport) = renet::setup();
    app.insert_resource(client);
    app.insert_resource(transport);
    
    let mut queue = InputQueue::default();
    queue.0.push_front(Event::Input { ent: Entity::PLACEHOLDER, key_bits: KeyBits::default(), dt: 0, seq: 1 });
    app.insert_resource(queue);

    let kdtree = NNTree { 0: KdTree::with_capacity(1_000_000) };
    app.insert_resource(kdtree);

    app.init_resource::<EffectMap>();
    app.init_resource::<EntityMap>();
    app.init_resource::<Map>();

    app.run();
}
