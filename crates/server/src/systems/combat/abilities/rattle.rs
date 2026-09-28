use bevy::prelude::*;
use common_bevy::systems::targeting::faces;
use common_bevy::{
    components::{dazed::Dazed, resources::*, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

pub const RATTLE_STAMINA_COST: f32 = 40.0;

/// The least pace a daze leaves: a dazed actor still moves and swings.
const MIN_PACE: f32 = 0.1;

/// Handle Rattle, the Juggernaut's signature: a strike on a target within
/// melee reach that adds a stack to its daze (`Dazed`), up to `rattle_stacks`.
/// Each stack takes `rattle_daze` of its pace, its movement, auto-attacks
/// and recovery alike. The strike is Vitality's: `rattle_health` of the
/// Juggernaut's own health, and `rattle_growth` more for each stack already
/// on the target, so the longer a fight runs the harder a Juggernaut hits
/// and the less the target escapes or presses it.
pub fn handle_rattle(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    dazed_query: Query<&Dazed>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    heading_query: Query<&common_bevy::components::heading::Heading>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    health_query: Query<&Health>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Rattle, target } } = event else {
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
        let distance = caster_loc.flat_distance(target_loc);
        if distance < 1 || distance > common_bevy::components::AttackRange::default().0 {
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
        if stamina.state < RATTLE_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= RATTLE_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        let held = dazed_query.get(target_ent).map_or(0, |h| h.stacks);
        let stacks = held.saturating_add(1).min(tuning.rattle_stacks);
        let dazed = Dazed {
            stacks,
            pace: (1.0 - tuning.rattle_daze * stacks as f32).max(MIN_PACE),
        };
        commands.entity(target_ent).insert(dazed);
        writer.write(Do {
            event: GameEvent::Incremental { ent: target_ent, component: common_bevy::message::Component::Dazed(dazed) },
        });
        let bulk = health_query.get(*ent).map_or(0.0, |health| health.max);
        commands.trigger(Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: bulk * tuning.rattle_health * (1.0 + tuning.rattle_growth * held as f32),
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::Rattle),
                dot: 0.0,
            },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Rattle, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Rattle), AbilityType::Rattle));
    }
}
