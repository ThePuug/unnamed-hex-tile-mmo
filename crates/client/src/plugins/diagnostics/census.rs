//! What each kind of thing in the scene costs the rasteriser, counted in
//! the main world: how many draws it is, how many copies those draws
//! stand for, and how many triangles reach the clipper.
//!
//! A pass timing says the frame is slow; this says which group made it
//! so. The forest draws instanced, so its draw count and its triangle
//! count move independently — one batch of a thousand trees is one draw
//! and a thousand crowns — and terrain is the reverse, a draw per region
//! and the triangles of the tiles in it. Only what a camera can see is
//! counted, so the numbers move as the view does.

use bevy::camera::visibility::ViewVisibility;
use bevy::prelude::*;
use std::time::Duration;

use crate::plugins::forest::draw::{Batch, CardBatch, TreeBatch};
use crate::resources::SummaryMesh;
use common_bevy::components::Actor;

/// How often the census is retaken. Walking every visible mesh costs
/// real time, and the numbers are read by eye.
const EVERY: Duration = Duration::from_millis(500);

/// One group's share of the scene.
#[derive(Default, Clone, Copy)]
pub struct Group {
    /// Draws: one per entity the rasteriser is handed.
    pub draws: u32,
    /// Copies those draws stand for. Equal to `draws` unless the group
    /// draws instanced, as the forest does.
    pub instances: u32,
    /// Triangles reaching the clipper, instances counted.
    pub triangles: u64,
}

impl Group {
    fn add(&mut self, mesh_triangles: u64, instances: u32) {
        self.draws += 1;
        self.instances += instances;
        self.triangles += mesh_triangles * instances as u64;
    }
}

/// The scene split by what the group is, as of the last census.
#[derive(Resource, Default)]
pub struct RenderCensus {
    pub terrain: Group,
    pub forest: Group,
    pub actors: Group,
    /// Everything else drawn: the sky dome, the water plane, the sun and
    /// moon discs, the grid overlay.
    pub other: Group,
}

impl RenderCensus {
    pub fn total_triangles(&self) -> u64 {
        self.terrain.triangles + self.forest.triangles + self.actors.triangles + self.other.triangles
    }
}

/// A mesh's triangles, indexed or not. A mesh whose asset has not
/// arrived counts nothing rather than guessing.
fn triangles_of(mesh: &Mesh) -> u64 {
    match mesh.indices() {
        Some(indices) => indices.len() as u64 / 3,
        None => mesh.count_vertices() as u64 / 3,
    }
}

/// Retake the census over every mesh a camera can see, grouped by what
/// the entity is: its own marker, or the nearest ancestor's for the
/// parts an actor is built from.
#[allow(clippy::type_complexity)]
pub fn take_census(
    mut census: ResMut<RenderCensus>,
    state: Res<super::DiagnosticsState>,
    dump: Res<super::dump::MetricsDump>,
    time: Res<Time>,
    mut due: Local<Duration>,
    meshes: Res<Assets<Mesh>>,
    drawn: Query<(
        Entity,
        &Mesh3d,
        &ViewVisibility,
        Option<&SummaryMesh>,
        Option<&TreeBatch>,
        Option<&CardBatch>,
    )>,
    parents: Query<&ChildOf>,
    actors: Query<(), With<Actor>>,
) {
    if !state.metrics_overlay_visible && !dump.on {
        return;
    }
    *due = due.saturating_sub(time.delta());
    if !due.is_zero() {
        return;
    }
    *due = EVERY;

    let mut next = RenderCensus::default();
    for (entity, mesh, visible, summary, trees, cards) in &drawn {
        if !visible.get() {
            continue;
        }
        let Some(triangles) = meshes.get(&mesh.0).map(triangles_of) else { continue };
        match (summary, trees, cards) {
            (Some(_), _, _) => next.terrain.add(triangles, 1),
            (_, Some(batch), _) => next.forest.add(triangles, batch.len()),
            (_, _, Some(batch)) => next.forest.add(triangles, batch.len()),
            _ if under_actor(entity, &parents, &actors) => next.actors.add(triangles, 1),
            _ => next.other.add(triangles, 1),
        }
    }
    *census = next;
}

/// Whether this entity is an actor or hangs under one: a worn piece and
/// a rig's parts are separate meshes, and the actor is what they cost.
fn under_actor(entity: Entity, parents: &Query<&ChildOf>, actors: &Query<(), With<Actor>>) -> bool {
    if actors.contains(entity) {
        return true;
    }
    parents.iter_ancestors(entity).any(|a| actors.contains(a))
}
