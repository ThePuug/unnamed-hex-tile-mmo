use bevy::prelude::*;
use common_bevy::{
    components::{hamstrung::Hamstrung, resources::*, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

pub const HAMSTRING_STAMINA_COST: f32 = 40.0;

/// Handle Hamstring, the Juggernaut's signature: a strike on an adjacent
/// target for `ArchetypeTuning::hamstring_force` of Force that adds a stack
/// to its `Hamstrung`, up to `hamstring_stacks`. Each stack takes
/// `hamstring_slow` of its speed and strips `hamstring_shred` of its
/// Toughness, so a Juggernaut grows more dangerous the longer a fight runs.
pub fn handle_hamstring(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    hamstrung_query: Query<&Hamstrung>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Hamstring, target } } = event else {
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
        if caster_loc.flat_distance(target_loc) != 1 {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < HAMSTRING_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= HAMSTRING_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        let stacks = hamstrung_query.get(target_ent).map_or(0, |h| h.stacks).saturating_add(1).min(tuning.hamstring_stacks);
        let hamstrung = Hamstrung {
            stacks,
            pace: (1.0 - tuning.hamstring_slow * stacks as f32).max(0.0),
            shred: (tuning.hamstring_shred * stacks as f32).min(1.0),
        };
        commands.entity(target_ent).insert(hamstrung);
        writer.write(Do {
            event: GameEvent::Incremental { ent: target_ent, component: common_bevy::message::Component::Hamstrung(hamstrung) },
        });
        let attrs = attrs_query.get(*ent).expect("Hamstring caster must have ActorAttributes");
        commands.trigger(Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: attrs.force() * tuning.hamstring_force,
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::Hamstring),
            },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Hamstring, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Hamstring), AbilityType::Hamstring));
    }
}
