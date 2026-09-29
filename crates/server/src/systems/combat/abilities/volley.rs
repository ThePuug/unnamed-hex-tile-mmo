use bevy::prelude::*;
use common_bevy::systems::targeting::faces;
use common_bevy::{
    components::{resources::*, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};


/// Handle Volley, the Kiter's signature: a burst of shots at a target within
/// `KITER_REACH`, `Tuning::volley_shots` of them by the Kiter's Intensity,
/// so Focus, which gives its pace, decides how many fly — each striking for
/// `Tuning::volley_force` of Force. Every shot is its own threat.
/// Each shot, as it lands (`landing::land`), slows its target by
/// `volley_slow` for `volley_slow_secs`, and the shot that finds the target
/// unslowed leaps the Kiter `volley_leap` tiles straight away from it: the
/// gap opens only when the target can no longer close it.
pub fn handle_volley(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    heading_query: Query<&common_bevy::components::heading::Heading>,
    time: Res<Time>,
    runtime: Res<crate::resources::RunTime>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
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
        if stamina.state < tuning.volley_cost {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= tuning.volley_cost;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        // Its shots are queued now, the time a landing knows the burst by
        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let at = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
        commands.entity(*ent).insert(crate::systems::combat::landing::VolleyBurst { at, leapt: false });

        let attrs = attrs_query.get(*ent).expect("Volley caster must have ActorAttributes");
        for _ in 0..tuning.volley_shots[attrs.intensity().index()] {
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

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Volley, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Volley), AbilityType::Volley));
    }
}
