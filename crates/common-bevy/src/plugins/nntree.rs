// common/plugins/nntree.rs:
// NNTree plugins implements Nearest neighbor lookups
// - adds a NNTree Resource for querying nearest neighbors given a location
// - updates the NNTree as Entities change their locations
// - provides a manhattan distance algorithm ("Hexhattan") for hexagonal grids using axial coordinates in first 2 dimensions and z in the 3rd

use bevy::{
    ecs::{
        lifecycle::HookContext,
        world::DeferredWorld
    },
    prelude::*
};
use rstar::{DefaultParams, Point, PointDistance, RTree, RTreeObject, AABB};

use crate::components::*;
pub struct NNTreePlugin;

impl Plugin for NNTreePlugin {
    fn build(&self, app: &mut App) {
        let nntree = NNTree(RTree::new());
        app.insert_resource(nntree)
            .add_systems(Update, update)
        ;
    }
}

#[derive(Clone, Component, Copy, Eq, PartialEq)]
#[component(on_add = on_add)]
#[component(on_remove = on_remove)]
pub struct NearestNeighbor {
    pub ent: Entity,
    pub loc: Loc,
}

impl NearestNeighbor {
    pub fn new(ent: Entity, loc: Loc) -> Self {
        NearestNeighbor { ent, loc }
    }
}

fn on_add(mut world: DeferredWorld, context: HookContext) {
    let Some(&nn) = world.get::<NearestNeighbor>(context.entity) else { unreachable!() };
    world.resource_mut::<NNTree>().insert(nn);
}

fn on_remove(mut world: DeferredWorld, context: HookContext) {
    let Some(&nn) = world.get::<NearestNeighbor>(context.entity) else { unreachable!() };
    world.resource_mut::<NNTree>().remove(&nn);
}

#[derive(Deref, DerefMut, Resource)]
pub struct NNTree(RTree<NearestNeighbor, DefaultParams>);

impl NNTree {
    /// Create an empty NNTree for testing purposes
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        NNTree(RTree::new())
    }
}

pub fn update(
    mut query: Query<(&Loc, &mut NearestNeighbor), Changed<Loc>>,
    mut nntree: ResMut<NNTree>,
) {
    for (&loc, mut nn) in &mut query {
        nntree.remove(&nn);
        nn.loc = loc;
        nntree.insert(*nn);
    }
}

impl RTreeObject for NearestNeighbor {
    type Envelope = AABB<Loc>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.loc)
    }
}

impl PointDistance for NearestNeighbor {
    fn distance_2(
        &self,
        point: &<Self::Envelope as rstar::Envelope>::Point,
    ) -> <<Self::Envelope as rstar::Envelope>::Point as rstar::Point>::Scalar {
        let sq = self.loc.q as i64;
        let sr = self.loc.r as i64;
        let sz = self.loc.z as i64;
        let pq = point.q as i64;
        let pr = point.r as i64;
        let pz = point.z as i64;
        let self_s = -sq - sr;
        let point_s = -pq - pr;
        let dist = [
            (sq - pq).abs(),
            (sr - pr).abs(),
            (self_s - point_s).abs()
        ].into_iter().max().unwrap() + (sz - pz).abs();
        dist * dist
    }
}

impl Point for Loc {
    // i64 prevents overflow in rstar's internal AABB area/distance calculations
    // (multiplies dimension spans together — overflows i32 when entities span large maps)
    type Scalar = i64;
    const DIMENSIONS: usize = 3;

    fn generate(mut generator: impl FnMut(usize) -> Self::Scalar) -> Self {
        Loc::from_qrz(generator(0) as i32, generator(1) as i32, generator(2) as i32)
    }

    fn nth(&self, index: usize) -> Self::Scalar {
        match index {
            0 => self.q as i64,
            1 => self.r as i64,
            2 => self.z as i64,
            _ => unreachable!(),
        }
    }

    fn nth_mut(&mut self, _index: usize) -> &mut Self::Scalar {
        unimplemented!("nth_mut not supported for Loc - rstar doesn't need it for queries")
    }
}
