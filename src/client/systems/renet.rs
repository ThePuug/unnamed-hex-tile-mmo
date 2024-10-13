use bevy::{prelude::*, sprite::Anchor};
use keybits::KeyBits;
use renet::{DefaultChannel, RenetClient};

use crate::{*,
    common::{
        message::{*, Event},
        components::hx::*,
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
                                    translation: (hx, Offset::default()).into_screen(),
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
                            Heading::default(),
                            typ,
                            hx,
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
                                    translation: (hx, Offset::default()).into_screen(),
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
                if let Some(ent) = l2r.0.remove_by_right(&ent) {
                    commands.entity(ent.0).despawn();
                } else {
                    warn!("Player {} not found when Despawn received", ent);
                }
            }
            Do { event: Event::Move { ent, hx, heading } } => {
                if let Some(&ent) = l2r.0.get_by_right(&ent) {
                    writer.send(Do { event: Event::Move { ent, hx, heading } });
                } else {
                    warn!("Player {} not found when Move received", ent);
                }
            }
            _ => {}
        }
    }
}

pub fn send_try(
    mut conn: ResMut<RenetClient>,
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    l2r: Res<EntityMap>,
    mut queue: ResMut<InputQueue>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Move { ent, .. } } => {
                if let Some(key_bits_last) = queue.0.pop() {
                    conn.send_message(DefaultChannel::ReliableOrdered, 
                        bincode::serialize(&Try { event: Event::Input {
                            ent: *l2r.0.get_by_left(&ent).unwrap(), 
                            key_bits: key_bits_last.key_bits, 
                            dt: key_bits_last.dt 
                    }}).unwrap());
                }
            }
            Try { event: Event::Input { ent, key_bits, dt } } => {
                writer.send(Do { event: Event::Input { ent, key_bits, dt } });
                let mut key_bits_last = queue.0.pop().unwrap_or(InputAccumulator { key_bits, dt: 0 });
                if key_bits.key_bits != key_bits_last.key_bits.key_bits
                    || key_bits_last.dt > 1000 {
                    conn.send_message(DefaultChannel::ReliableOrdered, 
                        bincode::serialize(&Try { event: Event::Input { 
                            ent: *l2r.0.get_by_left(&ent).unwrap(), 
                            key_bits: key_bits_last.key_bits, 
                            dt: key_bits_last.dt 
                    }}).unwrap());
                    key_bits_last = InputAccumulator { key_bits, dt: 0 };
                }
                key_bits_last.dt += dt;
                queue.0.push(key_bits_last);                
            }
            Try { event: Event::Discover { ent, hx } } => { 
                conn.send_message(DefaultChannel::ReliableOrdered, 
                    bincode::serialize(&Try { event: Event::Discover { 
                        ent: *l2r.0.get_by_left(&ent).unwrap(), 
                        hx 
                }}).unwrap());
            }
            _ => {}
        }
    }
}