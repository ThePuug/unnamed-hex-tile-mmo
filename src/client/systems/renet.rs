use std::{
    f32::consts::PI, 
    time::Duration
};

use bevy::prelude::*;
use bevy_renet::netcode::ClientAuthentication;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::{
        components::*,
        resources::*, 
    },
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
    mut query: Query<(Entity, &mut AnimationPlayer, &Parent), Added<AnimationPlayer>>,
    q_prnt: Query<&Parent>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    for (ent, mut player, scene) in &mut query {
        let parent = q_prnt.get(scene.get()).unwrap().get();
        commands.entity(parent).insert(Animator(ent));
        let (graph, _) = AnimationGraph::from_clips([
            asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/actor-baby.glb")),
            asset_server.load(GltfAssetLabel::Animation(1).from_asset("models/actor-baby.glb")),
            asset_server.load(GltfAssetLabel::Animation(2).from_asset("models/actor-baby.glb"))]);
        let handle = AnimationGraphHandle(graphs.add(graph));
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, 2.into(), Duration::ZERO).set_speed(1.).repeat();
        commands.entity(ent)
            .insert(handle)
            .insert(transitions);
    }
}

pub fn write_do(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut map: ResMut<Map>,
    mut buffer: ResMut<InputQueue>,
    mut server: ResMut<Server>,
    asset_server: Res<AssetServer>,
    tmp: Res<Tmp>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
        match message {
            Do { event: Event::Init { ent, dt }} => {
                debug!("Player {ent} connected, time offset: {dt}");
                server.elapsed_offset = dt;
                let loc = commands.spawn(Actor).id();
                l2r.0.insert(loc, ent);
            }
            Do { event: Event::Spawn { ent, typ, hx } } => {
                match typ {
                    EntityType::Actor => {
                        let mut loc = 
                            if let Some(loc) = l2r.0.get_by_right(&ent) { commands.get_entity(*loc).unwrap() } 
                            else { 
                                let loc = commands.spawn_empty();
                                l2r.0.insert(loc.id(), ent);
                                loc
                            };
                        loc.insert((
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
                        ));
                    }
                    EntityType::Decorator(_desc) => {
                        let loc = map.remove(hx);
                        if loc != Entity::PLACEHOLDER { commands.entity(loc).despawn(); }
                        let loc = commands.spawn((
                            Mesh3d(tmp.mesh.clone()),
                            MeshMaterial3d(tmp.material.clone()),
                            Transform {
                                translation: Vec3::from(hx)+Vec3::Y*TILE_RISE/2.,
                                rotation: Quat::from_rotation_x(-PI/2.),
                                // scale: Vec3::ONE*0.99,
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
                let (ent, _) = l2r.0.remove_by_right(&ent).unwrap();
                debug!("Player {ent} disconnected");
                commands.entity(ent).despawn();
            }
            Do { event: Event::Incremental { ent, attr } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Incremental { ent, attr } });
            }
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                if let Some(Event::Input { seq: seq0, dt: dt0, key_bits: key_bits0, .. }) = buffer.queue.pop_back() {
                    assert_eq!(seq0, seq);
                    assert_eq!(key_bits0, key_bits);
                    if (dt0 as i16 - dt as i16).abs() >= 100 { warn!("{dt0} !~ {dt}"); }
                    if buffer.queue.len() > 2 { warn!("long input queue, len: {}", buffer.queue.len()); }
                } else { unreachable!(); }
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
    mut buffer: ResMut<InputQueue>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Input { ent, key_bits, mut dt, mut seq } } => {
                // seq 0 is input this frame which we should handle
                // other received Input events are being replayed and should be ignored
                if seq != 0 { continue; }

                let input0 = buffer.queue.pop_front().unwrap();
                match input0 {
                    Event::Input { key_bits: key_bits0, dt: mut dt0, seq: seq0, .. } => {
                        seq = seq0;
                        if key_bits.key_bits != key_bits0.key_bits || dt0 > 1000 { 
                            buffer.queue.push_front(input0);
                            seq = if seq0 == 255 { 1 } else { seq0 + 1}; 
                            dt0 = 0;
                            conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Input { 
                                ent: *l2r.0.get_by_left(&ent).unwrap(), 
                                key_bits, dt: 0, seq,
                            }}, bincode::config::legacy()).unwrap());
                        }
                        dt += dt0;
                        buffer.queue.push_front(Event::Input { ent, key_bits, dt, seq });
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