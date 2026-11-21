use bevy::{ecs::{entity::Entities, query::QueryData}, prelude::*};
use qrz::*;

use common_bevy::{
    components::{ *,
        behaviour::{PlayerControlled, Side},
        entity_type::*,
        equipment::Equipment,
        heading::Heading,
        position::Position,
        resources::*,
    },
    message::{ Component, Event, * },
    systems::*
};
use ::combat::RunTime;

/// What a client is sent of an actor as it meets it: the components
/// that cross the wire, each where the actor has it.
#[derive(QueryData)]
pub struct Synced {
    loc: &'static Loc,
    typ: &'static EntityType,
    attrs: Option<&'static ActorAttributes>,
    player_controlled: Option<&'static PlayerControlled>,
    side: Option<&'static Side>,
    heading: Option<&'static Heading>,
    health: Option<&'static Health>,
    endurance: Option<&'static Endurance>,
    combat_state: Option<&'static CombatState>,
    equipment: Option<&'static Equipment>,
    status: Option<&'static status::Status>,
    activating: Option<&'static activation::Activating>,
}

impl SyncedItem<'_, '_> {
    /// The `Do`s that put `ent` on a client ([`generate_actor_spawn_events`])
    pub fn events(&self, ent: Entity) -> Vec<Do> {
        generate_actor_spawn_events(
            ent,
            *self.typ,
            **self.loc,
            self.attrs.copied(),
            self.player_controlled,
            self.side,
            self.heading,
            self.health,
            self.endurance,
            self.combat_state,
            self.equipment,
            self.status,
            self.activating.and_then(|activating| activating.0),
        )
    }
}

/// The `Do`s that put `ent` on a client: its `Spawn`, then an
/// `Incremental` for each component it has, then the skill it is part way
/// through activating, whole as it was first told. The `Spawn` comes
/// first: a client attaches a component only to an entity it holds.
#[allow(clippy::too_many_arguments)]
pub fn generate_actor_spawn_events(
    ent: Entity,
    typ: EntityType,
    qrz: Qrz,
    attrs: Option<ActorAttributes>,
    player_controlled: Option<&PlayerControlled>,
    side: Option<&Side>,
    heading: Option<&Heading>,
    health: Option<&Health>,
    endurance: Option<&Endurance>,
    combat_state: Option<&CombatState>,
    equipment: Option<&Equipment>,
    status: Option<&status::Status>,
    activation: Option<activation::Activation>,
) -> Vec<Do> {
    let mut events = Vec::new();

    // Spawn event MUST come first to ensure entity exists before components arrive
    events.push(Do { event: Event::Spawn { ent, typ, qrz, attrs }});

    if let Some(pc) = player_controlled {
        events.push(Do { event: Event::Incremental { ent, component: Component::PlayerControlled(*pc) }});
    }

    // Which actors are hostile to which: the client targets by it
    if let Some(s) = side {
        events.push(Do { event: Event::Incremental { ent, component: Component::Side(*s) }});
    }

    if let Some(h) = heading {
        events.push(Do { event: Event::Incremental { ent, component: Component::Heading(*h) }});
    }

    if let Some(h) = health {
        events.push(Do { event: Event::Incremental { ent, component: Component::Health(*h) }});
    }

    if let Some(e) = endurance {
        events.push(Do { event: Event::Incremental { ent, component: Component::Endurance(*e) }});
    }


    if let Some(cs) = combat_state {
        events.push(Do { event: Event::Incremental { ent, component: Component::CombatState(*cs) }});
    }

    if let Some(e) = equipment {
        events.push(Do { event: Event::Incremental { ent, component: Component::Equipment(*e) }});
    }

    // What holds it now, a lurk, a block and a stance among them
    if let Some(s) = status {
        events.push(Do { event: Event::Incremental { ent, component: Component::Status(*s) }});
    }

    if let Some(activation) = activation {
        events.push(Do { event: Event::Activate { ent, activation }});
    }

    events
}

pub fn setup(
    mut runtime: ResMut<RunTime>,
    time: Res<Time>,
) {
    // The calendar's anchor: the wall-clock moment, in the calendar's own
    // zone, the game clock read 0 at, so a date is read from the wall
    // clock alone (`systems::Date`): its midnight is the day's, Monday is
    // Mot, a month's first Monday the year's. The game clock stays small.
    let elapsed = time.elapsed().as_millis();
    runtime.calendar = Calendar::running(wall_now() - elapsed);
}

/// Takes an admin's change to the world's calendar and sends the calendar
/// as it then stands to every client, so every player's date, sky and
/// weather move together.
pub fn try_change_calendar(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut runtime: ResMut<RunTime>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Try { event: Event::ChangeCalendar { change } } = message else { continue };
        let now = runtime.now(&time);
        let wall_at_zero = wall_now() - now.since(common_bevy::Moment::ZERO).as_millis();
        runtime.calendar.change(*change, now, wall_at_zero);
        info!("calendar: {change:?}, now {}", Date::of(runtime.calendar.at(now)));
        writer.write(Do { event: Event::Calendar { calendar: runtime.calendar } });
    }
}

pub fn try_spawn(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    query: Query<Synced, Without<RespawnTimer>>,
) {
    for message in reader.read() {
        let Try { event: Event::Spawn { ent, .. }} = message else { continue };
        let ent = *ent;
        // A dead player is spawned for no one until it respawns
        // (`resources::process_respawn`)
        let Ok(synced) = query.get(ent) else { continue; };
        for event in synced.events(ent) {
            writer.write(event);
        }
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    entities: &Entities,
    existing_actors: Query<(), With<Actor>>,
) {
    for message in reader.read() {
        if let Do { event: Event::Spawn { qrz, typ, ent, .. } } = message {
            let qrz = *qrz;
            let typ = *typ;
            let ent = *ent;
            // A `Spawn` comes again for an actor a client asks after
            // (`try_spawn`), and for one gone since: an actor standing
            // already keeps what it has
            if let EntityType::Actor(_) = typ {
                if entities.contains(ent) && existing_actors.get(ent).is_err() {
                    commands.entity(ent).insert((
                        Actor,
                        AirTime { state: Some(movement::JUMP_DURATION_MS), step: None },
                        Heading::default(),
                        Turn::default(),
                        Position::at_tile(qrz),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_meeting_an_actor_late_learns_what_it_is_activating() {
        let ent = Entity::from_raw_u32(7).unwrap();
        let activation = activation::Activation {
            ability: AbilityType::Overpower,
            target: None,
            began: common_bevy::Moment::from_millis(500),
            length: std::time::Duration::from_secs(1),
        };
        let met = |activation| generate_actor_spawn_events(
            ent, EntityType::default(), Qrz::default(), None, None, None, None, None, None, None, None,
            Some(&status::Status::default()), activation,
        );

        let events = met(Some(activation));
        assert!(matches!(events.first(), Some(Do { event: Event::Spawn { .. } })), "the actor comes before anything of it");
        assert!(
            matches!(events.last(), Some(Do { event: Event::Activate { ent: told, activation: whole } }) if *told == ent && *whole == activation),
            "its skill is told as it began, so the client counts from the same moment",
        );
        assert!(!met(None).iter().any(|told| matches!(told.event, Event::Activate { .. })), "one activating nothing tells of none");
    }
}
