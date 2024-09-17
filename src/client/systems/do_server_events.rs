use bevy::prelude::*;
use renet::{DefaultChannel, RenetClient};

use crate::{*,
    common::{
        components::message::{*, Event}, 
        hxpx::*,
        resources::map::*,
    }
};

pub fn do_server_events(
    mut conn: ResMut<RenetClient>,
    mut commands: Commands,
    mut client: ResMut<Client>,
    mut rpcs: ResMut<Rpcs>,
    mut map: ResMut<Map>,
    texture_handles: Res<TextureHandles>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let message = bincode::deserialize(&serialized).unwrap();
        trace!("do_server_events: {:?}", message);
        match message {
            Message::Do { event } => {
                match event {
                    Event::Spawn { ent, typ, translation } => {
                        match typ {
                            EntityType::Actor => {
                                let loc = commands.spawn((SpriteBundle {
                                   texture: texture_handles.actor.0.clone(),
                                   transform: Transform::from_translation(translation),
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
                                )).id();
                                rpcs.0.insert(ent, loc);
                                if client.ent == None { 
                                    client.ent = Some(ent); 
                                    debug!("Player {} is the local player", ent);
                                }
                            }
                            EntityType::Decorator(desc) => {
                                let loc = commands.spawn((
                                    SpriteBundle {
                                        texture: texture_handles.decorator.0.clone(),
                                        transform: Transform::from_translation(translation),
                                        ..default()},
                                    TextureAtlas {
                                        layout: texture_handles.decorator.1.clone(),
                                        index: desc.index,
                                        ..default()},
                                    typ,
                                )).id();
                                map.0.insert(Hx::from(translation), loc);
                            }
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
