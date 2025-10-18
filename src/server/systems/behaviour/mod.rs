pub mod pathto;

use bevy::prelude::*;
use bevy_behave::prelude::*;
use rand::seq::IteratorRandom;

use crate::common::{
    components::{entity_type::EntityType, *}, 
    plugins::nntree::*
};

#[derive(Clone, Component, Copy, Deref, DerefMut)]
pub struct Target(Entity);

impl Target {
    pub fn new(ent: Entity) -> Self { Self(ent) }
}

#[derive(Clone, Component, Copy, Default)]
pub struct FindSomethingInterestingWithin {
    pub dist: u16,
}

pub fn find_something_interesting_within(
    mut commands: Commands,
    mut query: Query<(&FindSomethingInterestingWithin, &BehaveCtx)>,
    q_target: Query<(&Loc, &NearestNeighbor)>,
    q_other: Query<(Entity, &EntityType, &NearestNeighbor)>,
    nntree: Res<NNTree>,
) {
    for (&behaviour, &ctx) in &mut query {
        let Ok((&loc, &nn)) = q_target.get(ctx.target_entity()) else { continue };
        let dist = behaviour.dist as i16;
        let others = nntree.locate_within_distance(loc, dist*dist).map(
            |it| q_other.get(it.ent).expect("missing other entity")
        );
        let Some((o_ent,_,_)) = others.filter(|it| {
            let &(_,_,&o_nn) = it;
            o_nn != nn
        }).choose(&mut rand::rng()) else { continue };
        commands.entity(ctx.target_entity()).insert(Target::new(o_ent));
        commands.trigger(ctx.success());
    }
}