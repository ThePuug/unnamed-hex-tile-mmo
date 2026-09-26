use bevy::prelude::*;
use common_bevy::{
    components::{resources::*, Loc, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    resources::map::Map,
};

/// A Disengage fires when its target is this close or closer.
pub const DISENGAGE_TRIGGER: u32 = 2;

pub const DISENGAGE_STAMINA_COST: f32 = 20.0;

/// Tiles leapt, each one the neighbour furthest from the target
const LEAP_TILES: usize = 3;

/// Handle Disengage, the Kiter's signature: with its target within
/// `DISENGAGE_TRIGGER`, it leaps `LEAP_TILES` tiles straight away from it,
/// back to where its auto-attack reaches and a melee attacker must close again.
pub fn handle_disengage(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<&Loc>,
    mut stamina_query: Query<&mut Stamina>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    map: Res<Map>,
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
        if caster_loc.flat_distance(target_loc) as u32 > DISENGAGE_TRIGGER {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
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
        for _ in 0..LEAP_TILES {
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

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Disengage, target: *target } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Disengage), AbilityType::Disengage));
    }
}
