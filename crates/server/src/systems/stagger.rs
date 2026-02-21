use bevy::prelude::*;
use qrz::Qrz;
use common::{
    components::{position::Position, stagger::Stagger, Loc},
    message::{Do, Event as GameEvent},
    resources::map::Map,
};

/// Server-only: entity being pushed tile-by-tile over multiple ticks.
/// Each FixedUpdate tick (125ms) moves the entity 1 tile toward the destination.
/// Uses greedy neighbor selection (terrain-following). Stops on cliff or no progress.
#[derive(Component, Clone, Copy, Debug)]
pub struct Knockback {
    pub destination: Qrz,
    pub remaining_tiles: i32,
}

/// Tick stagger timers and remove expired ones.
/// Runs in FixedUpdate before behavior systems.
pub fn tick_stagger(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Stagger)>,
    dt: Res<Time>,
) {
    for (ent, mut stagger) in &mut query {
        stagger.remaining -= dt.delta_secs();
        if stagger.remaining <= 0.0 {
            commands.entity(ent).remove::<Stagger>();
        }
    }
}

/// Process knockback: move entity 1 tile per tick toward destination via greedy pathfinding.
/// Broadcasts Loc incremental for each step so clients see smooth 1-hex crossings.
/// Stops when: no progress, no walkable neighbors, or remaining_tiles exhausted.
pub fn process_knockback(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Loc, &mut Knockback)>,
    map: Res<Map>,
    mut writer: MessageWriter<Do>,
) {
    for (ent, mut loc, mut knockback) in &mut query {
        if knockback.remaining_tiles <= 0 {
            commands.entity(ent).remove::<Knockback>();
            continue;
        }

        // Find floor tile under current Loc (Loc is at standing height = floor + Z)
        let Some((floor, _)) = map.get_by_qr(loc.q, loc.r) else {
            commands.entity(ent).remove::<Knockback>();
            continue;
        };

        // Greedy: pick walkable neighbor closest to destination
        let best = map.neighbors(floor)
            .into_iter()
            .min_by_key(|(n, _)| n.flat_distance(&knockback.destination));

        let Some((next_qrz, _)) = best else {
            commands.entity(ent).remove::<Knockback>();
            continue;
        };

        // No progress check
        if next_qrz.flat_distance(&knockback.destination) >= floor.flat_distance(&knockback.destination) {
            commands.entity(ent).remove::<Knockback>();
            continue;
        }

        // Move to next tile (Loc is standing height: floor + Z)
        let new_loc = Loc::new(next_qrz + Qrz::Z);
        *loc = new_loc;

        writer.write(Do {
            event: GameEvent::Incremental {
                ent,
                component: common::message::Component::Loc(new_loc),
            },
        });

        knockback.remaining_tiles -= 1;
        if knockback.remaining_tiles <= 0 {
            commands.entity(ent).remove::<Knockback>();
        }
    }
}

/// Freeze staggered entities by resetting Position.offset to zero.
/// Runs in FixedUpdate AFTER behavior systems (chase, kite) so it overrides
/// any movement they computed. Universal — no per-behavior code needed.
pub fn enforce_stagger(
    mut query: Query<&mut Position, With<Stagger>>,
) {
    for mut pos in &mut query {
        pos.offset = Vec3::ZERO;
    }
}
