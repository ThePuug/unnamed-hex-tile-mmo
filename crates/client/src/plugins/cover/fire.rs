//! The fires the cover burns: a light over every model a region of tiles
//! stands whose faces glow, as its materials declare (`Variation::glow`),
//! the region's child in its frame, so it comes, re-bases and goes with
//! the model drawn under it. The cel light reads it as it reads the sun
//! (`shaders/cel.wesl`), so the ground, the cover and the actors round a
//! fire are lit by one rule. A fire casts no shadow.

use bevy::prelude::*;

use super::draw::{RegionCover, StandKey};
use super::CoverKit;

/// A fire's colour, and its light at its steadiest, in lumens: by night it
/// carries a few tiles, and by day the sun drowns it.
const COLOR: Color = Color::srgb(1.0, 0.62, 0.28);
const LUMENS: f32 = 20_000.0;
/// How far a fire's light reaches before it is none.
const REACH: f32 = 16.0;
/// How far over its flames' tips a fire's light stands, so the ground
/// about it faces the light.
const OVER: f32 = 0.08;
/// The waves a fire wavers by, each its share of the light and its turns
/// a second: none a multiple of another, so the waver does not repeat in
/// any time a player watches. `shaders/models.wesl` draws the flame by the
/// same waves from the same phase, so it stands tall as this burns bright:
/// change both.
const WAVER: [(f32, f32); 3] = [(0.10, 1.7), (0.07, 4.3), (0.04, 9.1)];

/// A fire's light, wavering from its own place in the waves.
#[derive(Component)]
pub struct Fire {
    phase: f32,
}

/// How bright a fire at `phase` burns at `seconds`, about one.
fn waver(seconds: f32, phase: f32) -> f32 {
    1.0 + WAVER
        .iter()
        .enumerate()
        .map(|(k, (share, turns))| share * (std::f32::consts::TAU * turns * seconds + phase * (k + 1) as f32).sin())
        .sum::<f32>()
}

/// Lights a fire over every glowing model a region of tiles has come to
/// stand, in place of those it had, and puts out those of a region
/// standing none. A summary level's models take none: the tiles' region
/// under them lights the same fire.
pub fn kindle(
    mut commands: Commands,
    kit: Res<CoverKit>,
    regions: Query<(Entity, &RegionCover), Changed<RegionCover>>,
    mut gone: RemovedComponents<RegionCover>,
    fires: Query<(Entity, &ChildOf), With<Fire>>,
) {
    let changed: Vec<Entity> = regions.iter().map(|(region, _)| region).chain(gone.read()).collect();
    if changed.is_empty() {
        return;
    }
    for (fire, of) in &fires {
        if changed.contains(&of.parent()) {
            commands.entity(fire).try_despawn();
        }
    }
    for (region, cover) in &regions {
        for part in &cover.parts {
            let StandKey::Model(kind, k) = part.key else { continue };
            let Some(glow) = kit.kit.of(kind).get(k).and_then(|v| v.glow) else { continue };
            for instance in part.instances.iter().filter(|i| i.turn[2] == 0.0) {
                let [x, y, z, scale] = instance.pos_scale;
                let [cos, sin, ..] = instance.turn;
                // The instance's own placing, as `shaders/models.wesl` does it.
                let tip = (glow + Vec3::Y * OVER) * scale;
                let turned = Vec3::new(tip.x * cos - tip.z * sin, tip.y, tip.x * sin + tip.z * cos);
                let at = Quat::from_array(instance.tilt) * turned + Vec3::new(x, y, z);
                commands.entity(region).with_child((
                    Fire { phase: (x * 0.137 + z * 0.731).rem_euclid(1.0) * std::f32::consts::TAU },
                    PointLight { color: COLOR, intensity: LUMENS, range: REACH, ..default() },
                    Transform::from_translation(at),
                ));
            }
        }
    }
}

/// Wavers every fire's light. The clock is the wrapped one, which the
/// shaders' own is: a float of the whole uptime would lose the waver's
/// fraction of a second in a day.
pub fn flicker(time: Res<Time>, mut fires: Query<(&Fire, &mut PointLight)>) {
    let seconds = time.elapsed_secs_wrapped();
    for (fire, mut light) in &mut fires {
        light.intensity = LUMENS * waver(seconds, fire.phase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fire never goes out or doubles, and two fires do not waver as one.
    #[test]
    fn a_fire_wavers_about_its_light() {
        let swing: f32 = WAVER.iter().map(|(share, _)| share).sum();
        assert!(swing < 0.5);
        let mut apart = false;
        for i in 0..600 {
            let t = i as f32 / 60.0;
            let (a, b) = (waver(t, 0.3), waver(t, 2.1));
            assert!((a - 1.0).abs() <= swing + 1e-5);
            apart |= (a - b).abs() > 0.05;
        }
        assert!(apart);
    }
}
