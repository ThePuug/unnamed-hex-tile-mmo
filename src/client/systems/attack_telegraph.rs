use bevy::prelude::*;
use qrz::Convert;
use crate::common::{
    components::{Loc, offset::Offset},
    message::{Do, Event as GameEvent},
    resources::map::Map,
};

/// Component for attack telegraph visual (ball over attacker's head)
#[derive(Component)]
pub struct AttackBall {
    /// Entity that is attacking
    pub source: Entity,
    /// Entity that is being attacked
    pub target: Entity,
}

/// Component for hit line visual (line connecting attacker to target)
#[derive(Component)]
pub struct HitLine {
    /// Entity that is attacking
    pub source: Entity,
    /// Entity that is being attacked
    pub target: Entity,
    /// Time when the line was spawned
    pub spawn_time: f32,
    /// Duration of the line effect (milliseconds)
    pub duration_ms: u64,
}

/// Spawn attack ball when a threat is inserted into the queue (Volley ability only)
pub fn on_insert_threat(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for message in reader.read() {
        let Do { event: GameEvent::InsertThreat { ent: target, threat } } = message else { continue };

        // Only show ball for Volley ability
        if threat.ability != Some(crate::common::message::AbilityType::Volley) {
            continue; // Skip all non-Volley threats
        }

        // Spawn attack ball as child of source entity
        // Local offset of 1.5 units above entity (0, 1.5, 0)
        // Bevy's transform hierarchy will automatically track parent position
        let ball_id = commands.spawn((
            AttackBall {
                source: threat.source,
                target: *target,
            },
            Mesh3d(meshes.add(Sphere::new(0.2))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.8, 0.0), // Yellow
                emissive: LinearRgba::rgb(8.0, 6.0, 0.0), // Bright yellow glow
                ..default()
            })),
            Transform::from_translation(Vec3::new(0.0, 1.5, 0.0)), // Local offset above parent
            Visibility::default(),
        )).id();

        // Parent ball to source entity for automatic position tracking
        commands.entity(threat.source).add_child(ball_id);
    }
}

/// Replace attack ball with hit line when damage is applied (Volley ability only)
pub fn on_apply_damage(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    map: Res<Map>,
    locs: Query<&Loc>,
    offsets: Query<&Offset>,
    balls: Query<(Entity, &AttackBall)>,
) {
    for message in reader.read() {
        let Do { event: GameEvent::ApplyDamage { ent: target, source, .. } } = message else { continue };

        // Find and despawn the attack ball for this source->target pair
        // Note: Ball only exists for ranged attacks, so line will only spawn for ranged too
        for (ball_entity, ball) in balls.iter() {
            if ball.source == *source && ball.target == *target {
                // Despawn the ball
                commands.entity(ball_entity).despawn();

                // Get positions for line (connects from player to ball position)
                let Ok(source_loc) = locs.get(*source) else { continue };
                let Ok(target_loc) = locs.get(*target) else { continue };

                let source_offset = offsets.get(*source).map(|o| o.step).unwrap_or(Vec3::ZERO);
                let target_offset = offsets.get(*target).map(|o| o.step).unwrap_or(Vec3::ZERO);

                // Source is the attacker (ball position), target is the player
                let source_world = map.convert(**source_loc) + source_offset + Vec3::new(0.0, 1.5, 0.0); // Ball height
                let target_world = map.convert(**target_loc) + target_offset + Vec3::new(0.0, 0.5, 0.0); // Player center

                // Spawn hit line
                let direction = target_world - source_world;
                let distance = direction.length();
                let midpoint = source_world + direction * 0.5;
                let line_mesh = Cuboid::new(0.05, 0.05, distance);
                let rotation = Quat::from_rotation_arc(Vec3::Z, direction.normalize());

                commands.spawn((
                    HitLine {
                        source: *source,
                        target: *target,
                        spawn_time: time.elapsed_secs(),
                        duration_ms: 300,
                    },
                    Mesh3d(meshes.add(line_mesh)),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(1.0, 1.0, 1.0),
                        emissive: LinearRgba::rgb(15.0, 15.0, 15.0),
                        ..default()
                    })),
                    Transform::from_translation(midpoint).with_rotation(rotation),
                    Visibility::default(),
                ));

                break;
            }
        }
    }
}

/// Despawn attack ball when threat is cleared (deflect/knockback)
pub fn on_clear_queue(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    balls: Query<(Entity, &AttackBall)>,
) {
    for message in reader.read() {
        let Do { event: GameEvent::ClearQueue { ent: target, .. } } = message else { continue };

        // Despawn all balls targeting this entity
        for (ball_entity, ball) in balls.iter() {
            if ball.target == *target {
                commands.entity(ball_entity).despawn();
            }
        }
    }
}

/// Update and cleanup expired attack telegraphs
pub fn update_telegraphs(
    mut commands: Commands,
    time: Res<Time>,
    lines: Query<(Entity, &HitLine, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let current_time = time.elapsed_secs();

    // Note: Balls are parented to source entities, so they automatically track position
    // Balls are NOT cleaned up by time - they're cleaned up by server events:
    // - ApplyDamage: Replace ball with line when damage lands
    // - ClearQueue: Remove ball when threat is cleared (deflect/knockback)

    // Update and cleanup hit lines
    for (line_entity, line, material_handle) in lines.iter() {
        let age_ms = ((current_time - line.spawn_time) * 1000.0) as u64;

        if age_ms >= line.duration_ms {
            commands.entity(line_entity).despawn();
        } else {
            // Fade out (line position stays static where it spawned)
            let progress = age_ms as f32 / line.duration_ms as f32;
            let alpha = 1.0 - progress;

            if let Some(material) = materials.get_mut(&material_handle.0) {
                material.emissive = LinearRgba::rgb(15.0 * alpha, 15.0 * alpha, 15.0 * alpha);
            }
        }
    }
}
