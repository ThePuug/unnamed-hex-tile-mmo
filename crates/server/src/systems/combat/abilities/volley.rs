use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

/// How far a Volley reaches: the Kiter's own auto-attack range.
pub const VOLLEY_RANGE: u32 = 6;

pub const VOLLEY_STAMINA_COST: f32 = 20.0;

/// Handle Volley, the Kiter's signature: a burst of shots at a target within
/// `VOLLEY_RANGE`, one shot per threat the Kiter can see — its Concentration
/// window, so Focus decides how many land — each striking for
/// `ArchetypeTuning::volley_force` of Force. Every shot is its own threat.
/// The burst slows its target by `volley_slow` for `volley_slow_secs`, which
/// is what lets a Kiter open the gap its turn to flee costs it.
pub fn handle_volley(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
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
        if caster_loc.flat_distance(target_loc) as u32 > VOLLEY_RANGE {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
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
                },
            });
        }

        let slowed = common_bevy::components::slowed::Slowed { pace: 1.0 - tuning.volley_slow, remaining: tuning.volley_slow_secs };
        commands.entity(target_ent).insert(slowed);
        writer.write(Do {
            event: GameEvent::Incremental { ent: target_ent, component: common_bevy::message::Component::Slowed(slowed) },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Volley, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Volley), AbilityType::Volley));
    }
}
