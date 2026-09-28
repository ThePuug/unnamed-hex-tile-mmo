use bevy::prelude::*;
use common_bevy::systems::targeting::faces;
use common_bevy::{
    components::{resources::*, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

pub const VOLLEY_STAMINA_COST: f32 = 20.0;

/// Handle Volley, the Kiter's signature: a burst of shots at a target within
/// `KITER_REACH`, one shot per threat the Kiter can see — its Concentration
/// window, so Focus decides how many land — each striking for
/// `ArchetypeTuning::volley_force` of Force. Every shot is its own threat.
/// The burst slows its target by `volley_slow` for `volley_slow_secs`, and
/// sets the Kiter running at `volley_run` of its pace for `volley_run_secs`,
/// in which it backs away to the far edge of its band, facing and shooting:
/// at a run of 2 its back-up is as fast as a walk.
pub fn handle_volley(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    heading_query: Query<&common_bevy::components::heading::Heading>,
    status_query: Query<&common_bevy::components::status::Status>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Volley, target } } = event else {
            continue;
        };
        if respawn_query.get(*ent).is_ok() {
            continue;
        }
        if recovery_query.get(*ent).is_ok_and(|recovery| recovery.is_active()) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }
        let Some(target_ent) = *target else {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        };
        if respawn_query.get(target_ent).is_ok() {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NoTargets } });
            continue;
        }
        let (Ok(caster_loc), Ok(target_loc)) = (loc_query.get(*ent), loc_query.get(target_ent)) else {
            continue;
        };
        if caster_loc.flat_distance(target_loc) > crate::systems::behaviour::kite::KITER_REACH {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
        if !faces(heading_query.get(*ent).ok(), caster_loc, target_loc) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NotFacing } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < VOLLEY_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= VOLLEY_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        let attrs = attrs_query.get(*ent).expect("Volley caster must have ActorAttributes");
        for _ in 0..attrs.window_size() {
            commands.trigger(Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: attrs.force() * tuning.volley_force,
                    damage_type: DamageType::Physical,
                    ability: Some(AbilityType::Volley),
                    dot: 0.0,
                },
            });
        }

        use common_bevy::components::status::Timed;
        for (who, slot) in [
            (target_ent, Timed { pace: 1.0 - tuning.volley_slow, remaining: tuning.volley_slow_secs }),
            (*ent, Timed { pace: tuning.volley_run, remaining: tuning.volley_run_secs }),
        ] {
            let mut status = status_query.get(who).copied().unwrap_or_default();
            if who == target_ent { status.slow = Some(slot) } else { status.run = Some(slot) }
            commands.entity(who).insert(status);
            writer.write(Do {
                event: GameEvent::Incremental { ent: who, component: common_bevy::message::Component::Status(status) },
            });
        }

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Volley, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Volley), AbilityType::Volley));
    }
}
