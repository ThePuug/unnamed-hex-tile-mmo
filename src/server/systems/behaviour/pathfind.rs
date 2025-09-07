use bevy::{
    prelude::*, 
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool}
};
use pathfinding::prelude::*;
use qrz::Qrz;
use rand::seq::IteratorRandom;
use tinyvec::ArrayVec;

use crate::{
    common::{
        components::{ behaviour::*, entity_type::*, heading::Heading, keybits::KeyBits, offset::*, * },
        message::{Component, Event, * },
        plugins::nntree::*, 
        resources::map::*, 
        systems::physics,
    }, 
    server::resources::AsyncTasks
};

pub fn tick(
    query: Query<(Entity, &Loc, &NearestNeighbor, &Behaviour)>,
    q_other: Query<(&Loc, &EntityType)>,
    map: Res<Map>,
    nntree: Res<NNTree>,
    mut tasks: ResMut<AsyncTasks>,
) {
    if !tasks.task_behaviour_pathfind.is_none() { return }

    let mut paths = Vec::new();
    for (ent, &loc, &nn, &behaviour) in query.iter() {
        let Behaviour::Pathfind(Pathfind { .. }) = behaviour else { continue };

        let others = nntree.locate_within_distance(loc, 20*20);
        let Some(other) = others.filter(|it| **it != nn).choose(&mut rand::rng()) else { continue };
        let Ok((&o_loc, &o_typ)) = q_other.get(other.ent) else { continue };
        let EntityType::Actor(_) = o_typ else { continue };
        
        paths.extend_one((ent,*loc,*o_loc));
    }

    let pool = AsyncComputeTaskPool::get();
    let map = map.clone();
    tasks.task_behaviour_pathfind = Some(pool.spawn(async move {
        async_tick(&paths, &map)
    }));
}

pub fn async_ready(
    mut writer: EventWriter<Do>,
    mut tasks: ResMut<AsyncTasks>,
) {
    if tasks.task_behaviour_pathfind.is_none() { return; }

    let task = tasks.task_behaviour_pathfind.as_mut();
    let result = block_on(future::poll_once(task.unwrap()));
    if result.is_none() { return; }

    let events = result.unwrap();
    for &event in events.iter() { writer.write(event); }
    tasks.task_behaviour_pathfind = None;
}

fn async_tick(
    paths: &Vec<(Entity,Qrz,Qrz)>,
    map: &Map,
) -> Vec<Do> {
    let mut ret = Vec::new();
    for &(ent,start,dest) in paths.iter() {
        let Some((dest,_)) = map.find(dest,-2) else { continue };
        let Some((start,_)) = map.find(start,-2) else { continue };

        // if dest == dest0 { continue; }

        let (full_path, _) = astar(
                &start,
                |&l| map.neighbors(l).into_iter().map(|it| (it.0, match it.1 {
                        EntityType::Decorator(_) => 1_i16,
                        _ => unreachable!()
                })),
                |&l| l.distance(&dest), 
                |&l| l == dest
            ).unwrap_or_default();

        let mut pathfind = Pathfind { dest, path: ArrayVec::new() };
        // push the first 20 steps of the path not including current location
        for &it in full_path.iter().skip(1).take(20).rev() { pathfind.path.push(it); }
        ret.extend_one(Do { event: Event::Incremental { ent, component: Component::Behaviour(Behaviour::Pathfind(pathfind)) }});
    }
    ret
}

pub fn apply(
    mut writer: EventWriter<Do>,
    mut query: Query<(Entity, &mut Behaviour, &Loc, &mut Heading, &mut Offset, &mut AirTime)>,
    dt: Res<Time>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (ent, mut behaviour, &loc, mut heading0, mut offset0, mut airtime0) in &mut query {
        let Behaviour::Pathfind(mut pathfind) = *behaviour else { continue; };
        if pathfind.path.is_empty() { continue; }
        let &qrz = pathfind.path.last().expect("no last in path");
        let here = *loc - Qrz::Z;
        if here == qrz { 
            pathfind.path.pop(); 
            *behaviour = Behaviour::Pathfind(pathfind);
        }

        let Some(&dest) = pathfind.path.last() else { continue };
        let heading = Heading::from(KeyBits::from(Heading::new(dest - here)));
        if heading != *heading0 {
            *heading0 = heading;
            writer.write(Do { event: Event::Incremental { ent, component: Component::Heading(heading) }});
        }
        if loc.z <= dest.z && airtime0.state.is_none() { airtime0.state = Some(125); }
        let (offset, airtime) = physics::apply(Loc::new(dest), dt.delta().as_millis() as i16, loc, offset0.state, airtime0.state, &map, &nntree);
        (offset0.state, airtime0.state) = (offset,airtime);
    }
}
