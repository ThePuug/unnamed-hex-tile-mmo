use bevy::{ecs::{entity::Entities, query::QueryData}, prelude::*};
use chrono::{
    offset::Local, Datelike, Timelike
};
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
    mana: Option<&'static Mana>,
    combat_state: Option<&'static CombatState>,
    equipment: Option<&'static Equipment>,
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
            self.mana,
            self.combat_state,
            self.equipment,
        )
    }
}

/// The `Do`s that put `ent` on a client: its `Spawn`, then an
/// `Incremental` for each component it has. The `Spawn` comes first: a
/// client attaches a component only to an entity it holds.
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
    mana: Option<&Mana>,
    combat_state: Option<&CombatState>,
    equipment: Option<&Equipment>,
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

    if let Some(m) = mana {
        events.push(Do { event: Event::Incremental { ent, component: Component::Mana(*m) }});
    }

    if let Some(cs) = combat_state {
        events.push(Do { event: Event::Incremental { ent, component: Component::CombatState(*cs) }});
    }

    if let Some(e) = equipment {
        events.push(Do { event: Event::Incremental { ent, component: Component::Equipment(*e) }});
    }

    events
}

pub fn setup(
    mut runtime: ResMut<RunTime>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed().as_millis();
    let secs_since_midnight = Local::now().time().num_seconds_from_midnight();
    let days_since_monday = Local::now().weekday().number_from_monday() - 1;
    let weeks_since_year = Local::now().iso_week().week();
    runtime.elapsed_offset = weeks_since_year as u128 * SEASON_MS
        + days_since_monday as u128 * WEEK_MS
        + secs_since_midnight as u128 * 1000
        - elapsed;
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
