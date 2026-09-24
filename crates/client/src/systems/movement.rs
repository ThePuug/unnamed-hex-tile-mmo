//! Movement on the client: the local player predicted from its open inputs,
//! every other entity simulated from its last intent, and the tile updates,
//! intents and displacements that re-anchor them. All of it drives
//! `VisualPosition`; `Position` is what physics says and `Transform` is
//! read from the visual alone.

use std::time::Duration;

use bevy::prelude::*;

use crate::components::RemoteMotion;
use crate::resources::RenderOrigin;
use common_bevy::{
    components::{
        displacing::Displacing,
        equipment::Burdened,
        heading::Heading,
        keybits::*,
        position::{Position, VisualPosition},
        ActorAttributes, AirTime, Loc, Turn,
    },
    message::{Component, Event, *},
    plugins::nntree::NNTree,
    resources::{map::Map, InputQueues},
    systems::movement::{calculate_movement, speed, MovementInput, JUMP_DURATION_MS, MOVEMENT_SPEED, TURN_REPEAT_MS},
};

/// Interpolation span in fixed ticks. One tick completes inside a single
/// frame and reintroduces jitter on direction changes as the predicted point
/// shifts; two smooths it.
const VISUAL_TICKS: f32 = 2.0;

/// A `Loc` this many tiles from the previous one is a teleport.
const TELEPORT_THRESHOLD_HEXES: i32 = 2;

/// A simulated remote entity further than this many tile spacings from its
/// authoritative tile is snapped to it.
const DRIFT_LIMIT_TILES: f32 = 1.5;

/// Seconds a `Loc` update with nothing else driving the entity takes to
/// interpolate to the tile centre.
const LOC_SETTLE_SECS: f32 = 0.125;

/// Replays the local player's open inputs from its confirmed position and
/// turn state, and points the visual and `Heading` at the result.
/// `Position` and `Turn` are never written here; they are the confirmed
/// state.
pub fn predict_local_player(
    fixed_time: Res<Time<Fixed>>,
    origin: Res<RenderOrigin>,
    mut query: Query<(&Position, &Turn, &mut Heading, &mut AirTime, &mut VisualPosition, Option<&ActorAttributes>, Has<Burdened>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
    buffers: Res<InputQueues>,
) {
    let tick = fixed_time.timestep().as_secs_f32();

    for (ent, buffer) in buffers.iter() {
        assert!(!buffer.queue.is_empty(), "Queue invariant violation: entity {ent} has empty queue");
        let Ok((position, turn, mut heading, mut airtime, mut visual, attrs, burdened)) = query.get_mut(ent) else { continue; };
        let movement_speed = speed(attrs.map_or(MOVEMENT_SPEED, |a| a.movement_speed()), burdened);

        let (mut offset, mut air) = (position.offset, airtime.state);
        let (mut facing, mut since_step_ms) = (turn.heading, turn.since_step_ms);
        for input in buffer.queue.iter().rev() {
            let Event::Input { key_bits, dt, .. } = input else { unreachable!() };
            if key_bits.is_pressed(KB_JUMP) && air.is_none() { air = Some(JUMP_DURATION_MS); }
            let out = calculate_movement(MovementInput {
                position: Position::new(position.tile, offset),
                heading: facing,
                moving: key_bits.moving(),
                back: key_bits.back(),
                turn: key_bits.turn(),
                since_step_ms,
                airtime: air,
                movement_speed,
            }, *dt as i16, &map, &nntree);
            (offset, air, facing, since_step_ms) = (out.position.offset, out.airtime, out.heading, out.since_step_ms);
        }

        if *heading != facing { *heading = facing; }
        airtime.step = air;
        let predicted = origin.render(&map, &Position::new(position.tile, offset));
        visual.interpolate_toward(predicted, tick * VISUAL_TICKS);
    }
}

/// Advances every remote entity by the fixed tick with the physics the
/// server runs, from the state its last intent gave, and points the visual
/// at the result. A displacing entity is driven by its slide instead.
pub fn simulate_remote(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    origin: Res<RenderOrigin>,
    mut query: Query<(Entity, &mut RemoteMotion, &Heading, &mut Position, &mut AirTime, &mut VisualPosition, Option<&ActorAttributes>, Has<Burdened>), Without<Displacing>>,
    buffers: Res<InputQueues>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    let delta_us = time.delta().as_micros() as u32;
    let tick = fixed_time.timestep().as_secs_f32();

    for (ent, mut motion, heading, mut position, mut airtime, mut visual, attrs, burdened) in &mut query {
        if buffers.get(&ent).is_some() { continue; }
        motion.residual_us += delta_us;
        let dt = (motion.residual_us / 1000) as u16;
        motion.residual_us %= 1000;
        if dt > 0 {
            let movement_speed = speed(attrs.map_or(MOVEMENT_SPEED, |a| a.movement_speed()), burdened);
            let out = calculate_movement(MovementInput {
                position: *position,
                heading: *heading,
                moving: motion.moving,
                back: motion.back,
                turn: 0,
                since_step_ms: TURN_REPEAT_MS,
                airtime: airtime.state,
                movement_speed,
            }, dt as i16, &map, &nntree);
            position.offset = out.position.offset;
            airtime.state = out.airtime;
            airtime.step = out.airtime;
        }
        visual.interpolate_toward(origin.render(&map, &position), tick * VISUAL_TICKS);
    }
}

/// Advance VisualPosition interpolation once per frame, before anything
/// reads it: `current()` then holds through the schedules that follow,
/// since every re-target starts from it.
pub fn advance_interpolation(
    time: Res<Time>,
    mut query: Query<&mut VisualPosition>,
) {
    let delta = time.delta_secs();
    for mut visual in &mut query {
        visual.advance(delta);
    }
}

/// Takes a remote entity's simulation state from an intent. The local
/// player is predicted from input and skips its own.
pub fn apply_intent(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut query: Query<(&mut Position, &mut Heading, &mut AirTime, Option<&mut RemoteMotion>, Has<Burdened>)>,
    buffers: Res<InputQueues>,
) {
    for message in reader.read() {
        let Do { event: Event::MovementIntent { ent, position, heading, moving, back, airtime, burdened } } = message else { continue };
        let ent = *ent;
        if buffers.get(&ent).is_some() { continue; }
        let Ok((mut position0, mut heading0, mut airtime0, motion, was_burdened)) = query.get_mut(ent) else { continue; };
        match (*burdened, was_burdened) {
            (true, false) => { commands.entity(ent).insert(Burdened); }
            (false, true) => { commands.entity(ent).remove::<Burdened>(); }
            _ => {}
        }
        *position0 = *position;
        if *heading0 != *heading { *heading0 = *heading; }
        airtime0.state = *airtime;
        match motion {
            Some(mut motion) => { motion.moving = *moving; motion.back = *back; }
            None => { commands.entity(ent).insert(RemoteMotion { moving: *moving, back: *back, residual_us: 0 }); }
        }
    }
}

/// Starts an ability displacement: a terrain-following slide to the
/// destination over its duration, for the local player and remote entities
/// alike. `Displacing` marks it until the matching `Loc` arrives.
pub fn apply_displace(
    origin: Res<RenderOrigin>,
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut query: Query<(&Loc, &mut VisualPosition)>,
    map: Res<Map>,
) {
    for message in reader.read() {
        let Do { event: Event::Displace { ent, destination, duration_ms } } = message else { continue };
        let (ent, destination, duration_ms) = (*ent, *destination, *duration_ms);
        let Ok((loc, mut visual)) = query.get_mut(ent) else { continue; };

        let duration_secs = duration_ms as f32 / 1000.0;
        let flat_dist = loc.flat_distance(&destination);
        let dest_world: Vec3 = origin.render_tile(&map, destination);

        if flat_dist > 1 {
            // Path on floor tiles: Loc and the destination stand one level up.
            let current_floor = map.get_by_qr(loc.q, loc.r).map(|(f, _)| f).unwrap_or(**loc);
            let dest_floor = qrz::Qrz { q: destination.q, r: destination.r, z: destination.z - 1 };
            let path = map.greedy_path(current_floor, dest_floor, flat_dist as usize);
            if path.is_empty() {
                visual.interpolate_toward(dest_world, duration_secs);
            } else {
                let waypoints: Vec<Vec3> = path.iter().map(|&tile| origin.render_tile(&map, tile + qrz::Qrz::Z)).collect();
                visual.interpolate_along_path(&waypoints, duration_secs);
            }
        } else {
            visual.interpolate_toward(dest_world, duration_secs);
        }

        if let Ok(mut e) = commands.get_entity(ent) {
            e.insert(Displacing { destination, duration_ms });
        }
    }
}

/// Applies a `Loc` update to the entity's position and visual. A slide in
/// progress ends when its destination arrives and ignores tiles short of it;
/// a jump of two tiles or more snaps; a simulated remote entity is re-anchored
/// to the tile, keeping its world position unless it has drifted; the local
/// player keeps its confirmed position, which carries its own tile; and an
/// entity nothing drives settles to the tile centre.
pub fn do_loc(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut query: Query<(&mut Loc, &mut Position, &mut VisualPosition, Option<&Displacing>, Option<&RemoteMotion>)>,
    buffers: Res<InputQueues>,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
) {
    let drift_limit = DRIFT_LIMIT_TILES * 3f32.sqrt() * map.radius();

    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::Loc(loc) } } = message else { continue };
        let (ent, loc) = (*ent, *loc);
        let Ok((mut loc0, mut position, mut visual, displacing, motion)) = query.get_mut(ent) else { continue; };
        // The tile's centre as drawn.
        let centre: Vec3 = origin.render_tile(&map, *loc);
        let is_local = buffers.get(&ent).is_some();

        if let Some(displacing) = displacing {
            if loc.flat_distance(&displacing.destination) == 0 {
                if let Ok(mut e) = commands.get_entity(ent) {
                    e.remove::<Displacing>();
                }
                *position = Position::at_tile(*loc);
            }
        } else if loc0.flat_distance(&loc) >= TELEPORT_THRESHOLD_HEXES {
            *position = Position::at_tile(*loc);
            visual.snap_to(centre);
        } else if motion.is_some() {
            let mut rebased = *position;
            rebased.rebase(*loc, &map);
            if rebased.offset.xz().length() > drift_limit {
                *position = Position::at_tile(*loc);
                visual.interpolate_toward(centre, LOC_SETTLE_SECS);
            } else {
                *position = rebased;
            }
        } else if !is_local {
            *position = Position::at_tile(*loc);
            visual.interpolate_toward(centre, LOC_SETTLE_SECS);
        }

        *loc0 = loc;
    }
}

#[allow(dead_code)]
fn _duration_ms(duration_ms: u16) -> Duration {
    Duration::from_millis(duration_ms as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::{Convert, Qrz};

    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop))
    }

    /// An adjacent Loc re-anchors without moving the entity in the world.
    #[test]
    fn world_space_preserved_on_smooth_tile_crossing() {
        let map = create_test_map();
        let old = Position::new(Qrz { q: 5, r: 5, z: 0 }, Vec3::new(0.5, 0.0, 0.3));
        let before = old.to_world(&map);

        let new_tile = Qrz { q: 6, r: 5, z: 0 };
        let centre: Vec3 = map.convert(new_tile);
        let rebased = Position::new(new_tile, before - centre);

        assert!((rebased.to_world(&map) - before).length() < 0.001);
    }

    #[test]
    fn visual_follows_the_predicted_point_without_jumping() {
        let map = create_test_map();
        let start = Position::at_tile(Qrz { q: 0, r: 0, z: 0 });
        let mut visual = VisualPosition::at(start.to_world(&map));

        let east = Position::new(start.tile, Vec3::new(0.5, 0.0, 0.0)).to_world(&map);
        visual.interpolate_toward(east, 0.125);
        visual.advance(0.0625);
        let before = visual.current();

        let west = Position::new(start.tile, Vec3::new(-0.5, 0.0, 0.0)).to_world(&map);
        visual.interpolate_toward(west, 0.125);
        assert!((visual.current() - before).length() < 0.001, "a redirect starts from what is on screen");
    }
}
