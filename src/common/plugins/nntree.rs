// common/plugins/nntree.rs:
// This plugin implements a KdTree for mapping Hx to Entity.
// The KdTree is used for finding nearest neighbors in the physics system.
// TODO:
// - Generalise marker component
// - Generalise location component to anything that implements Into<Axis>

use bevy::{
    ecs::{
        component::ComponentId, 
        world::DeferredWorld
    }, 
    prelude::*
};
use fixed::{types::extra::U0, FixedI16};
use kiddo::{
    fixed::kdtree::{Axis, KdTree}, 
    traits::DistanceMetric
};

use crate::common::components::hx::Hx;

#[derive(Component, Default)]
#[component(on_add = on_add)]
#[component(on_remove = on_remove)]
pub struct NearestNeighbor(pub Hx);

pub fn on_add(mut world: DeferredWorld, ent: Entity, _: ComponentId) {
    let &hx = world.get::<Hx>(ent).unwrap();
    world.get_mut::<NearestNeighbor>(ent).unwrap().0 = hx;
    world.resource_mut::<NNTree>().0.add(&hx.into(), ent.to_bits());
}

pub fn on_remove(mut world: DeferredWorld, ent: Entity, _: ComponentId) {
    let hx = world.get::<NearestNeighbor>(ent).unwrap().0;
    world.resource_mut::<NNTree>().0.remove(&hx.into(), ent.to_bits());
}

#[derive(Resource)]
pub struct NNTree(pub KdTree<FixedI16<U0>, u64, 4, 8, u32>);

pub fn update(
    mut query: Query<(Entity, &Hx, &mut NearestNeighbor), Changed<Hx>>,
    mut nntree: ResMut<NNTree>,
) {
    for (ent, &hx, mut nn) in &mut query {
        nntree.0.remove(&nn.0.into(), ent.to_bits());
        nn.0 = hx;
        nntree.0.add(&hx.into(), ent.to_bits());
    }
}

pub struct Hexhattan {}

impl<A: Axis, const K: usize> DistanceMetric<A, K> for Hexhattan {
    #[inline]
    fn dist(a: &[A; K], b: &[A; K]) -> A {
        let mut iter = a.iter()
            .zip(b.iter())
            .map(|(&a_val, &b_val)| {
                if a_val > b_val { a_val - b_val } 
                else { b_val - a_val }
            });
        let max = iter.by_ref().take(3).fold(A::ZERO, |a, b| a.max(b));
        iter.fold(max, |a, b| a + b)
    }

    #[inline]
    fn dist1(a: A, b: A) -> A {
        let diff: A = a.dist(b);
        diff * diff
    }
}
