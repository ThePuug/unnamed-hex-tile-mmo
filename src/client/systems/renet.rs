use std::time::Duration;

use bevy::{
    color::palettes::css::*, pbr::VolumetricLight, prelude::*
};
use bevy_renet::netcode::ClientAuthentication;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::resources::*, 
    common::{
        components::{
            heading::*, 
            hx::*, 
            keybits::*, 
            offset::*
        }, 
        message::{Event, *}, 
        resources::*
    }, *
};

pub fn setup() -> (RenetClient, NetcodeClientTransport) {
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

pub fn ready(
    mut commands: Commands,
    mut query: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    for (ent, mut player) in &mut query {
        debug!("ready {ent}");
        let (graph, animation) = AnimationGraph::from_clip(
            asset_server.load(GltfAssetLabel::Animation(2).from_asset("models/actor-baby.glb")));
        let handle = graphs.add(graph);
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, animation, Duration::ZERO).set_speed(1.5).repeat();
        commands.entity(ent)
            .insert(AnimationGraphHandle(handle))
            .insert(transitions);
    }
}

pub fn write_do(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut map: ResMut<Map>,
    mut queue: ResMut<InputQueue>,
    asset_server: Res<AssetServer>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
        match message {
            Do { event: Event::Spawn { ent, typ, hx } } => {
                match typ {
                    EntityType::Actor => {
                        let loc = commands.spawn((
                            SceneRoot(asset_server.load(
                                GltfAssetLabel::Scene(0).from_asset("models/actor-baby.glb"),
                            )),
                            Transform {
                                translation: hx.into(),
                                scale: Vec3::ONE * TILE_SIZE,
                                ..default()},
                            typ,
                            hx,
                            AirTime { state: Some(0), step: None },
                            Heading::default(),
                            Offset::default(),
                            KeyBits::default(),
                            Visibility::default(),
                        )).with_children(|builder| {
                            builder.spawn((PointLight {
                                    radius: 100.,
                                    color: RED.into(),
                                    intensity: 100_000.,
                                    shadows_enabled: true,
                                    ..default()},
                                Transform::from_xyz(100., 100., 100.),
                                VolumetricLight,
                            ));
                        }).id();
                        l2r.0.insert(loc, ent);
                        if l2r.0.len() == 1 {
                            commands.get_entity(loc).unwrap().insert(Actor);
                        }
                    }
                    EntityType::Decorator(_desc) => {
                        let loc = map.remove(hx);
                        if loc != Entity::PLACEHOLDER { commands.entity(loc).despawn(); }
                        let loc = commands.spawn((
                            SceneRoot(asset_server.load(
                                GltfAssetLabel::Scene(0).from_asset("models/hex-block-stone-grey.glb"),
                            )),
                            Transform {
                                translation: hx.into(),
                                scale: Vec3::ONE * TILE_SIZE*2.,
                                ..default()},
                            typ,
                            hx,
                            Offset::default(),
                        )).id();
                        map.insert(hx, loc);
                    }
                }
            }
            Do { event: Event::Despawn { ent } } => {
                debug!("Player {} disconnected", ent);
                let ent = l2r.0.remove_by_right(&ent).unwrap();
                commands.entity(ent.0).despawn();
            }
            Do { event: Event::Incremental { ent, attr } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Incremental { ent, attr } });
            }
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                queue.0.pop_back();
                writer.send(Do { event: Event::Input { ent, key_bits, dt, seq } });
            }
            Do { event: Event::Gcd { ent, typ } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Gcd { ent, typ } });
            }
            _ => {}
        }
    }
}

pub fn send_try(
    mut conn: ResMut<RenetClient>,
    mut reader: EventReader<Try>,
    l2r: Res<EntityMap>,
    mut queue: ResMut<InputQueue>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, mut dt, mut seq } } => {
                if seq != 0 { continue; } // seq 0 is input this frame
                let input0 = queue.0.pop_front().unwrap();
                match input0 {
                    Event::Input { key_bits: key_bits0, dt: mut dt0, seq: seq0, .. } => {
                        // the longer dt0 check is, the faster continuous motion desyncs
                        if key_bits.key_bits != key_bits0.key_bits || dt0 > 1000 { 
                            queue.0.push_front(input0);
                            seq = if seq0 == 255 { 1 } else { seq0 + 1}; dt0 = 0;
                            conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Input { 
                                ent: *l2r.0.get_by_left(&ent).unwrap(), 
                                key_bits, dt: 0, seq,
                            } }, bincode::config::legacy()).unwrap());
                        }
                        dt += dt0;
                        queue.0.push_front(Event::Input { ent, key_bits, dt, seq });
                    }
                    _ => unreachable!()
                };
            }
            Try { event: Event::Gcd { ent, typ, .. } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Gcd { 
                    ent: *l2r.0.get_by_left(&ent).unwrap(), 
                    typ,
                } }, bincode::config::legacy()).unwrap());
            }
            _ => {}
        }
    }
}