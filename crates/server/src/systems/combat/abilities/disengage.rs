use bevy::prelude::*;
use common_bevy::{
    components::{reaction_queue::ReactionQueue, resources::*, Loc, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    systems::combat::queue::clear_threats,
    resources::map::Map,
};

pub const DISENGAGE_STAMINA_COST: f32 = 20.0;

/// Damage a Disengage adds to its caster's next auto-attack, spent by that
/// blow, which comes behind a feint: a damage-free threat queued ahead of
/// it, so a reaction that takes the front of the queue takes the feint. A
/// second Disengage before the blow replaces the first's.
#[derive(Clone, Component, Copy, Debug)]
pub struct Poised(pub f32);

/// Handle Disengage, the Skirmisher's signature: a reaction to the blow at
/// the front of its queue, whose source is the event's target. The caster
/// leaps `ArchetypeTuning::disengage_leap` tiles, each the neighbour furthest
/// from that source, and the blow misses: the front threat is cleared. Its
/// next auto-attack strikes harder, by `disengage_technique` of its
/// Technique, behind a feint (`Poised`).
pub fn handle_disengage(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    mut queue_query: Query<&mut ReactionQueue>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    map: Res<Map>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Disengage, target } } = event else {
            continue;
        };
        if respawn_query.get(*ent).is_ok() {
            continue;
        }
        if recovery_query.get(*ent).is_ok_and(|recovery| recovery.is_active()) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }
        let (Ok(caster_loc), Some(Ok(target_loc))) = (loc_query.get(*ent), target.map(|t| loc_query.get(t))) else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        };
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < DISENGAGE_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }

        // Walk the ground away from the target, one neighbour at a time
        let Some((mut ground, _)) = map.get_by_qr(caster_loc.q, caster_loc.r) else {
            continue;
        };
        for _ in 0..tuning.disengage_leap {
            let Some((next, _)) = map.neighbors(ground).into_iter()
                .max_by_key(|(neighbor, _)| neighbor.flat_distance(target_loc))
                .filter(|(neighbor, _)| neighbor.flat_distance(target_loc) > ground.flat_distance(target_loc))
            else {
                break;
            };
            ground = next;
        }
        let landing = ground + qrz::Qrz::Z;
        if landing == **caster_loc {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }

        stamina.state -= DISENGAGE_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });
        writer.write(Do { event: GameEvent::Displace { ent: *ent, destination: landing + qrz::Qrz::Z, duration_ms: 250 } });
        commands.entity(*ent).insert((Loc::new(landing), common_bevy::components::position::Position::at_tile(landing)));
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Loc(Loc::new(landing)) },
        });

        if let Ok(mut queue) = queue_query.get_mut(*ent) {
            if !clear_threats(&mut queue, ClearType::First(1)).is_empty() {
                writer.write(Do { event: GameEvent::ClearQueue { ent: *ent, clear_type: ClearType::First(1) } });
            }
        }
        if let Ok(attrs) = attrs_query.get(*ent) {
            commands.entity(*ent).insert(Poised(attrs.technique() * tuning.disengage_technique));
        }

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Disengage, target: *target } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Disengage), AbilityType::Disengage));
    }
}
