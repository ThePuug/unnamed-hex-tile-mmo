use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::*, Actor, ActorAttributes, AttackRange, Loc},
    components::recovery::GlobalRecovery,
    message::{AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::{combos, queue as queue_utils},
    tuning::Tuning,
};

/// How long a gone threat is kept for the notes it ends
const GONE_KEPT: f32 = 1.0;

/// How each threat lately gone from a queue here went: landed, or
/// answered. A note whose threat is gone looks it up to pulse or shatter.
/// The server says which for every queue (`ClearType`); the local
/// player's own landings and answers are told here first, on its clock.
#[derive(Resource, Default)]
pub struct Gone(Vec<Went>);

struct Went {
    ent: Entity,
    source: Entity,
    inserted_at: std::time::Duration,
    landed: bool,
    at: f32,
}

impl Gone {
    /// Whether the threat `source` queued at `inserted_at` on `ent` landed,
    /// or None while nothing is known of it going
    pub fn landed(&self, ent: Entity, source: Entity, inserted_at: std::time::Duration) -> Option<bool> {
        self.0.iter()
            .find(|went| (went.ent, went.source, went.inserted_at) == (ent, source, inserted_at))
            .map(|went| went.landed)
    }

    fn record(&mut self, ent: Entity, threats: &[QueuedThreat], landed: bool, at: f32) {
        self.0.retain(|went| at - went.at < GONE_KEPT);
        self.0.extend(threats.iter().map(|threat| Went { ent, source: threat.source, inserted_at: threat.inserted_at, landed, at }));
    }
}

/// Client system to handle InsertThreat events
/// Inserts threats into the visual reaction queue for display
/// No deduplication needed - we don't predict threat insertions
pub fn handle_insert_threat(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
) {
    for event in reader.read() {
        if let GameEvent::InsertThreat { ent, threat } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                // Insert always succeeds (unbounded queue)
                queue_utils::insert_threat(&mut queue, threat);
            }
        }
    }
}

/// Takes from a queue what the server cleared from it: a threat that
/// landed, or what a reaction answered. The local player's own are mostly
/// gone already, told on its clock ([`land_own`], [`answer_own`]).
pub fn handle_clear_queue(
    mut reader: MessageReader<Do>,
    mut query: Query<&mut ReactionQueue>,
    mut gone: ResMut<Gone>,
    time: Res<Time>,
) {
    for event in reader.read() {
        if let GameEvent::ClearQueue { ent, clear_type } = event.event {
            if let Ok(mut queue) = query.get_mut(ent) {
                let cleared = queue_utils::clear_threats(&mut queue, clear_type);
                gone.record(ent, &cleared, matches!(clear_type, ClearType::Threat { .. }), time.elapsed_secs());
            }
        }
    }
}

/// Lands each threat on the local player as its time runs on the client's
/// clock, as the server will: its note leaves the line on time. Its damage
/// comes with the server's word, a trip later.
pub fn land_own(
    mut query: Query<(Entity, &mut ReactionQueue), With<Actor>>,
    mut gone: ResMut<Gone>,
    time: Res<Time>,
    server: Res<crate::resources::Server>,
) {
    let Ok((ent, mut queue)) = query.single_mut() else { return };
    let now = server.now(time.elapsed().as_millis());
    let landed = queue_utils::check_expired_threats(&queue, now);
    for threat in &landed {
        queue_utils::clear_threats(&mut queue, ClearType::Threat { source: threat.source, inserted_at: threat.inserted_at });
    }
    gone.record(ent, &landed, true, time.elapsed_secs());
}

/// Answers the local player's reaction as it is pressed, as the server
/// will judge it at the moment it carries: what lands in its band goes at
/// once. A press the server refuses or hears of late is told by the damage
/// that follows.
pub fn answer_own(
    mut reader: MessageReader<Try>,
    mut query: Query<(Entity, &mut ReactionQueue, &ActorAttributes, &Loc, Option<&AttackRange>, Option<&GlobalRecovery>), With<Actor>>,
    locs: Query<&Loc>,
    mut gone: ResMut<Gone>,
    tuning: Res<Tuning>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target, at, .. } } = message else { continue };
        let Ok((own, mut queue, attrs, loc, reach, recovery)) = query.single_mut() else { return };
        if *ent != own {
            continue;
        }
        let reach = reach.copied().unwrap_or_default().0;
        let in_reach = *ability == AbilityType::Leap
            && target.and_then(|target| locs.get(target).ok()).is_some_and(|target| loc.distance(target) <= reach);
        let reacting = ability.reacts(in_reach);
        if !reacting || !combos::may_use(*ability, reacting, recovery, attrs) {
            continue;
        }
        let span = attrs.span(&tuning);
        let start = queue.band(*at, attrs.awareness_snap(&tuning));
        let answered = queue_utils::clear_threats(&mut queue, ClearType::Span { at: start, span });
        gone.record(own, &answered, false, time.elapsed_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{components::ActorAttributes, message::{AbilityType, ClearType}, tuning::Tuning};
    use std::time::Duration;

    #[test]
    fn a_threat_clears_after_its_source_is_gone_here() {
        let tuning = Tuning::DEFAULT;
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.insert_resource(crate::resources::Server::default());
        app.init_resource::<crate::resources::EntityMap>();
        app.init_resource::<Gone>();
        app.add_systems(Update, (handle_insert_threat, handle_clear_queue).chain());

        let player = app.world_mut().spawn((ReactionQueue::default(), ActorAttributes::default())).id();
        let attacker = app.world_mut().spawn(ActorAttributes::default()).id();
        let on_server = Entity::from_raw_u32(9_000).unwrap();
        app.world_mut().resource_mut::<crate::resources::EntityMap>().insert(attacker, on_server);

        let threat = queue_utils::create_threat(
            &tuning,
            on_server, &ActorAttributes::default(), &ActorAttributes::default(),
            50.0, Some(AbilityType::Frenzy), Duration::from_secs(10), 0.0, 0.0,
        );
        app.world_mut().write_message(Do { event: GameEvent::InsertThreat { ent: player, threat } });
        app.update();
        assert_eq!(app.world().get::<ReactionQueue>(player).unwrap().threats.len(), 1);

        // The attacker dies and is despawned here before its threat lands
        app.world_mut().resource_mut::<crate::resources::EntityMap>().remove_by_left(&attacker);
        app.world_mut().despawn(attacker);
        let clear_type = ClearType::Threat { source: on_server, inserted_at: threat.inserted_at };
        app.world_mut().write_message(Do { event: GameEvent::ClearQueue { ent: player, clear_type } });
        app.update();
        assert!(app.world().get::<ReactionQueue>(player).unwrap().threats.is_empty(), "the landing clears it all the same");
        assert_eq!(app.world().resource::<Gone>().landed(player, on_server, threat.inserted_at), Some(true), "and is told as a landing");
    }

    fn own_player(app: &mut App) -> Entity {
        app.add_message::<Do>();
        app.add_message::<Try>();
        app.init_resource::<Time>();
        app.init_resource::<Tuning>();
        app.init_resource::<Gone>();
        app.insert_resource(crate::resources::Server::default());
        app.add_systems(Update, (land_own, answer_own));
        app.world_mut().spawn((Actor, ReactionQueue::default(), ActorAttributes::default(), Loc::default())).id()
    }

    fn queued(app: &mut App, ent: Entity, source: Entity, lands_in: Duration) -> QueuedThreat {
        let tuning = Tuning::DEFAULT;
        let server = app.world().resource::<crate::resources::Server>();
        let now = Duration::from_millis(server.current_time(0) as u64);
        let mut threat = queue_utils::create_threat(&tuning, source, &ActorAttributes::default(), &ActorAttributes::default(), 10.0, Some(AbilityType::Frenzy), now, 0.0, 0.0);
        threat.timer_duration = lands_in;
        queue_utils::insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(ent).unwrap(), threat);
        threat
    }

    #[test]
    fn the_local_player_lands_its_own_threats_on_its_own_clock() {
        let mut app = App::new();
        let player = own_player(&mut app);
        let [one, other] = [9_000, 9_001].map(|id| Entity::from_raw_u32(id).unwrap());
        let due = queued(&mut app, player, one, Duration::ZERO);
        let later = queued(&mut app, player, other, Duration::from_secs(3));
        app.update();
        let gone = app.world().resource::<Gone>();
        assert_eq!(gone.landed(player, one, due.inserted_at), Some(true), "its time run, it lands here");
        assert_eq!(gone.landed(player, other, later.inserted_at), None, "and one to come stands");
        assert_eq!(app.world().get::<ReactionQueue>(player).unwrap().threats.len(), 1);
    }

    #[test]
    fn the_local_players_reaction_answers_its_band_as_pressed() {
        let tuning = Tuning::DEFAULT;
        let mut app = App::new();
        let player = own_player(&mut app);
        let [one, other, third] = [9_000, 9_001, 9_002].map(|id| Entity::from_raw_u32(id).unwrap());
        let near = queued(&mut app, player, one, Duration::from_millis(500));
        let far = queued(&mut app, player, other, Duration::from_secs(2));
        let span = ActorAttributes::default().span(&tuning);
        let at = near.lands_at() - span / 2;
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent: player, ability: AbilityType::Parry, target: None, at, arrived: Duration::ZERO } });
        app.update();
        let gone = app.world().resource::<Gone>();
        assert_eq!(gone.landed(player, one, near.inserted_at), Some(false), "what lands in its band is answered");
        assert_eq!(gone.landed(player, other, far.inserted_at), None, "and the rest stands");

        // A skill that is no reaction answers nothing
        let struck = queued(&mut app, player, third, Duration::from_millis(700));
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent: player, ability: AbilityType::Frenzy, target: None, at: struck.lands_at(), arrived: Duration::ZERO } });
        app.update();
        assert_eq!(app.world().resource::<Gone>().landed(player, third, struck.inserted_at), None);
    }
}
