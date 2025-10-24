use bevy::prelude::*;
use bevy_behave::prelude::*;

use crate::{ 
    common::{
        components::{ behaviour::*, entity_type::*, heading::*, * }, 
        message::{Event, *}, 
        plugins::nntree::*, 
        systems::gcd::*
    }, 
    server::systems::behaviour::*, *
};

pub fn try_input(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        let Try { event } = message;
        let Event::Input { .. } = event else { continue };
        writer.write(Do { event });
    }
}

pub fn send_input(
    lobby: Res<Lobby>,
    mut conn: ResMut<RenetServer>,
    mut buffers: ResMut<InputQueues>,
) {
    let entities_to_send: Vec<Entity> = buffers.entities().copied().collect();
    
    for ent in entities_to_send {
        let Some(buffer) = buffers.get_mut(&ent) else { continue };

        // Queue invariant: all queues must have at least 1 input
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");

        while buffer.queue.len() > 1 {
            let event = buffer.queue.pop_back().unwrap();
            let message = bincode::serde::encode_to_vec(
                Do { event },
                bincode::config::legacy()).unwrap();
            conn.send_message(*lobby.get_by_right(&ent).unwrap(), DefaultChannel::ReliableOrdered, message);
        }

        // Queue invariant maintained: exactly 1 input remaining (the accumulating one)
        assert_eq!(buffer.queue.len(), 1, "Queue must have exactly 1 input after sending confirmations");
    }
}

pub fn try_gcd(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    query: Query<(&Loc, &Heading)>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ, .. } } = message {
            match typ {
                GcdType::Spawn(typ) => {
                    let (&loc, &heading) = query.get(ent).expect(&format!("missing loc/heading for entity {ent}"));
                    let ent = match typ {
                        EntityType::Actor(_) => {
                            let qrz = *loc + *heading;
                            let loc = Loc::new(qrz);
                            let ent = commands.spawn((
                                typ,
                                loc,
                                children![(
                                    Name::new("curious behaviour"),
                                    BehaveTree::new(behave! {
                                        Behave::Forever => {
                                            Behave::Sequence => {
                                                Behave::spawn_named(
                                                    "find something interesting", 
                                                    FindSomethingInterestingWithin { dist: 20 }),
                                                Behave::spawn_named(
                                                    "path to target", 
                                                    PathTo::default()),
                                                Behave::Wait(5.),
                                    }}})
                                )],
                            )).id();
                            commands.entity(ent).insert(NearestNeighbor::new(ent, loc));
                            ent
                        },
                        _ => Entity::PLACEHOLDER,
                    };
                    writer.write(Do { event: Event::Spawn { ent, typ, qrz: *loc + *heading }});
                }
                _ => unreachable!()
            }
        }
    }
}
