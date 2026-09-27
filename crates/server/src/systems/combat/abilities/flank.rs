use bevy::prelude::*;
use common_bevy::{
    components::{
        heading::Heading, position::Position, resources::*, stunned::Stunned, AttackRange, Loc,
        reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration},
    },
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    plugins::nntree::NNTree,
    resources::map::Map,
};

pub const FLANK_STAMINA_COST: f32 = 30.0;

/// Handle Flank, the Cutthroat's signature: on a target within melee reach,
/// a stun of `ArchetypeTuning::flank_stun` seconds, a step to the tile at its
/// back, and a strike for `flank_intuition` of the caster's Intuition. The
/// stun holds the target completely: `Stunned` stops its movement and
/// auto-attacks, and a lockout as long stops its abilities and reactions.
/// With its back tile taken or not standable, the Cutthroat strikes from
/// where it stands.
pub fn handle_flank(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    loc_query: Query<(&Loc, Option<&Heading>)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    respawn_query: Query<&RespawnTimer>,
    map: Res<Map>,
    nntree: Res<NNTree>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability: AbilityType::Flank, target } } = event else {
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
        let (Ok((caster_loc, _)), Ok((target_loc, target_heading))) = (loc_query.get(*ent), loc_query.get(target_ent)) else {
            continue;
        };
        let distance = caster_loc.flat_distance(target_loc);
        if distance < 1 || distance > AttackRange::default().0 {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < FLANK_STAMINA_COST {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= FLANK_STAMINA_COST;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        let stunned = Stunned { remaining: tuning.flank_stun };
        let lockout = recovery_query.get(target_ent).map_or(0.0, |recovery| recovery.remaining).max(tuning.flank_stun);
        commands.entity(target_ent).insert((stunned, GlobalRecovery::new(lockout, AbilityType::Flank)));
        writer.write(Do {
            event: GameEvent::Incremental { ent: target_ent, component: common_bevy::message::Component::Stunned(stunned) },
        });

        // The tile at the target's back, one standing tile from its own,
        // taken when its floor is there and no one stands on it
        let back = target_heading.map(|heading| **target_loc + heading.reversed().hex_dir());
        let landing = back.and_then(|back| map.get_by_qr(back.q, back.r)).map(|(floor, _)| floor + qrz::Qrz::Z)
            .filter(|landing| *landing != **caster_loc && nntree.locate_all_at_point(&Loc::new(*landing)).next().is_none());
        if let Some(landing) = landing {
            writer.write(Do { event: GameEvent::Displace { ent: *ent, destination: landing + qrz::Qrz::Z, duration_ms: 200 } });
            commands.entity(*ent).insert((Loc::new(landing), Position::at_tile(landing)));
            writer.write(Do {
                event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Loc(Loc::new(landing)) },
            });
        }

        let attrs = attrs_query.get(*ent).expect("Flank caster must have ActorAttributes");
        commands.trigger(Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: attrs.intuition() * tuning.flank_intuition,
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::Flank),
            },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Flank, target: Some(target_ent) } });
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Flank), AbilityType::Flank));
    }
}
