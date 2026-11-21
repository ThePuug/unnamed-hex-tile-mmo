//! Runes at work (`common_bevy::components::rune`): cut ahead as its
//! skill's activation ends ([`cut`]), each standing on its tile as an
//! entity on its caster's side that a blow strikes down
//! (`Abilities::attack`), lapsing at its end ([`lapse`]). A control rune
//! pulses ([`control`]): each pulse is the rune's own activation, shown on
//! every foe then in its reach, and as it ends works on those still
//! inside, measured from the rune wherever the foe then stands. A rune
//! holds no health and no attributes of its own, so nothing that seeks a
//! fighter finds one.

use std::time::Duration;

use bevy::prelude::*;
use qrz::Convert;
use common_bevy::{
    components::{
        activation::Activation,
        entity_type::EntityType,
        loaded_by::LoadedBy,
        rune::{Rune, RuneKind},
        ActorAttributes, Loc,
    },
    message::{Do, Event as GameEvent, Outcome},
    moment::Moment,
    plugins::nntree::NearestNeighbor,
    tuning::Tuning,
};

use crate::abilities::{Abilities, AbilityFailReason, Cast};

/// A rune standing on its tile: which, whose, what of its caster it works
/// with, until when, and for a control rune its pulses.
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
    /// When its next pulse begins, on the game clock
    pub next_pulse: Moment,
    /// The pulse winding up: when it ends, on the game clock, and every
    /// foe that stood in its reach as it began
    pub pulse: Option<(Moment, Vec<Entity>)>,
}

/// Where `cast`'s rune would stand: the tile `distance` tiles ahead of its
/// caster along its heading, measured from where it stands in its tile, so
/// each step of a turn moves it; walked to over the ground as a move is,
/// inside an NPC's leash. None where the ground ahead takes no step.
pub fn ahead(abilities: &Abilities, cast: &Cast, rune: Rune) -> Option<Loc> {
    let tuning = *abilities.tuning;
    let dir = cast.heading.unwrap_or_default().to_world_dir();
    let offset = abilities.positions.get(cast.ent).map_or(Vec3::ZERO, |position| position.offset);
    let spacing = abilities.map.radius() * 3f32.sqrt();
    let reach = Vec3::new(offset.x, 0.0, offset.z) + Vec3::new(dir.x, 0.0, dir.y) * spacing * distance(&tuning, rune) as f32;
    let at = *cast.loc;
    let to = at + qrz::Qrz { z: 0, ..abilities.map.convert(reach) };
    let steps = at.flat_distance(&to).max(1) as usize;
    crate::leap::away(&abilities.map, at, at - (to - at), steps, abilities.leash(cast.ent)).map(Loc::new)
}

/// The tile `cast`'s rune would be cut on ([`ahead`]). Refused where the
/// ground ahead takes no step, or the tile holds anyone's rune.
pub fn room(abilities: &Abilities, cast: &Cast, rune: Rune) -> Result<Loc, AbilityFailReason> {
    let at = ahead(abilities, cast, rune).ok_or(AbilityFailReason::NoRoom)?;
    if abilities.runes.iter().any(|(_, loc, _)| loc.q == at.q && loc.r == at.r) {
        return Err(AbilityFailReason::NoRoom);
    }
    Ok(at)
}

/// Cuts `rune` for `cast` on the tile ahead ([`room`]), lifting the rune
/// of its kind the caster had standing.
pub fn cut(abilities: &mut Abilities, cast: &Cast, rune: Rune) -> Result<(), AbilityFailReason> {
    let tuning = *abilities.tuning;
    let at = room(abilities, cast, rune)?;
    let lifted: Vec<Entity> = abilities.runes.iter()
        .filter(|(_, _, stone)| stone.caster == cast.ent && stone.rune.kind() == rune.kind())
        .map(|(ent, ..)| ent)
        .collect();
    for ent in lifted {
        abilities.writer.write(Do { event: GameEvent::Despawn { ent, died: false } });
    }
    let reach = reach_of(&tuning, &cast.attrs, rune);
    let until = Moment::ZERO + abilities.time.elapsed() + Duration::from_secs_f32(length(&tuning, rune));
    let stone = RuneStone { rune, caster: cast.ent, attrs: cast.attrs, reach, until, next_pulse: Moment::ZERO, pulse: None };
    let mut entity = abilities.commands.spawn((EntityType::Rune(rune), at, stone, LoadedBy::default()));
    if let Some(side) = cast.side {
        entity.insert(side);
    }
    let ent = entity.id();
    abilities.commands.entity(ent).insert(NearestNeighbor::new(ent, at));
    Ok(())
}

/// Each control rune's pulses, the first as it is cut and one every
/// `Tuning::rune_pulse` after. A pulse winds up for `Tuning::rune_windup`,
/// told as the rune's activation on every foe then in its reach; as it
/// ends it works on each of those still inside and alive ([`work`]) and
/// lets the rest go. A foe stays caught, pulse after pulse, until it
/// leaves the reach or the rune falls.
pub fn control(mut abilities: Abilities) {
    let tuning = *abilities.tuning;
    let now = abilities.game_now();
    let stones: Vec<(Entity, Loc, RuneStone)> = abilities.runes.iter()
        .filter(|(_, _, stone)| stone.rune.kind() == RuneKind::Control)
        .map(|(ent, &loc, stone)| (ent, loc, stone.clone()))
        .collect();
    for (ent, loc, stone) in stones {
        let Some(ability) = stone.rune.ability() else { continue };
        let Ok(&side) = abilities.rune_sides.get(ent) else { continue };
        let inside = |abilities: &Abilities, foe: Entity| abilities.lives(foe) && abilities.actors.get(foe)
            .is_ok_and(|(at, _, _, theirs, ..)| theirs.is_some_and(|theirs| side.is_hostile_to(*theirs)) && at.flat_distance(&loc) <= stone.reach);

        let mut pulse = stone.pulse;
        if let Some((_, caught)) = pulse.take_if(|(ends, _)| *ends <= now) {
            for foe in caught {
                if inside(&abilities, foe) {
                    abilities.told(ent, ability, Some(foe), 0.0, Outcome::Landed);
                    work(&mut abilities, stone.rune, loc, foe);
                } else {
                    abilities.told(ent, ability, Some(foe), 0.0, Outcome::Missed);
                }
            }
        }
        let mut next_pulse = stone.next_pulse;
        if pulse.is_none() && now >= next_pulse {
            next_pulse = now + Duration::from_secs_f32(tuning.rune_pulse);
            let caught: Vec<Entity> = abilities.nntree.within_tiles(loc, stone.reach as i64 + 1)
                .map(|nn| nn.ent)
                .filter(|&foe| inside(&abilities, foe))
                .collect();
            let length = Duration::from_secs_f32(tuning.rune_windup);
            for &foe in &caught {
                let activation = Activation { ability, target: Some(foe), began: now, length };
                abilities.writer.write(Do { event: GameEvent::Activate { ent, activation } });
            }
            pulse = (!caught.is_empty()).then_some((now + length, caught));
        }
        if let Ok((_, _, mut standing)) = abilities.runes.get_mut(ent) {
            standing.next_pulse = next_pulse;
            standing.pulse = pulse;
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

/// What a control rune's pulse does to `foe` as it ends, from the rune's
/// tile `at`. Drag pulls it all the way to the rune from wherever it then
/// stands; the ground and the foe's leash alone stop it short
/// (`Abilities::drag`).
fn work(abilities: &mut Abilities, rune: Rune, at: Loc, foe: Entity) {
    if rune == Rune::Drag {
        abilities.drag(foe, at, i8::MAX);
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
