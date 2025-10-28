use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{
    common::{
        components::{
            entity_type::{ decorator::*, *},
            heading::Heading,
            offset::Offset, *
        },
        message::{Component, Event, *},
        resources::map::*,
    },
    server::resources::terrain::*
};

/// Server-side system: Generates Try::Discover events when the server authoritatively changes an entity's Loc or Heading
/// This replaces the client-driven discovery to enforce server authority
/// The existing try_discover system will handle these Try events and respond with spawn events
pub fn do_incremental(
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
    query: Query<(&Loc, &Heading)>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        match component {
            Component::Loc(loc) => {
                // When server authoritatively updates Loc, generate discovery for the new tile
                writer.write(Try { event: Event::Discover { ent, qrz: *loc } });

                // Also generate discoveries for FOV based on current heading
                if let Ok((_, heading)) = query.get(ent) {
                    if **heading != default() {
                        for qrz in loc.fov(&heading, 10) {
                            writer.write(Try { event: Event::Discover { ent, qrz } });
                        }
                    }
                }
            }
            Component::Heading(heading) => {
                // When server authoritatively updates Heading, generate discoveries for new FOV
                if *heading != default() {
                    if let Ok((loc, _)) = query.get(ent) {
                        for qrz in loc.fov(&heading, 10) {
                            writer.write(Try { event: Event::Discover { ent, qrz } });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn try_discover(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    mut map: ResMut<Map>,
    terrain: Res<Terrain>,
    query: Query<(&Loc, &EntityType)>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Discover { ent, qrz } } = message {
            let (&loc, _) = query.get(ent).unwrap();
            if loc.flat_distance(&qrz) > 25 { continue; }
            if let Some((qrz, typ)) = map.find(qrz + Qrz{q:0,r:0,z:30}, -60) {
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            } else {
                let px = map.convert(qrz).xy();
                let qrz = Qrz { q:qrz.q, r:qrz.r, z:terrain.get(px.x, px.y)};
                let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
                map.insert(qrz, typ);
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            }
        }
    }
}

pub fn update(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Loc, &Offset), Changed<Offset>>,
    map: Res<Map>,
) {
    for (ent, &loc0, &offset) in &mut query {
        let px = map.convert(*loc0);
        let qrz = map.convert(px + offset.state);
        if *loc0 != qrz {
            let loc = Loc::new(qrz);
            let component = Component::Loc(loc);
            writer.write(Try { event: Event::Incremental { ent, component } });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::{App, Update};
    use qrz::Qrz;
    use crate::common::components::heading::Heading;
    use crate::common::components::entity_type::actor::*;

    #[test]
    fn test_server_discovers_tiles_on_authoritative_loc_change() {
        // Setup
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));
        app.insert_resource(Terrain::default());
        app.init_resource::<crate::common::resources::InputQueues>();

        app.add_systems(Update, (
            crate::common::systems::world::try_incremental,
            crate::common::systems::world::do_incremental,
            do_incremental.after(crate::common::systems::world::do_incremental),
            try_discover,
        ));

        // Create a player entity
        let player = app.world_mut().spawn((
            Loc::new(Qrz { q: 0, r: 0, z: 0 }),
            Heading::new(Qrz { q: 0, r: 1, z: 0 }), // south-east direction (valid DIRECTIONS entry)
            Offset::default(),
            EntityType::Actor(ActorImpl::new(Origin::Natureborn, Approach::Direct, Resilience::Vital)),
        )).id();

        app.update();

        // Clear any initial discovery events
        app.world_mut().resource_mut::<Events<Try>>().clear();

        // Act: Server changes player's Loc (simulating authoritative position update)
        app.world_mut().send_event(Try {
            event: Event::Incremental {
                ent: player,
                component: Component::Loc(Loc::new(Qrz { q: 1, r: 0, z: -1 })),
            }
        });

        app.update();

        // Run another update to process the events
        app.update();

        // Assert: Server should generate Discovery Try events based on new position
        let all_try_events: Vec<_> = {
            let mut try_reader = app.world_mut().resource_mut::<Events<Try>>().get_cursor();
            let try_events = app.world().resource::<Events<Try>>();
            try_reader.read(try_events).cloned().collect()
        };

        let discoveries: Vec<_> = all_try_events.iter()
            .filter_map(|t| {
                if let Try { event: Event::Discover { ent, qrz } } = t {
                    Some((*ent, *qrz))
                } else {
                    None
                }
            })
            .collect();

        // Should have discovered the new tile and its FOV
        assert!(!discoveries.is_empty(), "Server should generate discovery events when authoritative Loc changes");
        assert!(discoveries.iter().any(|(e, q)| *e == player && *q == Qrz { q: 1, r: 0, z: -1 }),
            "Should discover the new tile position");
        // Should also have FOV discoveries (FOV distance 10 generates 120 tiles)
        assert!(discoveries.len() > 100, "Should have FOV discoveries, got {}", discoveries.len());
    }
}
