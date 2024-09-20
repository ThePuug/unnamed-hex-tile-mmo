use bevy::{prelude::*, sprite::Anchor};
use renet::{DefaultChannel, RenetClient};

use crate::{*, Event,
    common::{
        hx::*,
        components::message::*,
    },
};

pub fn do_server_events(
    mut commands: Commands,
    mut conn: ResMut<RenetClient>,
    mut events: EventWriter<Event>,
    mut l2r: ResMut<EntityMap>,
    mut map: ResMut<Map>,
    texture_handles: Res<TextureHandles>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let message = bincode::deserialize(&serialized).unwrap();
        match message {
            Message::Do { event } => {
                match event {
                    Event::Spawn { ent, typ, hx } => {
                        let pos = Pos { hx, ..default() };
                        match typ {
                            EntityType::Actor => {
                                let loc = commands.spawn((
                                    SpriteBundle {
                                        texture: texture_handles.actor.0.clone(),
                                        transform: Transform {
                                            translation: pos.into_screen(),
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
                                    KeyBits::default(),
                                    Heading::default(),
                                    typ,
                                    pos,
                                )).id();
                                l2r.0.insert(loc, ent);
                                if l2r.0.len() == 1 {
                                    commands.get_entity(loc).unwrap().insert(Actor);
                                    debug!("Player server:{} is local:{}", ent, loc);
                                }
                            }
                            EntityType::Decorator(desc) => {
                                let loc = commands.spawn((
                                    SpriteBundle {
                                        texture: texture_handles.decorator.0.clone(),
                                        transform: Transform {
                                            scale: Vec3 { x: TILE_SIZE_W / 83., y: TILE_SIZE_H / 83., z: 1. },
                                            translation: pos.into_screen(),
                                            ..default()},
                                        sprite: Sprite {
                                            anchor: Anchor::Custom(Vec2{ x: 0., y: (68.-TILE_SIZE*2.) / 138. }),
                                            ..default()},
                                        ..default()},
                                    TextureAtlas {
                                        layout: texture_handles.decorator.1.clone(),
                                        index: desc.index,
                                        ..default()},
                                    typ,
                                    pos,
                                )).id();
                                map.0.insert(hx, loc);
                            }
                        }
                    }
                    Event::Despawn { ent } => {
                        debug!("Player {} disconnected", ent);
                        commands.entity(l2r.0.remove_by_right(&ent).unwrap().1).despawn();
                    }
                    Event::Input { ent, key_bits, dt } => {
                        events.send(Event::Input { ent: *l2r.0.get_by_right(&ent).unwrap(), key_bits, dt });
                    }
                }
            }
            Message::Try { event } => {
                warn!("Unexpected try event: {:?}", event);
            }
        }
    }
}

pub fn try_events(
    mut conn: ResMut<RenetClient>,
    mut events: EventReader<Event>,
    l2r: Res<EntityMap>,
) {
    for &event in events.read() {
        let message;
        match event {
            Event::Input { ent, key_bits, dt } => {
                message = bincode::serialize(&Message::Try { event: Event::Input { 
                    ent: *l2r.0.get_by_left(&ent).unwrap(), 
                    key_bits, dt }}).unwrap();
                conn.send_message(DefaultChannel::ReliableOrdered, message);
            }
            _ => {
                warn!("Unexpected event: {:?}", event);
            }
        }
    }
}