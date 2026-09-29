use bevy::prelude::*;
use common_bevy::{
    components::{
        engagement::EngagementMember, heading::Heading, hex_assignment::{AssignedHex, HexAssignment},
        position::Position, resources::*, AttackRange, Loc, Turn,
        reaction_queue::DamageType, recovery::{GlobalRecovery, get_ability_recovery_duration},
    },
    message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
    plugins::nntree::NNTree,
    resources::map::Map,
};

/// How long the circle to the target's back takes, however far round it
/// goes: the Ambusher's clip scuttles for this long before it strikes.
pub const FLANK_CIRCLE_MS: u16 = 1500;

/// Handle Flank, the Ambusher's signature: on a target within melee reach,
/// a circle round it on its ring to the tile at its back at reach, turned
/// to face it so its auto-attacks carry on from there, and a strike for
/// `flank_intuition` of the caster's Intuition that stuns for
/// `Tuning::flank_stun` seconds as it lands (`landing::land`).
/// The strike waits in the target's queue like any threat, so the target
/// has its window to answer it before the stun holds it: `Stunned` stops
/// its movement and auto-attacks, and a lockout as long its abilities and
/// reactions.
/// The back tile becomes the Ambusher's assigned tile, so it holds the flank;
/// an engagement member assigned there takes the tile the Ambusher left.
/// With its back tile taken or not standable, the Ambusher strikes from
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
    member_query: Query<&EngagementMember>,
    mut assignment_query: Query<&mut HexAssignment>,
    mut writer: MessageWriter<Do>,
) {
    let tuning = common_bevy::tuning::tuning();
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
        let (Ok((caster_loc, caster_heading)), Ok((target_loc, target_heading))) = (loc_query.get(*ent), loc_query.get(target_ent)) else {
            continue;
        };
        let distance = caster_loc.flat_distance(target_loc);
        if distance < 1 || distance > AttackRange::default().0 {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OutOfRange } });
            continue;
        }
        if !super::in_arc(caster_heading, attrs_query.get(*ent).ok(), caster_loc, target_loc) {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::NotFacing } });
            continue;
        }
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };
        if stamina.state < tuning.flank_cost {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::InsufficientStamina } });
            continue;
        }
        stamina.state -= tuning.flank_cost;
        stamina.step = stamina.state;
        writer.write(Do {
            event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Stamina(*stamina) },
        });

        // The tile at the target's back at reach, a place on the ring the
        // assignment stands melee on, taken when its floor is there and no
        // one stands on it
        let back = target_heading.map(|heading| **target_loc + heading.reversed().hex_dir() * AttackRange::default().0);
        let landing = back.and_then(|back| map.get_by_qr(back.q, back.r)).map(|(floor, _)| floor + qrz::Qrz::Z)
            .filter(|landing| *landing != **caster_loc && nntree.locate_all_at_point(&Loc::new(*landing)).next().is_none());
        if let Some(landing) = landing {
            // Round the target on its ring while the strike waits in its
            // queue, arriving turned to it so its auto-attacks carry on from
            // its back
            writer.write(Do { event: GameEvent::Displace { ent: *ent, destination: landing + qrz::Qrz::Z, duration_ms: FLANK_CIRCLE_MS, around: Some(**target_loc) } });
            commands.entity(*ent).insert((Loc::new(landing), Position::at_tile(landing)));
            if let Some(facing) = Heading::between(&map, landing, **target_loc) {
                commands.entity(*ent).insert((facing, Turn { heading: facing, ..Turn::default() }));
            }
            writer.write(Do {
                event: GameEvent::Incremental { ent: *ent, component: common_bevy::message::Component::Loc(Loc::new(landing)) },
            });
            if let Ok(mut assignment) = member_query.get(*ent).and_then(|member| assignment_query.get_mut(member.0)) {
                let left = assignment.get(*ent);
                let holder = assignment.assignments.iter().find(|&(npc, hex)| *hex == landing && *npc != *ent).map(|(npc, _)| *npc);
                // The map keeps the fallen until the engagement next reassigns, so
                // the holder may be gone by the time the command lands
                match (holder, left) {
                    (Some(holder), Some(left)) => {
                        assignment.assignments.insert(holder, left);
                        commands.entity(holder).try_insert(AssignedHex(left));
                    }
                    (Some(holder), None) => {
                        assignment.remove(holder);
                        commands.entity(holder).try_remove::<AssignedHex>();
                    }
                    (None, _) => {}
                }
                assignment.assignments.insert(*ent, landing);
            }
            commands.entity(*ent).insert(AssignedHex(landing));
        }

        let attrs = attrs_query.get(*ent).expect("Flank caster must have ActorAttributes");
        commands.trigger(Try {
            event: GameEvent::DealDamage {
                source: *ent,
                target: target_ent,
                base_damage: attrs.intuition() * tuning.flank_intuition,
                damage_type: DamageType::Physical,
                ability: Some(AbilityType::Flank),
                dot: 0.0,
            },
        });

        writer.write(Do { event: GameEvent::UseAbility { ent: *ent, ability: AbilityType::Flank, target: Some(target_ent) } });
        super::stride(*ent, caster_heading, caster_loc, target_loc, &mut commands);
        commands.entity(*ent).insert(GlobalRecovery::new(get_ability_recovery_duration(AbilityType::Flank), AbilityType::Flank));
    }
}
