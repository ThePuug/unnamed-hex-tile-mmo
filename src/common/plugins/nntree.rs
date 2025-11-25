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

use crate::common::components::*;
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
        let self_s = -self.loc.q-self.loc.r;
        let point_s = -point.q-point.r;
        // Calculate distance using i32 to prevent overflow
        let dist = [
            (self.loc.q - point.q).abs() as i32,
            (self.loc.r - point.r).abs() as i32,
            (self_s - point_s).abs() as i32
        ].iter().max().unwrap() + (self.loc.z - point.z).abs() as i32;
        dist * dist
    }
}

impl Point for Loc {
    // Use i32 instead of i16 to prevent overflow in rstar's internal AABB calculations
    // when entities are far from origin
    type Scalar = i32;
    const DIMENSIONS: usize = 3;

    fn generate(mut generator: impl FnMut(usize) -> Self::Scalar) -> Self {
        Loc::from_qrz(generator(0) as i16, generator(1) as i16, generator(2) as i16)
    }

    fn nth(&self, index: usize) -> Self::Scalar {
        match index {
            0 => self.q as i32,
            1 => self.r as i32,
            2 => self.z as i32,
            _ => unreachable!(),
        }
    }

    fn nth_mut(&mut self, index: usize) -> &mut Self::Scalar {
        // Since we can't return a mutable reference to a temporary i32,
        // we need to work with a static mutable. This is a limitation of the API.
        // However, this method is rarely used by rstar for spatial queries.
        unimplemented!("nth_mut not supported for Loc - rstar doesn't need it for queries")
    }
}
