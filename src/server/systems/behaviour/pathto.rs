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
    q_target: Query<(&Loc, &Target)>,
    q_other: Query<&Loc>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (mut path_to, &ctx) in &mut query {
        let Ok((&loc, &target)) = q_target.get(ctx.target_entity()) else { continue };
        let Ok(&o_loc) = q_other.get(*target) else { continue };

        let Some((dest,_)) = map.find(*o_loc,-60) else { continue };
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

        // push the first 20 steps of the path not including current location
        path_to.path.clear();
        for &it in full_path.iter().skip(1).take(20).rev() { path_to.path.push(it); }
    }
}

pub fn apply(
    mut commands: Commands,
    mut query: Query<(&mut PathTo, &BehaveCtx)>,
    mut q_target: Query<(&Loc, &mut Heading, &mut Offset, &mut AirTime, &Target)>,
    dt: Res<Time>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (mut path_to, &ctx) in &mut query {
        let Ok((&loc, mut heading0, mut offset0, mut airtime0, &_target)) = q_target.get_mut(ctx.target_entity()) else { unreachable!() };
        if path_to.path.is_empty() { commands.trigger(ctx.success()) }

        let Some(&qrz) = path_to.path.last() else { continue; };
        let here = *loc - Qrz::Z;
        if here == qrz { path_to.path.pop(); }

        let Some(&dest) = path_to.path.last() else { continue };
        let heading = Heading::from(KeyBits::from(Heading::new(dest - here)));
        if heading != *heading0 { *heading0 = heading; }
        if loc.z <= dest.z && airtime0.state.is_none() { airtime0.state = Some(125); }
        let (offset, airtime) = physics::apply(Loc::new(dest), dt.delta().as_millis() as i16, loc, offset0.state, airtime0.state, &map, &nntree);
        (offset0.state, airtime0.state) = (offset,airtime);
    }
}
