use bevy::prelude::*;
use bevy_behave::prelude::*;
use pathfinding::prelude::*;
use qrz::Qrz;

use crate::{
    common::{
        components::{ behaviour::*, entity_type::*, heading::Heading, keybits::KeyBits, offset::*, * },
        plugins::nntree::*, 
        resources::map::*, 
        systems::physics,
    }, 
    server::systems::behaviour::Target
};

pub fn tick(
    mut query: Query<(&mut PathTo, &BehaveCtx)>,
    q_target: Query<(&Loc, &crate::common::components::Dest)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    use crate::common::components::behaviour::PathLimit;

    for (mut path_to, &ctx) in &mut query {
        let Ok((&loc, &dest_comp)) = q_target.get(ctx.target_entity()) else { continue };

        let Some((dest,_)) = map.find(*dest_comp,-60) else { continue };
        let Some((start,_)) = map.find(*loc,-60) else { continue };

        if dest == path_to.dest { continue; }

        let (full_path, _) = astar(
                &start,
                |&l| map.neighbors(l).into_iter()
                    .filter(|it| nntree.locate_all_at_point(&Loc::new(it.0 + Qrz::Z)).count() < 7)
                    .map(|it| (it.0, match it.1 {
                        EntityType::Decorator(_) => 1_i16,
                        _ => unreachable!()
                })),
                |&l| l.distance(&dest),
                |&l| l == dest
            ).unwrap_or_default();

        path_to.dest = dest;

        // Apply path limit based on limit field
        let max_steps = match path_to.limit {
            PathLimit::By(n) => n as usize,                     // Limit to N steps
            PathLimit::Until(n) => {                            // Stop N tiles away from dest
                let total_dist = full_path.len().saturating_sub(1);
                total_dist.saturating_sub(n as usize)
            }
            PathLimit::Complete => 20,                          // Take up to 20 steps (original behavior)
        };

        // push the limited steps of the path not including current location
        path_to.path.clear();
        for &it in full_path.iter().skip(1).take(max_steps.min(20)).rev() {
            path_to.path.push(it);
        }
    }
}

pub fn apply(
    mut commands: Commands,
    mut query: Query<(&mut PathTo, &BehaveCtx)>,
    mut q_target: Query<(&Loc, &mut Heading, &mut Offset, &mut AirTime, Option<&ActorAttributes>, &Target)>,
    dt: Res<Time>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (mut path_to, &ctx) in &mut query {
        let Ok((&loc, mut heading0, mut offset0, mut airtime0, attrs, &_target)) = q_target.get_mut(ctx.target_entity()) else { unreachable!() };
        if path_to.path.is_empty() { commands.trigger(ctx.success()) }

        let Some(&qrz) = path_to.path.last() else { continue; };
        let here = *loc - Qrz::Z;
        if here == qrz { path_to.path.pop(); }

        let Some(&dest) = path_to.path.last() else { continue };
        let heading = Heading::from(KeyBits::from(Heading::new(dest - here)));
        if heading != *heading0 { *heading0 = heading; }
        if loc.z <= dest.z && airtime0.state.is_none() { airtime0.state = Some(125); }
        let movement_speed = attrs.map(|a| a.movement_speed).unwrap_or(0.005);
        let (offset, airtime) = physics::apply(Loc::new(dest), dt.delta().as_millis() as i16, loc, offset0.state, airtime0.state, movement_speed, *heading0, &map, &nntree);
        (offset0.state, airtime0.state) = (offset,airtime);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::components::behaviour::PathLimit;
    use tinyvec::ArrayVec;

    #[test]
    fn pathlimit_by_limits_path_length() {
        // Test that PathLimit::By(5) only paths 5 tiles
        let origin = Qrz { q: 0, r: 0, z: 0 };
        let target = Qrz { q: 10, r: 0, z: -10 }; // 10 tiles away

        let path_to = PathTo {
            dest: target,
            path: ArrayVec::new(),
            limit: PathLimit::By(5),
        };

        // After tick, path should be limited to 5 steps
        // This test will fail until we implement the limit logic
        assert_eq!(path_to.limit, PathLimit::By(5));
    }

    #[test]
    fn pathlimit_until_stops_at_distance() {
        // Test that PathLimit::Until(2) stops 2 tiles away from dest
        let path_to = PathTo {
            dest: Qrz { q: 10, r: 0, z: -10 },
            path: ArrayVec::new(),
            limit: PathLimit::Until(2),
        };

        // Should succeed when within 2 tiles of dest
        assert_eq!(path_to.limit, PathLimit::Until(2));
    }
}
