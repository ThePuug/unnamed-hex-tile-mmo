use crate::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::components::ActorAttributes;
use crate::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;
use crate::tuning::Tuning;

/// How long a threat from `source_attrs` waits in the queue of
/// `target_attrs` before it lands: the same whatever made it (INV-003).
///
/// `Tuning::reaction_window`, the same for every threat between any two
/// actors, extended by the reaction contest: the defender's Reflex against
/// the attacker's Tempo, with the level gap's edge on the defender's side.
/// The defender's `fatigue`, 0 to 1 (`Endurance::fatigue`), shortens it by
/// `Tuning::fatigue_window` of it.
pub fn threat_window(tuning: &Tuning, target_attrs: &ActorAttributes, source_attrs: &ActorAttributes, fatigue: f32) -> Duration {
    use crate::systems::combat::damage::{level_edge, reaction_contest_factor};

    let edge = level_edge(tuning, target_attrs.total_level(), source_attrs.total_level());
    let multiplier = reaction_contest_factor(tuning, target_attrs.reflex(), source_attrs.tempo(), edge);
    Duration::from_secs_f32(tuning.reaction_window * multiplier * (1.0 - tuning.fatigue_window * fatigue))
}

/// A threat from `source`, an actor of `source_attrs`, on one of
/// `target_attrs` for `damage`, made by `ability` at `now`, dealing `dot`
/// each tick while it stands (a wound's; zero for a blow), against a
/// defender of `fatigue` 0 to 1. Its timer is [`threat_window`]: by who
/// struck whom and the target's fatigue, never by the ability (INV-003),
/// so every threat is made here and none sets its own.
#[allow(clippy::too_many_arguments)]
pub fn create_threat(
    tuning: &Tuning,
    source: bevy::prelude::Entity,
    target_attrs: &ActorAttributes,
    source_attrs: &ActorAttributes,
    damage: f32,
    ability: Option<crate::message::AbilityType>,
    now: Duration,
    dot: f32,
    fatigue: f32,
) -> crate::components::reaction_queue::QueuedThreat {
    crate::components::reaction_queue::QueuedThreat {
        source,
        damage,
        inserted_at: now,
        timer_duration: threat_window(tuning, target_attrs, source_attrs, fatigue),
        ability,
        dot,
        ticked: 0,
        stride: 0.0,
    }
}

/// Insert a threat into the queue (unbounded, no overflow eviction), ahead
/// of every threat landing later. Server and client both insert through
/// here, so their queues hold the same order.
pub fn insert_threat(
    queue: &mut ReactionQueue,
    threat: crate::components::reaction_queue::QueuedThreat,
) {
    let at = queue.threats.partition_point(|t| t.lands_at() <= threat.lands_at());
    queue.threats.insert(at, threat);
}

/// The threats in the queue whose time has run by `now`. Does not remove
/// them; the caller does.
pub fn check_expired_threats(queue: &ReactionQueue, now: Duration) -> Vec<QueuedThreat> {
    queue
        .threats
        .iter()
        .filter(|threat| now >= threat.lands_at())
        .cloned()
        .collect()
}

/// Takes the threats `clear_type` names out of the queue and returns them.
pub fn clear_threats(queue: &mut ReactionQueue, clear_type: ClearType) -> Vec<QueuedThreat> {
    match clear_type {
        ClearType::Span { at, span } => {
            let (taken, kept): (Vec<_>, Vec<_>) = queue.threats.iter().copied().partition(|threat| threat.in_band(at, span));
            queue.threats = kept.into();
            taken
        }
        ClearType::Threat { source, inserted_at } => queue
            .threats
            .iter()
            .position(|t| t.source == source && t.inserted_at == inserted_at)
            .and_then(|pos| queue.threats.remove(pos))
            .into_iter()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blow from `source` for `damage`, queued `secs` in and landing
    /// `window` seconds on
    fn blow(source: Entity, damage: f32, secs: u64, window: u64) -> QueuedThreat {
        QueuedThreat {
            source,
            damage,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(window),
            ability: None,
            dot: 0.0,
            ticked: 0,
            stride: 0.0,
        }
    }

    fn someone() -> Entity {
        Entity::from_raw_u32(0).unwrap()
    }

    #[test]
    fn a_fatigued_target_has_less_time_to_answer() {
        let tuning = Tuning::DEFAULT;
        let plain = ActorAttributes::default();
        let fresh = threat_window(&tuning, &plain, &plain, 0.0);
        assert!(threat_window(&tuning, &plain, &plain, 0.5) < fresh);
        assert!(threat_window(&tuning, &plain, &plain, 1.0) < threat_window(&tuning, &plain, &plain, 0.5));
        assert!(threat_window(&tuning, &plain, &plain, 1.0) > Duration::ZERO, "spent, it still has a window");
    }

    #[test]
    fn every_threat_stands_by_when_it_lands_whatever_its_kind() {
        use crate::message::AbilityType::{AutoAttack, Frenzy};
        let mut queue = ReactionQueue::default();
        for (ability, dot, secs) in [(Frenzy, 0.0, 2), (AutoAttack, 0.0, 0), (Frenzy, 5.0, 1), (AutoAttack, 0.0, 3)] {
            insert_threat(&mut queue, QueuedThreat { ability: Some(ability), dot, ..blow(someone(), 10.0, secs, 1) });
        }
        let order: Vec<_> = queue.threats.iter().map(|t| t.inserted_at.as_secs()).collect();
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_check_expired_threats_one_expired() {
        let mut queue = ReactionQueue::default();
        queue.threats.push_back(blow(someone(), 10.0, 0, 1));

        assert!(check_expired_threats(&queue, Duration::from_millis(999)).is_empty(), "its time still runs");
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);
        assert_eq!(queue.threats.len(), 1); // check_expired_threats doesn't remove
    }

    #[test]
    fn test_check_expired_threats_multiple() {
        let mut queue = ReactionQueue::default();
        // Landing at 1s and at 1.5s
        queue.threats.push_back(blow(someone(), 10.0, 0, 1));
        queue.threats.push_back(QueuedThreat { inserted_at: Duration::from_millis(500), ..blow(someone(), 15.0, 0, 1) });

        // Only the first has landed
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);

        // Both have
        let expired = check_expired_threats(&queue, Duration::from_millis(1500));
        assert_eq!(expired.len(), 2);
    }

    #[test]
    fn test_clear_threats_names_one_threat_wherever_it_stands() {
        let mut queue = ReactionQueue::default();
        let a = Entity::from_raw_u32(0).unwrap();
        let b = Entity::from_raw_u32(1).unwrap();
        for (source, secs) in [(a, 0), (b, 0), (a, 1)] {
            queue.threats.push_back(blow(source, 10.0, secs, 1));
        }

        let cleared = clear_threats(&mut queue, ClearType::Threat { source: b, inserted_at: Duration::from_secs(0) });
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].source, b);
        assert_eq!(queue.threats.iter().map(|t| (t.source, t.inserted_at.as_secs())).collect::<Vec<_>>(), vec![(a, 0), (a, 1)]);

        let missing = clear_threats(&mut queue, ClearType::Threat { source: b, inserted_at: Duration::from_secs(0) });
        assert!(missing.is_empty());
        assert_eq!(queue.threats.len(), 2);
    }

    #[test]
    fn clearing_a_band_takes_what_lands_in_it_and_leaves_the_rest_in_order() {
        let mut queue = ReactionQueue::default();
        let make = |secs: u64, window: u64| QueuedThreat {
            ability: Some(crate::message::AbilityType::Frenzy),
            ..blow(someone(), secs as f32, secs, window)
        };
        for threat in [make(0, 3), make(1, 3), make(4, 3)] {
            insert_threat(&mut queue, threat);
        }
        let band = ClearType::Span { at: Duration::from_millis(3500), span: Duration::from_secs(1) };
        let cleared = clear_threats(&mut queue, band);
        assert_eq!(cleared.iter().map(|t| t.damage).collect::<Vec<_>>(), vec![1.0], "the one landing in the band");
        assert_eq!(queue.threats.iter().map(|t| t.damage).collect::<Vec<_>>(), vec![0.0, 4.0]);
        assert!(clear_threats(&mut ReactionQueue::default(), band).is_empty());
    }

    #[test]
    fn what_lands_soonest_stands_first() {
        let mut queue = ReactionQueue::default();
        let make = |secs, window| QueuedThreat {
            ability: Some(crate::message::AbilityType::Frenzy),
            ..blow(someone(), 10.0, secs, window)
        };
        insert_threat(&mut queue, make(0, 5));
        insert_threat(&mut queue, make(1, 3));
        let order: Vec<_> = queue.threats.iter().map(|t| t.inserted_at.as_secs()).collect();
        assert_eq!(order, vec![1, 0], "the later blow lands first, so it stands first");
    }

    #[test]
    fn a_wound_ticks_before_it_lands_and_lands_for_what_it_has_not_dealt() {
        let wound = QueuedThreat { dot: 5.0, ..blow(someone(), 0.0, 10, 3) };
        assert_eq!(wound.tick_count(), 2, "ticks fall short of the landing");
        assert_eq!(wound.ticks_due(Duration::from_millis(10_999)), 0);
        assert_eq!(wound.ticks_due(Duration::from_secs(11)), 1);
        assert_eq!(wound.ticks_due(Duration::from_secs(20)), 2);
        assert_eq!(wound.dot_left(), 10.0, "taken at once, it deals every tick");
        assert_eq!(QueuedThreat { ticked: 2, ..wound }.dot_left(), 0.0, "ticked out, it lands for nothing");
        assert_eq!(QueuedThreat { dot: 0.0, ..wound }.tick_count(), 0, "a blow never ticks");
    }
}
