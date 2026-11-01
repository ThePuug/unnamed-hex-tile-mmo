//! The target indicators: the outline of the tile under whoever the viewed
//! actor targets now, red for the hostile and green for the ally. They are
//! drawn afresh each frame from the targets as they stand, so an indicator
//! never lags its target or lingers after it. Unbuilt: a tier badge.

use bevy::prelude::*;

use common_bevy::{
    components::{ally_target::AllyTarget, resources::Health, target::Target, Loc},
    resources::map::Map,
};

use crate::{resources::RenderOrigin, systems::target_frame::{Lane, LANES}};

/// The gizmos the indicators are drawn with: a group of their own, so that
/// turning the debug gizmos off leaves them on screen.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TargetGizmos;

/// How far above the ground an outline is drawn
const LIFT: f32 = 0.05;

/// The outline's width, in pixels
const WIDTH: f32 = 4.0;

/// How an indicator is drawn: a line of [`WIDTH`], pulled toward the camera
/// enough that the ground it lies on never cuts through it.
pub fn config() -> GizmoConfig {
    GizmoConfig {
        line: GizmoLineConfig { width: WIDTH, ..default() },
        depth_bias: -0.01,
        ..default()
    }
}

fn color(lane: Lane) -> Color {
    match lane {
        Lane::Hostile => Color::srgb(1.0, 0.0, 0.0),
        Lane::Ally => Color::srgb(0.0, 1.0, 0.0),
    }
}

/// Outlines the tile under each lane's current target, never the sticky
/// `last_target`, following the ground's slope. Nothing is drawn for a
/// lane with no target, or while the viewed actor is dead.
pub fn update(
    mut gizmos: Gizmos<TargetGizmos>,
    viewed: Query<(&Target, Option<&AllyTarget>, &Health), With<crate::components::Viewed>>,
    locs: Query<&Loc>,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
) {
    let Ok((target, ally, health)) = viewed.single() else {
        return;
    };
    if health.state <= 0.0 {
        return;
    }
    for lane in LANES {
        let Some(loc) = lane.held(target, ally).and_then(|ent| locs.get(ent).ok()) else { continue };
        let Some((tile, _)) = map.get_by_qr(loc.q, loc.r) else { continue };
        // The corners about the tile's column, placed by its tile: the
        // render origin comes off as a tile difference, never a vector.
        let at = origin.render_tile(&map, tile);
        let foot = Vec3::new(at.x, LIFT, at.z);
        let corners = map.sloped_corners(tile);
        gizmos.linestrip((0..=6).map(|corner| foot + corners[corner % 6]), color(lane));
    }
}
