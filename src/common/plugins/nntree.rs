// common/plugins/nntree.rs:
// NNTree plugins implements Nearest neighbor lookups via an underlying KdTree
// - adds a NNTree Resource for querying nearest neighbors given a location
// - updates the KdTree as Entities change their locations
// - provides a manhattan distance algorithm ("Hexhattan") for hexagonal grids using cube coordinates in first 3 dimensions
// TODO:
// - Generalise marker component to anything that implements Into<Axis>
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

pub struct NNTreePlugin;

impl Plugin for NNTreePlugin {
    fn build(&self, app: &mut App) {
        let kdtree = NNTree(KdTree::with_capacity(1_000_000));
        app.insert_resource(kdtree)
            .add_systems(Update, update);
    }
}

#[derive(Component, Default, Deref, DerefMut)]
#[component(on_add = on_add)]
#[component(on_remove = on_remove)]
pub struct NearestNeighbor(Hx);

pub fn on_add(mut world: DeferredWorld, ent: Entity, _: ComponentId) {
    let hx = *world.get::<Hx>(ent).unwrap();
    **world.get_mut::<NearestNeighbor>(ent).unwrap() = hx;
    world.resource_mut::<NNTree>().add(&hx.into(), ent.to_bits());
}

pub fn on_remove(mut world: DeferredWorld, ent: Entity, _: ComponentId) {
    let hx = **world.get::<NearestNeighbor>(ent).unwrap();
    world.resource_mut::<NNTree>().remove(&hx.into(), ent.to_bits());
}

#[derive(Deref, DerefMut, Resource)]
pub struct NNTree(KdTree<FixedI16<U0>, u64, 4, 8, u32>);

pub fn update(
    mut query: Query<(Entity, &Hx, &mut NearestNeighbor), Changed<Hx>>,
    mut nntree: ResMut<NNTree>,
) {
    for (ent, &hx, mut nn) in &mut query {
        nntree.remove(&(**nn).into(), ent.to_bits());
        **nn = hx;
        nntree.add(&hx.into(), ent.to_bits());
    }
}

// TODO: current distance functions weirdly - need to rework dist to handle
// - players being 1 tile above the ground
// - within(value) (et.al.) calls return distances less than value, not equal to
#[allow(dead_code)]
pub struct Hexhattan();
impl<A: Axis, const K: usize> DistanceMetric<A, K> for Hexhattan {
    #[inline]
    fn dist(a: &[A; K], b: &[A; K]) -> A {
        let mut iter = a.iter()
            .zip(b.iter())
            .map(|(&a_val, &b_val)| {
                if a_val > b_val { a_val - b_val } 
                else { b_val - a_val }
            });
        let max = iter.by_ref().take(3).fold(A::ZERO, |a, b| a.max(b-A::TRY_ONE.unwrap()).max(A::ZERO));
        iter.fold(max, |a, b| a.saturating_add(b-A::TRY_ONE.unwrap()).max(a))
    }

    #[inline]
    fn dist1(a: A, b: A) -> A {
        a.dist(b)
    }
}
