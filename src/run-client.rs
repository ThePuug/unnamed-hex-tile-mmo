mod common;
mod client;

use std::time::{Duration, SystemTime};
use std::net::UdpSocket;

use bevy::{log::LogPlugin, prelude::*};
use bevy_renet::{
    renet::{
        transport::ClientAuthentication,
        ConnectionConfig, DefaultChannel, RenetClient,
    },
    transport::NetcodeClientPlugin,
    RenetClientPlugin,
};
use renet::transport::{NetcodeClientTransport, NetcodeTransportError};

use common::{
    components::{
        keybits::*, 
        prelude::{Event, *},
    },
    input::*,
};
use client::{
    components::animationconfig::*,
    input::*, 
    resources::*
};

const PROTOCOL_ID: u64 = 7;

fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
    let server_addr = "127.0.0.1:5000".parse().unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();
    let client = RenetClient::new(ConnectionConfig::default());

    (client, transport)
}

fn do_server_events(
    mut conn: ResMut<RenetClient>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut client: ResMut<Client>,
    mut rpcs: ResMut<Rpcs>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let message = bincode::deserialize(&serialized).unwrap();
        trace!("do_server_events: {:?}", message);
        match message {
            Message::Do { event } => {
                match event {
                    Event::Spawn { ent, typ, translation } => {
                        let (texture, layout, texture_atlas_layout);
                        match typ {
                            EntityType::Player => {
                                texture = asset_server.load("sprites/blank.png");
                                layout = TextureAtlasLayout::from_grid(UVec2{x:32,y:44}, 4, 3, None, None);
                                texture_atlas_layout = texture_atlas_layouts.add(layout);
                            }
                        }
                        let loc = commands
                            .spawn((
                                SpriteBundle {
                                    texture,
                                    transform: Transform::from_translation(translation),
                                    ..default()
                                },
                                TextureAtlas {
                                    layout: texture_atlas_layout,
                                    index: 0,
                                },
                                AnimationConfig::new([
                                    AnimationDirection { start:0, end:3, flip:false },
                                    AnimationDirection { start:8, end:11, flip:false },
                                    AnimationDirection { start:4, end:7, flip:false },
                                    AnimationDirection { start:4, end:7, flip:true }],
                                    2,0),
                                KeyBits::default(),
                                Heading::default(),
                            )).id();
                        rpcs.0.insert(ent, loc);
                        if client.ent == None { 
                            client.ent = Some(ent); 
                            debug!("Player {} is the local player", ent);
                        }
                    }
                    Event::Despawn { ent } => {
                        debug!("Player {} disconnected", ent);
                        commands.entity(rpcs.0.remove(&ent).unwrap()).despawn();
                    }
                    Event::Input { ent, key_bits } => {
                        commands.entity(*rpcs.0.get(&ent).unwrap()).insert(key_bits);
                    }
                }
            }
            Message::Try { event } => {
                warn!("Unexpected try event: {:?}", event);
            }
        }
    }
}

fn update_animations(
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

fn panic_on_error_system(
    mut renet_error: EventReader<NetcodeTransportError>
) {
    for e in renet_error.read() {
        panic!("{}", e);
    }
}

fn setup(
    mut commands: Commands,
) {
    commands.spawn(Camera2dBundle::default());
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
            filter:  "wgpu=error,bevy=warn,naga=warn,".to_owned()
                    +"client=info,"
                    +"client::common::input=trace,"
                    ,
            custom_layer: |_| None,
        }),
        RenetClientPlugin,
        NetcodeClientPlugin,
    ));

    app.add_systems(Startup, setup);
    app.add_systems(Update, (
        panic_on_error_system,
        ui_input,
        do_server_events,
        handle_input,
        update_animations,
    ));

    let (client, transport) = new_renet_client();

    app.init_resource::<Client>();
    app.init_resource::<Rpcs>();
    app.insert_resource(client);
    app.insert_resource(transport);

    app.run();
}
