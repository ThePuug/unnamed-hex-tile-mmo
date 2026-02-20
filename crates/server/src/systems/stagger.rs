use bevy::prelude::*;
use qrz::Qrz;
use common::{
    components::{position::Position, stagger::Stagger, Loc},
    message::{Do, Event as GameEvent},
    resources::map::Map,
};

/// Server-only: entity being pushed tile-by-tile over multiple ticks.
/// Each FixedUpdate tick (125ms) moves the entity 1 tile in the knockback direction.
/// Stops early on cliffs or map edges.
#[derive(Component, Clone, Copy, Debug)]
pub struct Knockback {
    pub direction: Qrz,
    pub remaining_tiles: i16,
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

/// Process knockback: move entity 1 tile per tick in knockback direction.
/// Broadcasts Loc incremental for each step so clients see smooth 1-hex crossings.
/// Stops on cliff (elevation diff > 1) or missing floor.
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

        // Try to move 1 tile in knockback direction
        let next_flat = Qrz {
            q: loc.q + knockback.direction.q,
            r: loc.r + knockback.direction.r,
            z: 0,
        };
        let next_floor = map.find(next_flat + Qrz { q: 0, r: 0, z: loc.z + 30 }, -60);

        let Some((next_qrz, _)) = next_floor else {
            commands.entity(ent).remove::<Knockback>();
            continue;
        };

        let elevation_diff = (next_qrz.z - loc.z).abs();
        if elevation_diff > 1 {
            commands.entity(ent).remove::<Knockback>();
            continue;
        }

        // Move to next tile
        let new_loc = Loc::new(next_qrz);
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
