use crate::components::reaction_queue::{QueuedThreat, ReactionQueue};
use crate::components::ActorAttributes;
use crate::message::ClearType;
use bevy::prelude::*;
use std::time::Duration;
use crate::tuning::Tuning;

/// Reaction window base from level gap.

/// Pattern 2 (Baseline+Bonus): 3.0s × gap × (1.0 + 0.5 × contest_factor)
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

/// Create a threat with proper timer calculation (INVARIANT: INV-003)

/// **CRITICAL INVARIANT (INV-003):** A threat's timer is set by who struck whom and
/// how fatigued the target is, never by which ability created it.
/// This ensures consistent reaction windows and prevents ability-specific timing quirks.

/// The timer is [`threat_window`].

/// # Arguments
/// * `source` - Attacker entity (source of threat)
/// * `target_attrs` - Defender's attributes (receives threat)
/// * `source_attrs` - Attacker's attributes (creates threat)
/// * `damage` - Final damage amount
/// * `ability` - Which ability created this threat
/// * `now` - Current game time
/// * `dot` - Damage each DoT tick deals: a wound's, zero for a blow
/// * `fatigue` - The defender's fatigue, 0 to 1

/// # Returns
/// Fully-formed QueuedThreat with correct timer duration
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
            bind: 0.0,
    }
}

/// Insert a threat into the queue (unbounded, no overflow eviction), ahead
/// of every threat landing later. Server and client both insert through
/// here, so their queues hold the same order.
pub fn insert_threat(
    queue: &mut ReactionQueue,
    threat: crate::components::reaction_queue::QueuedThreat,
    _now: Duration,
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
    fn test_insert_threat_unbounded() {
        let mut queue = ReactionQueue::default();
        let entity = Entity::from_raw_u32(0).unwrap();

        let make_threat = |damage: f32, secs: u64| QueuedThreat {
            source: entity,
            damage,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            bind: 0.0,

        };

        // Insert always succeeds, no overflow
        insert_threat(&mut queue, make_threat(10.0, 0), Duration::from_secs(0));
        assert_eq!(queue.threats.len(), 1);

        insert_threat(&mut queue, make_threat(15.0, 1), Duration::from_secs(1));
        assert_eq!(queue.threats.len(), 2);

        insert_threat(&mut queue, make_threat(20.0, 2), Duration::from_secs(2));
        assert_eq!(queue.threats.len(), 3);

        // Insert more — all succeed
        insert_threat(&mut queue, make_threat(25.0, 3), Duration::from_secs(3));
        insert_threat(&mut queue, make_threat(30.0, 4), Duration::from_secs(4));
        assert_eq!(queue.threats.len(), 5);
    }

    #[test]
    fn every_threat_stands_by_when_it_lands_whatever_its_kind() {
        use crate::message::AbilityType::{AutoAttack, Frenzy};
        let mut queue = ReactionQueue::default();
        let make = |ability, dot: f32, secs: u64| QueuedThreat {
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 10.0,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(1),
            ability: Some(ability),
            dot,
            ticked: 0,
            bind: 0.0,
        };
        for (ability, dot, secs) in [(Frenzy, 0.0, 2), (AutoAttack, 0.0, 0), (Frenzy, 5.0, 1), (AutoAttack, 0.0, 3)] {
            insert_threat(&mut queue, make(ability, dot, secs), Duration::ZERO);
        }
        let order: Vec<_> = queue.threats.iter().map(|t| t.inserted_at.as_secs()).collect();
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_check_expired_threats_none_expired() {
        let mut queue = ReactionQueue::default();
        let entity = Entity::from_raw_u32(0).unwrap();

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            bind: 0.0,

        };

        queue.threats.push_back(threat);

        // Check at 0.5s - threat expires at 1.0s, so not expired yet
        let expired = check_expired_threats(&queue, Duration::from_millis(500));
        assert_eq!(expired.len(), 0);
        assert_eq!(queue.threats.len(), 1); // Threat still in queue
    }

    #[test]
    fn test_check_expired_threats_one_expired() {
        let mut queue = ReactionQueue::default();
        let entity = Entity::from_raw_u32(0).unwrap();

        let threat = QueuedThreat {
            source: entity,
            damage: 10.0,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            bind: 0.0,

        };

        queue.threats.push_back(threat.clone());

        assert!(check_expired_threats(&queue, Duration::from_millis(999)).is_empty(), "its time still runs");
        let expired = check_expired_threats(&queue, Duration::from_secs(1));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].damage, 10.0);
        assert_eq!(queue.threats.len(), 1); // check_expired_threats doesn't remove
    }

    #[test]
    fn test_check_expired_threats_multiple() {
        let mut queue = ReactionQueue::default();
        let entity = Entity::from_raw_u32(0).unwrap();

        // Threat 1: inserted at 0s, expires at 1s
        let threat1 = QueuedThreat {
            source: entity,
            damage: 10.0,
            inserted_at: Duration::from_secs(0),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            bind: 0.0,

        };

        // Threat 2: inserted at 0.5s, expires at 1.5s
        let threat2 = QueuedThreat {
            source: entity,
            damage: 15.0,
            inserted_at: Duration::from_millis(500),
            timer_duration: Duration::from_secs(1),
            ability: None,
            dot: 0.0,
            ticked: 0,
            bind: 0.0,

        };

        queue.threats.push_back(threat1);
        queue.threats.push_back(threat2);

        // Only threat1 has landed
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
            queue.threats.push_back(QueuedThreat {
                source,
                damage: 10.0,
                inserted_at: Duration::from_secs(secs),
                timer_duration: Duration::from_secs(1),
                ability: None,
                dot: 0.0,
                ticked: 0,
            bind: 0.0,
            });
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
            source: Entity::from_raw_u32(0).unwrap(),
            damage: secs as f32,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(window),
            ability: Some(crate::message::AbilityType::Frenzy),
            dot: 0.0,
            ticked: 0,
            bind: 0.0,
        };
        for threat in [make(0, 3), make(1, 3), make(4, 3)] {
            insert_threat(&mut queue, threat, Duration::ZERO);
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
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 10.0,
            inserted_at: Duration::from_secs(secs),
            timer_duration: Duration::from_secs(window),
            ability: Some(crate::message::AbilityType::Frenzy),
            dot: 0.0,
            ticked: 0,
            bind: 0.0,
        };
        insert_threat(&mut queue, make(0, 5), Duration::ZERO);
        insert_threat(&mut queue, make(1, 3), Duration::ZERO);
        let order: Vec<_> = queue.threats.iter().map(|t| t.inserted_at.as_secs()).collect();
        assert_eq!(order, vec![1, 0], "the later blow lands first, so it stands first");
    }

    #[test]
    fn a_wound_ticks_before_it_lands_and_lands_for_what_it_has_not_dealt() {
        let wound = QueuedThreat {
            source: Entity::from_raw_u32(0).unwrap(),
            damage: 0.0,
            inserted_at: Duration::from_secs(10),
            timer_duration: Duration::from_secs(3),
            ability: None,
            dot: 5.0,
            ticked: 0,
            bind: 0.0,
        };
        assert_eq!(wound.tick_count(), 2, "ticks fall short of the landing");
        assert_eq!(wound.ticks_due(Duration::from_millis(10_999)), 0);
        assert_eq!(wound.ticks_due(Duration::from_secs(11)), 1);
        assert_eq!(wound.ticks_due(Duration::from_secs(20)), 2);
        assert_eq!(wound.dot_left(), 10.0, "taken at once, it deals every tick");
        assert_eq!(QueuedThreat { ticked: 2, ..wound }.dot_left(), 0.0, "ticked out, it lands for nothing");
        assert_eq!(QueuedThreat { dot: 0.0, ..wound }.tick_count(), 0, "a blow never ticks");
    }
}
