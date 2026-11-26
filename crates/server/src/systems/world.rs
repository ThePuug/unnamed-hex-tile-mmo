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

/// The calendar's anchor: the wall-clock moment, in the calendar's own
/// zone, the game clock read 0 at, so a date is read from the wall clock
/// alone (`systems::Date`): its midnight is the day's, Monday is Mot, a
/// month's first Monday the year's. Where the calendar's zero stands with
/// no admin's hand on it, and what the store measures an admin's shift
/// from. [`follow_wall`] alone moves it, the calendar with it.
#[derive(Resource, Default)]
pub struct Anchor(pub u128);

/// The wall-clock moment the game clock read 0 at, as the two read now.
fn wall_at_zero(runtime: &RunTime, time: &Time) -> u128 {
    wall_now().saturating_sub(runtime.now(time).since(common_bevy::Moment::ZERO).as_millis())
}

pub fn setup(
    mut runtime: ResMut<RunTime>,
    mut anchor: ResMut<Anchor>,
    time: Res<Time>,
) {
    anchor.0 = wall_at_zero(&runtime, &time);
    runtime.calendar = Calendar::running(anchor.0);
}

/// Keeps the calendar at the wall clock. The game clock gives up whatever
/// a frame runs past its longest step, so the wall clock stood later at
/// its zero than the anchor says: the anchor and the calendar move on by
/// what was given up, an admin's shift between them unchanged, and every
/// client is sent the calendar. Never back: the two clocks' readings
/// differ by a millisecond from frame to frame.
pub fn follow_wall(
    mut writer: MessageWriter<Do>,
    mut runtime: ResMut<RunTime>,
    mut anchor: ResMut<Anchor>,
    time: Res<Time>,
) {
    let lost = wall_at_zero(&runtime, &time).saturating_sub(anchor.0);
    if lost == 0 {
        return;
    }
    anchor.0 += lost;
    runtime.calendar.wall_at_zero += lost;
    writer.write(Do { event: Event::Calendar { calendar: runtime.calendar } });
}

/// Takes an admin's change to the world's calendar and sends the calendar
/// as it then stands to every client, so every player's date, sky and
/// weather move together. A clock scrubbed is a change a frame: it is
/// said once a second while it moves, and any other change at once.
pub fn try_change_calendar(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut runtime: ResMut<RunTime>,
    anchor: Res<Anchor>,
    time: Res<Time>,
    mut said: Local<Option<common_bevy::Moment>>,
) {
    for message in reader.read() {
        let Try { event: Event::ChangeCalendar { change } } = message else { continue };
        let now = runtime.now(&time);
        runtime.calendar.change(*change, now, anchor.0);
        let scrubbed = matches!(change, common_bevy::systems::CalendarChange::Shift(_));
        if !scrubbed || said.is_none_or(|said| now.since(said) >= std::time::Duration::from_secs(1)) {
            info!("calendar: {change:?}, now {}", Date::of(runtime.calendar.at(now)));
            *said = Some(now);
        }
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

    #[test]
    fn a_calendar_behind_the_wall_clock_is_moved_up_to_it_and_a_reset_leaves_no_shift() {
        use bevy::ecs::system::RunSystemOnce;
        use common_bevy::systems::CalendarChange;

        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<RunTime>();
        world.init_resource::<Anchor>();
        world.init_resource::<Messages<Do>>();
        world.init_resource::<Messages<Try>>();
        world.run_system_once(setup).unwrap();

        // The game clock a minute short of the wall clock, under an admin's hour
        const SHIFT: u128 = 3_600_000;
        world.resource_mut::<Anchor>().0 -= 60_000;
        let stale = world.resource::<Anchor>().0;
        world.resource_mut::<RunTime>().calendar = Calendar::running(stale + SHIFT);
        world.run_system_once(follow_wall).unwrap();
        let anchor = world.resource::<Anchor>().0;
        assert!(anchor >= stale + 60_000, "the anchor is the wall clock's again");
        assert_eq!(world.resource::<RunTime>().calendar.wall_at_zero, anchor + SHIFT, "the shift is the admin's still");
        assert!(world.resource::<Messages<Do>>().iter_current_update_messages().any(|told| matches!(told.event, Event::Calendar { .. })), "every client is told");

        world.write_message(Try { event: Event::ChangeCalendar { change: CalendarChange::Reset } });
        world.run_system_once(try_change_calendar).unwrap();
        assert_eq!(world.resource::<RunTime>().calendar, Calendar::running(world.resource::<Anchor>().0), "a reset stands on the anchor");
    }
}
