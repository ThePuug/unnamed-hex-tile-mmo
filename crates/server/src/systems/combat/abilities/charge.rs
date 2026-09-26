use std::ops::RangeInclusive;

use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, stagger::Stagger, Loc, reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
};

/// How far a Charge reaches. Beside its target it is a strike and a stagger;
/// further off it also closes the distance.
pub const CHARGE_RANGE: RangeInclusive<u32> = 1..=6;

pub const CHARGE_STAMINA_COST: f32 = 40.0;

/// Share of Force the impact deals
const FORCE_SHARE: f32 = 0.6;

/// How long the target is held in place, so a Kiter cannot step straight back out of reach.
const STAGGER_SECS: f32 = 1.0;

/// Handle Charge, the Juggernaut's signature: it rushes a target within
/// `CHARGE_RANGE`, lands beside it, strikes for `FORCE_SHARE` of Force and
/// staggers it for `STAGGER_SECS`. Nothing outruns a Juggernaut for long.
pub fn handle_charge(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Charge, target } } = event else {
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
        let distance = caster_loc.flat_distance(target_loc) as u32;
        if !CHARGE_RANGE.contains(&distance) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < CHARGE_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= CHARGE_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        // Land on the target's near side
        let landing = (**target_loc).neighbors().into_iter()
            .min_by_key(|neighbor| caster_loc.flat_distance(neighbor))
            .unwrap_or(**target_loc);
        writer.write(Do {
            event: GameEvent::Displace { ent: *ent, destination: landing + qrz::Qrz::Z, duration_ms: (distance as u16 * 60).max(120) },
        });
        commands.entity(*ent).insert((Loc::new(landing), common_bevy::components::position::Position::at_tile(landing)));
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Loc(Loc::new(landing)) },
        });

        commands.entity(target_ent).insert(Stagger::new(STAGGER_SECS));
        let attrs = attrs_query.get(*ent).expect("Charge caster must have ActorAttributes");
        commands.trigger(Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: attrs.force() * FORCE_SHARE,
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::Charge),
            },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Charge, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Charge), AbilityType::Charge));
    }
}
