//! Player input: the clock guard on client-timed inputs, their application
//! to physics with confirmation, and the movement intents other clients
//! simulate from.

use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::{
    components::{
        heading::Heading, keybits::*, movement_intent_state::MovementIntentState,
        position::Position, resources::RespawnTimer, tier_lock::TierLock, *,
    },
    message::{Event, *},
    plugins::nntree::NNTree,
    resources::{map::Map, InputQueues},
    systems::{movement::{JUMP_DURATION_MS, MOVEMENT_SPEED}, physics},
};
use crate::{network::ServerNet, systems::stagger::Knockback, *};

/// Longest slice of time one input message may carry. Legitimate messages
/// carry a few ticks; this also keeps the i16 cast in physics unreachable.
pub const MAX_INPUT_DT_MS: u16 = 250;

/// Time a connection may have accepted ahead of real time. Above realistic
/// network bunching, so a stalled link never clamps; small enough that idle
/// or dead time banks less than a second of movement.
pub const CREDIT_CAP_MS: f32 = 1000.0;

/// Input messages accepted per second. A client sends about twenty; the
/// margin covers retransmit bunching.
pub const MAX_INPUTS_PER_SECOND: u16 = 256;

/// Time one input may accumulate before further extensions are dropped. The
/// client opens a new input every second, so only a misbehaving one gets here.
pub const MAX_OPEN_INPUT_MS: u16 = 2000;

/// Violations inside `VIOLATION_WINDOW_S` that disconnect the client.
pub const MAX_VIOLATIONS: u8 = 10;
pub const VIOLATION_WINDOW_S: f32 = 10.0;

/// The clock guard for one connection (INV-007): every millisecond physics
/// applies for a player was drawn from this credit, which refills at real
/// time. Refilled from `Time<Real>`: the default clock is virtual and clamps
/// a long frame, which would refill less than clients legitimately sent.
#[derive(Debug)]
pub struct InputGuard {
    credit_ms: f32,
    last_refill: f32,
    messages: u16,
    window_start: f32,
    violations: u8,
    violation_start: f32,
    open_seq: u8,
    open_key_bits: KeyBits,
    open_dt: u16,
}

enum Verdict {
    Apply(u16),
    Drop,
    Violation(&'static str),
}

impl InputGuard {
    fn new(now: f32) -> Self {
        Self {
            credit_ms: CREDIT_CAP_MS,
            last_refill: now,
            messages: 0,
            window_start: now,
            violations: 0,
            violation_start: now,
            open_seq: 1,
            open_key_bits: KeyBits::default(),
            open_dt: 0,
        }
    }

    fn accept(&mut self, now: f32, key_bits: KeyBits, dt: u16, seq: u8) -> Verdict {
        self.credit_ms = (self.credit_ms + (now - self.last_refill) * 1000.0).min(CREDIT_CAP_MS);
        self.last_refill = now;

        if now - self.window_start >= 1.0 {
            self.messages = 0;
            self.window_start = now;
        }
        self.messages = self.messages.saturating_add(1);
        if self.messages > MAX_INPUTS_PER_SECOND {
            return Verdict::Violation("input rate");
        }
        if dt > MAX_INPUT_DT_MS {
            return Verdict::Violation("input dt");
        }

        if seq == self.open_seq.wrapping_add(1) {
            self.open_seq = seq;
            self.open_key_bits = key_bits;
            self.open_dt = 0;
        } else if seq == self.open_seq {
            if !key_bits.same_input(&self.open_key_bits) {
                return Verdict::Violation("input changed under its seq");
            }
            if dt == 0 || self.open_dt.saturating_add(dt) > MAX_OPEN_INPUT_MS {
                return Verdict::Drop;
            }
        } else {
            return Verdict::Violation("input seq");
        }

        let applied = dt.min(self.credit_ms as u16);
        self.credit_ms -= applied as f32;
        self.open_dt += applied;
        if applied < dt {
            debug!("input clamped: {dt} ms requested, {applied} ms of credit");
        }
        Verdict::Apply(applied)
    }

    /// Records a violation; true when the client has earned a disconnect.
    fn violate(&mut self, now: f32) -> bool {
        if now - self.violation_start >= VIOLATION_WINDOW_S {
            self.violations = 0;
            self.violation_start = now;
        }
        self.violations = self.violations.saturating_add(1);
        self.violations >= MAX_VIOLATIONS
    }
}

/// One guard per player entity. An entity is fresh per connection, so no
/// state survives a reconnect.
#[derive(Default, Resource)]
pub struct InputGuards(pub HashMap<Entity, InputGuard>);

/// Passes each player input through its guard and forwards what may apply.
/// A new sequence is forwarded even with nothing to apply, since opening it
/// is what closes and confirms the previous one.
pub fn try_input(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut guards: ResMut<InputGuards>,
    mut net: ResMut<ServerNet>,
    lobby: Res<Lobby>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs();
    for message in reader.read() {
        let Try { event: Event::Input { ent, key_bits, dt, seq } } = message else { continue };
        let (ent, key_bits, dt, seq) = (*ent, *key_bits, *dt, *seq);
        let guard = guards.0.entry(ent).or_insert_with(|| InputGuard::new(now));
        let opened = seq == guard.open_seq.wrapping_add(1);
        match guard.accept(now, key_bits, dt, seq) {
            Verdict::Apply(applied) => {
                if applied > 0 || opened {
                    writer.write(Do { event: Event::Input { ent, key_bits, dt: applied, seq } });
                }
            }
            Verdict::Drop => {}
            Verdict::Violation(what) => {
                warn!("input violation from {ent}: {what}");
                if guard.violate(now) {
                    if let Some(&client_id) = lobby.get_by_right(&ent) {
                        warn!("disconnecting {client_id}: repeated input violations");
                        net.disconnect(client_id);
                    }
                }
            }
        }
    }
}

/// Applies accepted inputs to physics in arrival order. The queue holds one
/// entry, the open input (INV-002); a message with the next sequence closes
/// it, confirming the position it left the entity at, and replaces it in
/// place. A dead player's inputs keep the sequence moving but do not move it.
pub fn apply(
    mut reader: MessageReader<Do>,
    mut commands: Commands,
    mut buffers: ResMut<InputQueues>,
    mut query: Query<(&mut Heading, &mut Position, &mut AirTime, Option<&ActorAttributes>, Option<&RespawnTimer>)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for message in reader.read() {
        let Do { event: Event::Input { ent, key_bits, dt, seq } } = message else { continue };
        let (ent, key_bits, dt, seq) = (*ent, *key_bits, *dt, *seq);
        let Some(buffer) = buffers.get_mut(&ent) else { continue };
        let Ok((mut heading, mut position, mut airtime, attrs, dead)) = query.get_mut(ent) else { continue };
        let Some(front) = buffer.queue.front_mut() else {
            panic!("Queue invariant violation: entity {ent} has empty queue");
        };
        let Event::Input { seq: seq0, dt: dt0, .. } = front else { panic!("not input") };

        if seq == seq0.wrapping_add(1) {
            // Through commands: a reader and a writer of `Do` in one system is a
            // conflicting access (B0002).
            commands.write_message(Do { event: Event::Confirm { ent, seq: *seq0, position: *position, airtime: airtime.state } });
            *front = Event::Input { ent, key_bits, dt, seq };
        } else if seq == *seq0 {
            *dt0 = dt0.saturating_add(dt);
        } else {
            continue;
        }

        if dead.is_some() {
            continue;
        }

        let moving = key_bits.is_pressed(KB_MOVE);
        if moving && *heading != key_bits.heading {
            *heading = key_bits.heading;
        }
        if key_bits.is_pressed(KB_JUMP) && airtime.state.is_none() {
            airtime.state = Some(JUMP_DURATION_MS);
        }
        let movement_speed = attrs.map_or(MOVEMENT_SPEED, |a| a.movement_speed());
        let (offset, air) = physics::apply(*position, *heading, moving, airtime.state, movement_speed, dt as i16, &map, &nntree);
        position.offset = offset;
        airtime.state = air;
    }
}

/// Tells every client that has an entity loaded what to simulate it from:
/// sent when its heading, motion or airborne state changes and at every tile
/// crossing while it moves, so a lost intent is repaired within a tile.
/// Motion is what physics produced this tick, so an input that moved nothing
/// reports a stopped entity. A knocked-back entity is driven by its
/// displacement instead.
pub fn broadcast_movement_intent(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &Loc, &Position, &Heading, &AirTime, Option<&mut MovementIntentState>), Without<Knockback>>,
    map: Res<Map>,
) {
    for (ent, loc, position, heading, airtime, state) in &mut query {
        let Some(mut state) = state else {
            commands.entity(ent).insert(MovementIntentState {
                last_tick: *position,
                sent_heading: *heading,
                sent_tile: **loc,
                ..default()
            });
            continue;
        };

        let here = position.to_world(&map).xz();
        let there = state.last_tick.to_world(&map).xz();
        let moving = here.distance_squared(there) > 1e-8;
        let airborne = airtime.state.is_some();
        state.last_tick = *position;

        let changed = moving != state.sent_moving
            || *heading != state.sent_heading
            || airborne != state.sent_airborne
            || (moving && **loc != state.sent_tile);
        if !changed {
            continue;
        }
        state.sent_moving = moving;
        state.sent_heading = *heading;
        state.sent_airborne = airborne;
        state.sent_tile = **loc;

        writer.write(Do { event: Event::MovementIntent {
            ent,
            position: *position,
            heading: *heading,
            moving,
            airtime: airtime.state,
        }});
    }
}

/// Handle tier lock requests from clients

/// Clients send SetTierLock events when pressing 1/2/3 keys.
/// Server updates the TierLock component to reflect the chosen tier.
/// Abilities will validate the existing Target component is in the correct tier.
pub fn try_set_tier_lock(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut tier_locks: Query<&mut TierLock>,
) {
    for message in reader.read() {
        let Try { event } = message;
        let Event::SetTierLock { ent, tier } = event else { continue };
        let ent = *ent;
        let tier = *tier;

        if let Ok(mut tier_lock) = tier_locks.get_mut(ent) {
            tier_lock.set(tier);

            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: common_bevy::message::Component::TierLock(*tier_lock),
                },
            });
        }
    }
}

/// Handle attribute respec requests from clients

/// Clients send RespecAttributes Try events when clicking Apply button.
/// Server validates the respec (budget, ranges, not in combat) and broadcasts Do event.
pub fn try_respec_attributes(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut attrs_query: Query<&mut ActorAttributes>,
) {
    for message in reader.read() {
        let Try { event } = message;
        let Event::RespecAttributes {
            ent,
            might_grace_axis,
            might_grace_spectrum,
            might_grace_shift,
            vitality_focus_axis,
            vitality_focus_spectrum,
            vitality_focus_shift,
            instinct_presence_axis,
            instinct_presence_spectrum,
            instinct_presence_shift,
        } = event
        else {
            continue;
        };
        let ent = *ent;
        let might_grace_axis = *might_grace_axis;
        let might_grace_spectrum = *might_grace_spectrum;
        let might_grace_shift = *might_grace_shift;
        let vitality_focus_axis = *vitality_focus_axis;
        let vitality_focus_spectrum = *vitality_focus_spectrum;
        let vitality_focus_shift = *vitality_focus_shift;
        let instinct_presence_axis = *instinct_presence_axis;
        let instinct_presence_spectrum = *instinct_presence_spectrum;
        let instinct_presence_shift = *instinct_presence_shift;

        let Ok(mut attrs) = attrs_query.get_mut(ent) else {
            continue;
        };

        // Calculate draft investment
        let draft_investment = might_grace_axis.unsigned_abs() as u32
            + might_grace_spectrum.max(0) as u32
            + vitality_focus_axis.unsigned_abs() as u32
            + vitality_focus_spectrum.max(0) as u32
            + instinct_presence_axis.unsigned_abs() as u32
            + instinct_presence_spectrum.max(0) as u32;

        // Validate budget
        if draft_investment > attrs.total_level() {
            continue; // Overbudget
        }

        // Validate ranges (i8 max is 127, but level is practical limit)
        let max_investment = attrs.total_level() as i8;
        if might_grace_axis.abs() > max_investment
            || might_grace_spectrum < 0
            || might_grace_spectrum > max_investment
            || vitality_focus_axis.abs() > max_investment
            || vitality_focus_spectrum < 0
            || vitality_focus_spectrum > max_investment
            || instinct_presence_axis.abs() > max_investment
            || instinct_presence_spectrum < 0
            || instinct_presence_spectrum > max_investment
        {
            continue; // Invalid ranges
        }

        // Apply respec
        attrs.apply_respec(
            might_grace_axis,
            might_grace_spectrum,
            might_grace_shift,
            vitality_focus_axis,
            vitality_focus_spectrum,
            vitality_focus_shift,
            instinct_presence_axis,
            instinct_presence_spectrum,
            instinct_presence_shift,
        );

        // Broadcast confirmation
        writer.write(Do { event: event.clone() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moving() -> KeyBits {
        KeyBits { key_bits: KB_MOVE, heading: Heading::from_slot(3), accumulator: 0 }
    }

    #[test]
    fn a_new_seq_opens_and_the_same_seq_extends() {
        let mut guard = InputGuard::new(0.0);
        assert!(matches!(guard.accept(0.0, moving(), 0, 2), Verdict::Apply(0)));
        assert!(matches!(guard.accept(0.01, moving(), 16, 2), Verdict::Apply(16)));
        assert!(matches!(guard.accept(0.02, moving(), 0, 2), Verdict::Drop), "nothing to apply");
        assert!(matches!(guard.accept(0.03, moving(), 16, 4), Verdict::Violation(_)), "skipped seq");
        assert!(matches!(guard.accept(0.04, KeyBits::default(), 16, 2), Verdict::Violation(_)), "keys changed under the seq");
    }

    /// Time is drawn from real time: a burst spends the bank, then the
    /// guard admits only what has elapsed.
    #[test]
    fn credit_bounds_accepted_time() {
        let mut guard = InputGuard::new(0.0);
        let mut accepted = 0u32;
        for i in 0..20 {
            let seq = 2u8.wrapping_add(i as u8);
            if let Verdict::Apply(dt) = guard.accept(0.0, moving(), MAX_INPUT_DT_MS, seq) {
                accepted += dt as u32;
            }
        }
        assert_eq!(accepted, CREDIT_CAP_MS as u32, "an instant burst gets the cap and no more");
        assert!(matches!(guard.accept(0.0, moving(), 16, 22), Verdict::Apply(0)));
        assert!(matches!(guard.accept(0.5, moving(), 16, 23), Verdict::Apply(16)), "real time refills");
    }

    #[test]
    fn oversized_and_flooded_inputs_are_violations() {
        let mut guard = InputGuard::new(0.0);
        assert!(matches!(guard.accept(0.0, moving(), MAX_INPUT_DT_MS + 1, 2), Verdict::Violation(_)));
        let mut guard = InputGuard::new(0.0);
        let mut violations = 0;
        for _ in 0..=MAX_INPUTS_PER_SECOND {
            if let Verdict::Violation(_) = guard.accept(0.0, moving(), 1, 2) { violations += 1; }
        }
        assert_eq!(violations, 1, "the message past the rate is refused");
        assert!(!guard.violate(0.0));
        for _ in 1..MAX_VIOLATIONS - 1 { guard.violate(0.0); }
        assert!(guard.violate(0.0), "repeated violations disconnect");
    }
}
