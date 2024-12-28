use bevy::{prelude::*, sprite::Anchor};
use keybits::KeyBits;
use renet::{DefaultChannel, RenetClient};

use crate::{*,
    common::{
        message::{*, Event},
        components::{
            heading::*,
            hx::*,
            offset::*,
        },
        resources::*,
    },
    client::resources::*,
};

pub fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
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

pub fn write_do(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut map: ResMut<Map>,
    mut queue: ResMut<InputQueue>,
    texture_handles: Res<TextureHandles>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let message = bincode::deserialize(&serialized).unwrap();
        match message {
            Do { event: Event::Spawn { ent, typ, hx } } => {
                match typ {
                    EntityType::Actor => {
                        let loc = commands.spawn((
                            SpriteBundle {
                                texture: texture_handles.actor.0.clone(),
                                transform: Transform {
                                    translation: (hx, Vec3::ZERO).calculate(),
                                    ..default()},
                                sprite: Sprite {
                                    anchor: Anchor::BottomCenter,
                                    ..default()},
                                ..default()},
                            TextureAtlas {
                                layout: texture_handles.actor.1.clone(),
                                index: 0,
                            },
                            AnimationConfig::new([
                                AnimationDirection { start:8, end:11, flip:false },
                                AnimationDirection { start:0, end:3, flip:false },
                                AnimationDirection { start:4, end:7, flip:false },
                                AnimationDirection { start:4, end:7, flip:true }],
                                2,0),
                            typ,
                            hx,
                            AirTime { state: Some(0), step: None },
                            Heading::default(),
                            Offset::default(),
                            KeyBits::default(),
                        )).id();
                        l2r.0.insert(loc, ent);
                        if l2r.0.len() == 1 {
                            commands.get_entity(loc).unwrap().insert(Actor);
                        }
                    }
                    EntityType::Decorator(desc) => {
                        let loc = map.remove(hx);
                        if loc != Entity::PLACEHOLDER { commands.entity(loc).despawn(); }
                        let loc = commands.spawn((
                            SpriteBundle {
                                texture: texture_handles.decorator.0.clone(),
                                transform: Transform {
                                    scale: Vec3 { x: TILE_SIZE_W / 83., y: TILE_SIZE_H / 96., z: 1. },
                                    translation: (hx, Vec3::ZERO).calculate(),
                                    ..default()},
                                sprite: Sprite {
                                    anchor: Anchor::Custom(Vec2{ x: 0., y: (48.-69.) / 138. }),
                                    ..default()},
                                ..default()},
                            TextureAtlas {
                                layout: texture_handles.decorator.1.clone(),
                                index: desc.index,
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
            Do { event: Event::Move { ent, hx, heading } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Move { ent, hx, heading } });
            }
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let &ent = l2r.0.get_by_right(&ent).unwrap();
                queue.0.pop_back();
                writer.send(Do { event: Event::Input { ent, key_bits, dt, seq } });
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
                        if key_bits.key_bits != key_bits0.key_bits || dt0 > 250 {
                            queue.0.push_front(input0);
                            seq = if seq0 == 255 { 1 } else { seq0 + 1}; dt0 = 0;
                            conn.send_message(DefaultChannel::ReliableOrdered, bincode::serialize(&Try { event: Event::Input { 
                                ent: *l2r.0.get_by_left(&ent).unwrap(), 
                                key_bits, dt: 0, seq,
                            } }).unwrap());
                        }
                        dt += dt0;
                        queue.0.push_front(Event::Input { ent, key_bits, dt, seq });
                    }
                    _ => unreachable!()
                };
            }
            _ => {}
        }
    }
}