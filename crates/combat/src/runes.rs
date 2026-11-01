//! Runes at work (`common_bevy::components::rune`): cut ahead by the gate
//! ([`cut`]), each standing on its tile as an entity on its caster's side
//! that a blow strikes down (`process_deal_damage`), lapsing at its end
//! ([`lapse`]). A control rune queues a threat of its own on each foe that
//! enters its reach ([`control`]): its caster's window and build, measured
//! from the rune as it lands (`resolve_drag`). A rune holds no health and
//! no attributes of its own, so nothing that seeks a fighter finds one.

use bevy::prelude::*;
use qrz::Convert;
use common_bevy::{
    components::{
        behaviour::Side,
        entity_type::EntityType,
        loaded_by::LoadedBy,
        reaction_queue::ReactionQueue,
        resources::{CombatState, Endurance, Health},
        rune::{Rune, RuneKind},
        ActorAttributes, Loc,
    },
    message::{Do, Event as GameEvent},
    moment::Moment,
    plugins::nntree::{NNTree, NearestNeighbor},
    systems::combat::{queue as queue_utils, state::enter_combat},
    tuning::Tuning,
};

use crate::abilities::{Abilities, AbilityFailReason, Cast};

/// A rune standing on its tile: which, whose, what of its caster it works
/// with, until when, and the foes inside its reach as it last looked.
#[derive(Clone, Component, Debug)]
pub struct RuneStone {
    pub rune: Rune,
    pub caster: Entity,
    /// Its caster's build as it was cut: a rune's work is its caster's, at
    /// its level and build, whether or not the caster still stands
    pub attrs: ActorAttributes,
    /// Hexes from its tile it reaches
    pub reach: i32,
    pub until: Moment,
    /// Foes inside its reach as it last looked, so one entering is told
    /// from one standing in it
    pub inside: Vec<Entity>,
}

/// Where `cast`'s rune would stand: the tile `distance` tiles ahead of its
/// caster along its heading, measured from where it stands in its tile, so
/// each step of a turn moves it; walked to over the ground as a move is,
/// inside an NPC's leash. None where the ground ahead takes no step.
pub fn ahead(abilities: &Abilities, cast: &Cast, rune: Rune) -> Option<Loc> {
    let tuning = *abilities.tuning;
    let (_, _, _, heading, ..) = abilities.actors.get(cast.ent).ok()?;
    let dir = heading.copied().unwrap_or_default().to_world_dir();
    let offset = abilities.positions.get(cast.ent).map_or(Vec3::ZERO, |position| position.offset);
    let spacing = abilities.map.radius() * 3f32.sqrt();
    let reach = Vec3::new(offset.x, 0.0, offset.z) + Vec3::new(dir.x, 0.0, dir.y) * spacing * distance(&tuning, rune) as f32;
    let at = *cast.loc;
    let to = at + qrz::Qrz { z: 0, ..abilities.map.convert(reach) };
    let steps = at.flat_distance(&to).max(1) as usize;
    crate::leap::away(&abilities.map, at, at - (to - at), steps, abilities.leash(cast.ent)).map(Loc::new)
}

/// Cuts `rune` for `cast` on the tile ahead ([`ahead`]), lifting the rune
/// of its kind the caster had standing. Refused where the ground ahead
/// takes no step, or the tile holds anyone's rune.
pub fn cut(abilities: &mut Abilities, cast: &Cast, rune: Rune) -> Result<Option<Entity>, AbilityFailReason> {
    let tuning = *abilities.tuning;
    let at = ahead(abilities, cast, rune).ok_or(AbilityFailReason::NoRoom)?;
    if abilities.runes.iter().any(|(_, loc, _)| loc.q == at.q && loc.r == at.r) {
        return Err(AbilityFailReason::NoRoom);
    }
    let lifted: Vec<Entity> = abilities.runes.iter()
        .filter(|(_, _, stone)| stone.caster == cast.ent && stone.rune.kind() == rune.kind())
        .map(|(ent, ..)| ent)
        .collect();
    for ent in lifted {
        abilities.writer.write(Do { event: GameEvent::Despawn { ent, died: false } });
    }
    let reach = reach_of(&tuning, &cast.attrs, rune);
    let until = Moment::ZERO + abilities.time.elapsed() + std::time::Duration::from_secs_f32(length(&tuning, rune));
    let stone = RuneStone { rune, caster: cast.ent, attrs: cast.attrs, reach, until, inside: Vec::new() };
    let mut entity = abilities.commands.spawn((EntityType::Rune(rune), at, stone, LoadedBy::default()));
    if let Some(side) = cast.side {
        entity.insert(side);
    }
    let ent = entity.id();
    abilities.commands.entity(ent).insert(NearestNeighbor::new(ent, at));
    Ok(None)
}

/// Each control rune's threat on each foe that has entered its reach since
/// it last looked: queued on the foe as its caster striking it would be
/// (`queue::create_threat`), its source the rune, carrying what the rune
/// does to it as it lands.
#[allow(clippy::type_complexity)]
pub fn control(
    tuning: Res<Tuning>,
    time: Res<Time>,
    runtime: Res<crate::RunTime>,
    nntree: Res<NNTree>,
    mut runes: Query<(Entity, &Loc, &Side, &mut RuneStone)>,
    mut foes: Query<(&Loc, &Side, &Health, &ActorAttributes, &mut ReactionQueue, Option<&Endurance>, Option<&mut CombatState>)>,
    mut writer: MessageWriter<Do>,
) {
    let now = runtime.now(&time);
    for (ent, &loc, &side, mut stone) in &mut runes {
        if stone.rune.kind() != RuneKind::Control {
            continue;
        }
        let inside: Vec<Entity> = nntree.within_tiles(loc, stone.reach as i64 + 1)
            .map(|nn| nn.ent)
            .filter(|&other| foes.get(other).is_ok_and(|(at, theirs, health, ..)| {
                side.is_hostile_to(*theirs) && health.state > 0.0 && at.flat_distance(&loc) <= stone.reach
            }))
            .collect();
        let entered: Vec<Entity> = inside.iter().copied().filter(|other| !stone.inside.contains(other)).collect();
        stone.inside = inside;
        for foe in entered {
            let Ok((_, _, _, attrs, mut queue, endurance, combat)) = foes.get_mut(foe) else { continue };
            let (stun, drag) = carried(&stone);
            let threat = queue_utils::create_threat(
                &tuning, ent, attrs, &stone.attrs, 0.0, stone.rune.ability(), now, 0.0, Endurance::fatigue_of(&tuning, endurance),
            ).carrying(stun, drag);
            queue_utils::insert_threat(&mut queue, threat);
            if let Some(mut combat) = combat {
                enter_combat(foe, &mut combat, &time, &mut writer);
            }
            writer.write(Do { event: GameEvent::InsertThreat { ent: foe, threat } });
        }
    }
}

/// Lifts each rune whose while has run out.
pub fn lapse(time: Res<Time>, runes: Query<(Entity, &RuneStone)>, mut writer: MessageWriter<Do>) {
    let now = Moment::ZERO + time.elapsed();
    for (ent, stone) in &runes {
        if now >= stone.until {
            writer.write(Do { event: GameEvent::Despawn { ent, died: false } });
        }
    }
}

/// What a control rune's threat carries to a foe: seconds of stun, and
/// tiles of drag toward the rune where positive. Drag's pull is as long as
/// the drag carries, so it reaches the rune from wherever the foe stands as
/// it lands, more tiles than a window's walk; the ground and the foe's
/// leash alone stop it short (`resolve_drag`).
fn carried(stone: &RuneStone) -> (f32, i8) {
    match stone.rune {
        Rune::Drag => (0.0, i8::MAX),
        _ => (0.0, 0),
    }
}

/// Hexes from its tile a rune cut by `attrs` reaches: Drag's raised by its
/// caster's Physique line. Unbuilt: Distract's raised by its Agility line.
pub fn reach_of(tuning: &Tuning, attrs: &ActorAttributes, rune: Rune) -> i32 {
    match rune.ability() {
        Some(ability) => attrs.line_tiles(tuning, ability, reach(tuning, rune)) as i32,
        None => reach(tuning, rune).round().max(0.0) as i32,
    }
}

/// Tiles ahead a rune is cut
pub fn distance(tuning: &Tuning, rune: Rune) -> usize {
    match rune {
        Rune::Drag => tuning.drag_distance.round().max(1.0) as usize,
        _ => 1,
    }
}

/// Hexes from its tile a rune reaches, at full commitment to its line
fn reach(tuning: &Tuning, rune: Rune) -> f32 {
    match rune {
        Rune::Drag => tuning.drag_reach,
        _ => 1.0,
    }
}

/// Seconds a rune stands
fn length(tuning: &Tuning, rune: Rune) -> f32 {
    match rune {
        Rune::Drag => tuning.drag_secs,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_line_raises_a_drag_runes_reach() {
        let tuning = Tuning::DEFAULT;
        let reach = |steps: i8| reach_of(&tuning, &ActorAttributes::new(0, 0, 0, -steps, 0, 0, 0, 0, 0), Rune::Drag);
        for steps in 0..ActorAttributes::STEPS as i8 {
            assert!(reach(steps + 1) >= reach(steps), "never less for more Physique, at {steps}");
        }
        assert!(reach(ActorAttributes::STEPS as i8) > reach(0), "further at full commitment than with none");
    }
}
