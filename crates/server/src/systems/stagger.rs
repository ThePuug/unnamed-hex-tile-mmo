use bevy::prelude::*;
use common_bevy::components::{behaviour::PlayerControlled, position::Position, stagger::Stagger, stunned::Stunned};

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

/// Freeze staggered and stunned NPCs by resetting Position.offset to zero.
/// Runs in FixedUpdate AFTER the chase behaviour so it overrides
/// any movement they computed. Universal — no per-behavior code needed.
/// A player's stun holds in `input::apply`, where its movement is made.
pub fn enforce_stagger(
    mut query: Query<(&mut Position, Has<Stagger>, Option<&Stunned>), Without<PlayerControlled>>,
) {
    for (mut pos, staggered, stunned) in &mut query {
        if staggered || Stunned::holds(stunned) {
            pos.offset = Vec3::ZERO;
        }
    }
}
