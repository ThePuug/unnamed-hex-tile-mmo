use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, Loc, reaction_queue::DamageType, recovery::GlobalRecovery},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};


/// Handle Volley, the Kiter's signature: a burst of `Tuning::volley_shots`
/// shots at a target within `KITER_REACH`, each striking for
/// `Tuning::volley_precision` of Precision. Every shot is its own threat.
/// Each shot, as it lands (`landing::land`), slows its target by
/// `volley_slow` for `volley_slow_secs`, and the first of the burst to land
/// leaps the Kiter `volley_leap` tiles straight away from it: the gap opens
/// only when the target can no longer close it.
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
        if !super::in_arc(heading_query.get(*ent).ok(), attrs_query.get(*ent).ok(), caster_loc, target_loc) {
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
        let damage = attrs.precision() * tuning.volley_precision;
        for _ in 0..tuning.volley_shots {
            commands.trigger(Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: damage,
                    damage_type: DamageType::Physical,
                    ability: Some(AbilityType::Volley),
                    dot: 0.0,
                },
            });
        }

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Volley, target: Some(target_ent) } });
        super::stride(*ent, heading_query.get(*ent).ok(), caster_loc, target_loc, &mut commands);
        commands.entity(*ent).insert(common_bevy::systems::combat::synergies::lockout(AbilityType::Volley, recovery_query.get(*ent).ok(), None, attrs_query.get(target_ent).ok()));
    }
}
