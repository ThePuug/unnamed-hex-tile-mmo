use bevy::prelude::*;
use common_bevy::{
    components::{status::Status, resources::*, Loc, reaction_queue::DamageType, recovery::GlobalRecovery},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};


/// Handle Rattle, the Juggernaut's signature: a strike on a target within
/// melee reach that, as it lands (`landing::land`), adds a stack to its
/// daze (`Status::daze`), up to `rattle_stacks`.
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
    status_query: Query<&Status>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    heading_query: Query<&common_bevy::components::heading::Heading>,
    health_query: Query<&Health>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
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
        if !super::in_arc(heading_query.get(*ent).ok(), attrs_query.get(*ent).ok(), caster_loc, target_loc) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NotFacing } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < tuning.rattle_cost {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= tuning.rattle_cost;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        let held = Status::stacks_of(status_query.get(target_ent).ok());
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
        super::stride(*ent, heading_query.get(*ent).ok(), caster_loc, target_loc, &mut commands);
        commands.entity(*ent).insert(common_bevy::systems::combat::synergies::lockout(AbilityType::Rattle, recovery_query.get(*ent).ok(), None, attrs_query.get(target_ent).ok()));
    }
}
