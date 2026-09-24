//! Movement physics as pure functions: a position, a heading, the held
//! keys and a duration in; a position, heading, turn clock and airborne
//! state out.
//!
//! The result does not depend on how a duration is split across calls.
//! Every sub-step is linear in time and the ground height is a function of
//! position, so a client replaying its inputs and a server applying them as
//! they arrive reach the same place. Anything added here must keep that:
//! no per-call smoothing, no per-step constants that are not scaled by `dt`.

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use crate::{
    components::{
        entity_type::{decorator::*, EntityType},
        equipment::Burdened,
        heading::Heading,
        position::Position,
        Loc,
    },
    plugins::nntree::NNTree,
    resources::map::Map,
};

/// Gravity: how fast a fall gathers speed, in world units per millisecond
/// squared. A fall must outrun a walk down any slope or an entity that
/// leaves the ground on a hillside floats to its foot, and a slope's
/// descent has no bound, so no constant fall speed does.
pub const GRAVITY: f32 = 2.0e-5;

/// A jump's ascent: its speed in world units per millisecond, held for
/// `JUMP_DURATION_MS`, then the fall.
pub const JUMP_ASCENT: f32 = 0.025;

/// Jump duration in milliseconds
pub const JUMP_DURATION_MS: i16 = 125;

/// Longest sub-step in milliseconds. Bounds how far a step can carry
/// before blocking and landing are checked again.
pub const PHYSICS_TIMESTEP_MS: i16 = 125;

/// Base movement speed in world units per millisecond
pub const MOVEMENT_SPEED: f32 = 0.0075;

/// The share of its speed an overburdened entity keeps: slow enough that
/// its gait plays the walk, which the actors' walk stride sets.
pub const BURDENED_PACE: f32 = 0.2;

/// The speed an entity moves at: its own, or [`BURDENED_PACE`] of it
/// overburdened.
/// Every caller of the physics takes its speed through here, so the
/// server, the owner's prediction and every remote simulation agree.
pub fn speed(own: f32, burdened: bool) -> f32 {
    if burdened { own * BURDENED_PACE } else { own }
}

/// Keeps [`Burdened`] on every entity whose bag weighs past the limit: on
/// the server for every player, on the client for its own, whose bag it
/// holds. A remote entity's comes with its intent.
pub fn update_burden(
    mut commands: Commands,
    bags: Query<(Entity, &crate::components::equipment::Inventory, Has<Burdened>), Changed<crate::components::equipment::Inventory>>,
) {
    for (ent, bag, burdened) in &bags {
        match (bag.is_burdened(), burdened) {
            (true, false) => { commands.entity(ent).insert(Burdened); }
            (false, true) => { commands.entity(ent).remove::<Burdened>(); }
            _ => {}
        }
    }
}

/// Milliseconds of input time between steps of a held turn key, and the
/// least time between any two steps: a tap after a rest turns at once, and a
/// hammered key turns no faster than a held one.
pub const TURN_REPEAT_MS: u16 = 80;

/// Ledge grab threshold in world units
/// Set to 0.0 to disable ledge grabbing
pub const LEDGE_GRAB_THRESHOLD: f32 = 0.0;

/// Maximum entity count per tile before considering it solid
pub const MAX_ENTITIES_PER_TILE: usize = 7;

/// The depth of water a walker wades, in z-levels: one, the same step that
/// bounds a climb. Deeper water is not entered; crossing it, by swimming, a
/// ford, a bridge or a boat, is undesigned, so it blocks.
pub const WADE_DEPTH: i32 = 1;

/// Whether a tile stands under more water than a walker wades.
pub fn is_deep_water(map: &Map, q: i32, r: i32) -> bool {
    match (map.water_at(q, r), map.get_by_qr(q, r)) {
        (Some(surface), Some((floor, _))) => surface - floor.z > WADE_DEPTH,
        _ => false,
    }
}

/// The fullness at which a tile refuses entry: more than half its
/// [`common::TILE_SLOTS`] held by solid things, so two trees close it.
pub const COVER_FULL: u8 = 4;

/// The fullness a walker crosses at full pace: one tree's slots.
pub const COVER_FREE: u8 = 2;

/// A walker's pace through a tile one slot short of full, as a share of
/// full: the slowest it goes before the last refuses it.
pub const COVER_PACE_MIN: f32 = 0.5;

/// A walker's pace through a tile as a share of full, by the tile's
/// fullness: [`COVER_FREE`] costs nothing, and from there the pace eases
/// to [`COVER_PACE_MIN`] one short of full. Full is refused in
/// [`is_tile_blocked`], never slowed to nothing.
pub fn pace(fullness: u8) -> f32 {
    let span = COVER_FULL - 1 - COVER_FREE;
    let n = fullness.saturating_sub(COVER_FREE).min(span) as f32;
    1.0 - n / span as f32 * (1.0 - COVER_PACE_MIN)
}

/// Whether a tile's solid things fill it, so nothing walks in.
pub fn is_full_cover(map: &Map, q: i32, r: i32) -> bool {
    map.cover_at(q, r).fullness() >= COVER_FULL
}

/// How far a fall `fallen_ms` old drops over its next `dt` milliseconds:
/// the integral of the speed gravity has built, so a fall sliced any way
/// drops the same.
pub fn fall(fallen_ms: f32, dt: f32) -> f32 {
    GRAVITY * (fallen_ms * dt + dt * dt / 2.0)
}

/// How long a fall `fallen_ms` old takes to drop `height` more, in
/// milliseconds.
pub fn fall_time(height: f32, fallen_ms: f32) -> f32 {
    let v = GRAVITY * fallen_ms;
    ((v * v + 2.0 * GRAVITY * height.max(0.0)).sqrt() - v) / GRAVITY
}

/// World height of the standing level over `floor`: one z-level up.
pub fn standing_y(floor: Qrz, map: &Map) -> f32 {
    let standing: Vec3 = map.convert(Qrz { z: floor.z + 1, ..floor });
    standing.y
}

/// Height of the terrain surface under `world_xz`, standing on `floor` — the
/// same fan surface the mesh draws (`surface::surface_y`), so an entity's
/// feet stay on what is rendered, across tile edges and up to a cliff's
/// face. Whether a tile may be entered is decided on tile z elsewhere; this
/// only says how high the ground is. The point is a world vector, so the
/// sample is as exact as that vector; the walk uses [`surface_y_from`].
pub fn surface_y(world_xz: Vec2, floor: Qrz, map: &Map) -> f32 {
    let floor_centre: Vec3 = map.convert(floor);
    floor_centre.y + surface_y_from(floor, world_xz - floor_centre.xz(), floor, map)
}

/// The surface's height over the level of `tile`, under a point given as
/// `xz` from that tile's centre: the walk's frame. Every height and
/// distance is taken as a difference from `tile`, so the sample is exact
/// however far out, or up, the tile is.
pub fn surface_y_from(tile: Qrz, xz: Vec2, floor: Qrz, map: &Map) -> f32 {
    let floor_centre: Vec3 = map.convert(floor - tile);
    crate::surface::surface_y(xz, Qrz { z: floor.z - tile.z, ..floor }, floor_centre, |q, r| {
        map.get_by_qr(q, r).map(|(qrz, _)| qrz.z - tile.z)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct MovementInput {
    pub position: Position,
    /// The facing, and the direction a forward move travels along.
    pub heading: Heading,
    pub moving: bool,
    /// The move travels opposite the heading.
    pub back: bool,
    /// Bearings the heading steps per repeat: -1 counter-clockwise, 0, 1
    /// clockwise.
    pub turn: i8,
    /// Milliseconds since the heading last stepped, at most
    /// [`TURN_REPEAT_MS`].
    pub since_step_ms: u16,
    /// Some(positive) = ascending, Some(negative or zero) = falling, None = grounded
    pub airtime: Option<i16>,
    /// World units per millisecond
    pub movement_speed: f32,
    /// Whether the walker is a player's pill, which goes round the
    /// footprints of what stands in a tile; anything else walks through
    /// them, and only a tile's own rules stop it.
    pub collides: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementOutput {
    pub position: Position,
    pub heading: Heading,
    pub since_step_ms: u16,
    pub airtime: Option<i16>,
}

/// Whether a step from the tile over `here_floor` may land in `next`, for an
/// entity whose feet stand `y` over the level of `tile`, the tile its walk
/// is measured from. Refused by a rise of more than one level unless
/// airborne at or above its standing height, by a solid decorator with no
/// floor, by a tile at its entity capacity, by water deeper than a walker
/// wades, and by trees in every slot.
pub fn is_tile_blocked(
    tile: Qrz,
    here_floor: Option<Qrz>,
    next: Qrz,
    y: f32,
    airtime: Option<i16>,
    map: &Map,
    nntree: &NNTree,
) -> bool {
    let next_floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);

    let cliff = match (here_floor, next_floor) {
        (Some(here), Some(there)) if there.z - here.z > 1 => {
            let standing = (there.z + 1 - tile.z) as f32 * map.rise();
            airtime.is_none() || y + LEDGE_GRAB_THRESHOLD < standing
        }
        _ => false,
    };

    let solid = match map.get(next) {
        Some(EntityType::Decorator(Decorator { is_solid, .. })) => is_solid,
        _ => nntree.locate_all_at_point(&Loc::new(next)).count() >= MAX_ENTITIES_PER_TILE,
    };

    cliff || (solid && next_floor.is_none()) || is_deep_water(map, next.q, next.r) || is_full_cover(map, next.q, next.r)
}

/// How far short of a refused face a walk stops, in world units: a hair, so
/// the position still converts to its own tile and never the refused one. A
/// distance from the face, not per step, so the stop is the same however
/// the time is sliced.
const FACE_MARGIN: f32 = 0.01;

/// Faces one walk may meet. A sub-step is shorter than a tile, so a few
/// crossings and slides cover it; the cap only ends the bounce between the
/// two walls of a concave corner, which makes no progress.
const FACES_PER_WALK: usize = 16;

/// Displacement of a walk of `reach` world units along the unit `heading`
/// from `from`, a ground-plane point given from the centre of `tile`, in
/// `here` whose floor is `floor`. The walk crosses the faces
/// [`is_tile_blocked`] allows and slides along the ones it refuses: against
/// a face only the heading's component along it is kept, so a graze keeps
/// most of its speed, a head-on push stands, and past the face's end the
/// heading resumes. Each tile is crossed at its own [`pace`], read at the
/// face, so a slow tile spends the reach faster over the part of the walk
/// inside it and the same however the walk is sliced. The faces are read
/// with the tiles taken relative to `tile`, so the geometry is exact
/// however far out the tile is; only the lookups name the tiles
/// themselves. `y` is the feet's height over the level of `tile`.
fn walk(
    tile: Qrz,
    from: Vec2,
    heading: Vec2,
    reach: f32,
    mut here: Qrz,
    mut floor: Option<Qrz>,
    y: f32,
    airtime: Option<i16>,
    collides: bool,
    map: &Map,
    nntree: &NNTree,
) -> Vec2 {
    let footprints = |here: Qrz| if collides { footprints(tile, here, map) } else { Vec::new() };
    let mut pos = from;
    let mut left = reach;
    let mut dir = heading;
    // Speed along `dir` as a fraction of full: the cosine of the incidence
    // while sliding, so a slide spends `left` faster than it covers ground.
    let mut rate = 1.0;
    let mut pace_here = pace(map.cover_at(here.q, here.r).fullness());
    let mut about = footprints(here);
    for _ in 0..FACES_PER_WALK {
        let (to_face, next) = map.exit(pos, dir, here - tile);
        let next = next + tile;
        let speed = rate * pace_here;
        let run = left * speed;

        // The first footprint the run meets before the face: the walker goes
        // round it on its circle while the heading presses into it.
        let met = about
            .iter()
            .filter_map(|&(centre, radius)| contact(pos, dir, centre, radius).map(|s| (s, centre, radius)))
            .filter(|&(s, ..)| s <= run.min(to_face))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((s, centre, radius)) = met {
            pos += dir * s;
            left -= s / speed;
            // Met while sliding along a refused face, it stands in the corner.
            if rate < 1.0 {
                break;
            }
            let (to, spent, clear) = round(pos, centre, radius, heading, pace_here, left);
            let rel = map.convert(Vec3::new(to.x, 0.0, to.y));
            let at = Qrz { q: tile.q + rel.q, r: tile.r + rel.r, z: here.z };
            if (at.q, at.r) != (here.q, here.r) {
                // Round into a tile it may not enter, it stands at the contact.
                if is_tile_blocked(tile, floor, at, y, airtime, map, nntree) {
                    break;
                }
                here = at;
                floor = map.get_by_qr(at.q, at.r).map(|(floor, _)| floor);
                pace_here = pace(map.cover_at(at.q, at.r).fullness());
                about = footprints(here);
            }
            pos = to;
            left -= spent;
            if !clear {
                break;
            }
            dir = heading;
            continue;
        }

        if run <= to_face {
            pos += dir * run;
            break;
        }
        if is_tile_blocked(tile, floor, next, y, airtime, map, nntree) {
            let short = (to_face - FACE_MARGIN).max(0.0);
            pos += dir * short;
            left -= short / speed;
            let along = map.face(here - tile, next - tile).0.perp();
            let kept = heading.dot(along);
            if kept.abs() < 1e-6 {
                break;
            }
            dir = along * kept.signum();
            rate = kept.abs();
        } else {
            pos += dir * to_face;
            left -= to_face / speed;
            here = next;
            floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);
            pace_here = pace(map.cover_at(next.q, next.r).fullness());
            about = footprints(here);
            dir = heading;
            rate = 1.0;
        }
    }
    pos - from
}

/// How far a walker keeps from what it walks round: the half-width of the
/// pill a player's body is, standing.
pub const WALKER_RADIUS: f32 = 0.3;

/// How high a walker's waist stands, in world units: what stands lower is
/// stepped over and has no footprint.
pub const WAIST: f32 = 0.72;

/// How wide and how tall the solid object in slot `k` of tile `(q, r)`
/// stands, as it is drawn: a tree by its form and growth, a stump by its
/// tree's, a boulder by its size. None for what is not solid, or names no
/// object from its slot: the second slot a tree spans.
pub fn solid_extent(cover: common::Cover, q: i32, r: i32, k: usize) -> Option<(f32, f32)> {
    let content = cover.content(k);
    match content {
        common::Content::Pine | common::Content::Deciduous | common::Content::PineStump | common::Content::DeciduousStump => {
            let site = common::SITE_SLOTS.iter().position(|s| s[0] == k)?;
            let form = common::cover::tree_form(content, common::sway(q, r, site).variation)?;
            let scale = common::cover::tree_scale(form.height, common::cover::tree_growth(cover, q, r, site));
            let stump = matches!(content, common::Content::PineStump | common::Content::DeciduousStump);
            Some((form.trunk * scale, if stump { form.stump } else { form.height } * scale))
        }
        common::Content::Boulder => {
            let scale = common::cover::boulder_scale(common::cover::boulder_growth(cover, q, r, k));
            Some((common::cover::BOULDER_RADIUS * scale, common::cover::BOULDER_HEIGHT * scale))
        }
        _ => None,
    }
}

/// The footprints a walker goes round in `about` and the six tiles around
/// it: each solid object taller than the waist, its centre from the centre
/// of `tile` and the radius the walker keeps from it, as wide as it is
/// drawn and the walker's own half-width more.
fn footprints(tile: Qrz, about: Qrz, map: &Map) -> Vec<(Vec2, f32)> {
    let mut out = Vec::new();
    for offset in std::iter::once(Qrz::default()).chain(qrz::DIRECTIONS) {
        let (q, r) = (about.q + offset.q, about.r + offset.r);
        let cover = map.cover_at(q, r);
        if cover.is_empty() {
            continue;
        }
        for k in 0..common::TILE_SLOTS as usize {
            let Some((radius, _)) = solid_extent(cover, q, r, k).filter(|&(_, height)| height > WAIST) else { continue };
            let (x, z) = crate::geometry::slot_point(cover, (q, r), k, (tile.q, tile.r));
            out.push((Vec2::new(x, z), radius + WALKER_RADIUS));
        }
    }
    out
}

/// Where a player at `from`, in the frame of `tile`, turned to `heading`,
/// stands to do `activity` at slot `k` of tile `(q, r)`: on the line
/// through what it works along the heading, as far off as the work
/// reaches past its drawn edge, so the edge, point or fists land on it.
/// None where the spot's tile may not be entered from the player's, or a
/// footprint other than what it works stands in the way there.
#[allow(clippy::too_many_arguments)]
pub fn stand_point(
    tile: Qrz,
    from: Vec3,
    heading: Heading,
    (q, r): (i32, i32),
    k: usize,
    activity: common::gathering::Activity,
    map: &Map,
    nntree: &NNTree,
) -> Option<Vec2> {
    let cover = map.cover_at(q, r);
    let (x, z) = crate::geometry::slot_point(cover, (q, r), k, (tile.q, tile.r));
    let target = Vec2::new(x, z);
    let edge = match activity {
        common::gathering::Activity::Pickup => 0.0,
        common::gathering::Activity::Work(_) => solid_extent(cover, q, r, k).map_or(0.0, |(radius, _)| radius),
    };
    let stand = target - heading.to_world_dir() * (common::gathering::reach(activity) + edge);

    let here: Qrz = tile + map.convert(from);
    let rel = map.convert(Vec3::new(stand.x, 0.0, stand.y));
    let at = Qrz { q: tile.q + rel.q, r: tile.r + rel.r, z: here.z };
    if (at.q, at.r) != (here.q, here.r) {
        let floor = map.get_by_qr(here.q, here.r).map(|(floor, _)| floor);
        if is_tile_blocked(tile, floor, at, from.y, None, map, nntree) {
            return None;
        }
    }
    let start = from.xz();
    let blocked = footprints(tile, here, map).into_iter().chain(footprints(tile, at, map)).any(|(centre, radius)| {
        centre.distance(target) > 1e-4 && segment_distance(start, stand, centre) < radius - 1e-4
    });
    (!blocked).then_some(stand)
}

/// `position` moved to `stand`, given in its tile's frame, on the ground
/// there.
pub fn stepped(position: Position, stand: Vec2, map: &Map) -> Position {
    let here: Qrz = position.tile + map.convert(Vec3::new(stand.x, 0.0, stand.y));
    let y = map.get_by_qr(here.q, here.r).map_or(position.offset.y, |(floor, _)| surface_y_from(position.tile, stand, floor, map));
    Position { tile: position.tile, offset: Vec3::new(stand.x, y, stand.y) }
}

/// How near the segment from `a` to `b` comes to `p`.
fn segment_distance(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    let ab = b - a;
    let s = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * s).distance(p)
}

/// How far along the unit `dir` a walker at `pos` meets the circle of
/// `radius` about `centre`: none where it moves off or along it, none where
/// the line misses it, and at once where it already stands on or in it
/// moving inward.
fn contact(pos: Vec2, dir: Vec2, centre: Vec2, radius: f32) -> Option<f32> {
    let m = pos - centre;
    let b = m.dot(dir);
    if b >= -1e-6 {
        return None;
    }
    let c = m.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    Some((-b - disc.sqrt()).max(0.0))
}

/// A walker at `pos` on the circle of `radius` about `centre`, pressed into
/// it along the unit `heading`, goes round it with the heading's part along
/// the circle, at `pace`, for `left` of its reach. Returns where it ends,
/// the reach it spent, and whether it came clear: where the heading turns
/// along the circle it leaves it, and head-on it stands.
///
/// Solved, not stepped: with β the heading's bearing less the walker's
/// about the centre, dβ = −pace·sin β / radius per reach, whose solution
/// tan(β/2) = tan(β₀/2)·e^(−pace·reach/radius) splits into any two walks
/// as it does into one, so the result is the same however the time is
/// sliced. The walker comes clear where |β| falls to a right angle.
fn round(pos: Vec2, centre: Vec2, radius: f32, heading: Vec2, pace: f32, left: f32) -> (Vec2, f32, bool) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let alpha = heading.y.atan2(heading.x);
    let m = pos - centre;
    let at = |theta: f32| centre + Vec2::new(theta.cos(), theta.sin()) * radius;
    let beta0 = (alpha - m.y.atan2(m.x) + PI).rem_euclid(2.0 * PI) - PI;
    if beta0.abs() <= FRAC_PI_2 {
        return (pos, 0.0, true);
    }
    let half = (beta0 / 2.0).tan();
    if half.abs() > 1e6 {
        return (pos, left, false);
    }
    let rate = pace / radius;
    let to_clear = half.abs().ln() / rate;
    if left < to_clear {
        let beta = 2.0 * (half * (-rate * left).exp()).atan();
        return (at(alpha - beta), left, false);
    }
    (at(alpha - FRAC_PI_2 * beta0.signum()), to_clear, true)
}

/// Advance `input` by `dt0` milliseconds in sub-steps of at most
/// [`PHYSICS_TIMESTEP_MS`]. The tile stays `input.position.tile`; the offset
/// may leave it, and the caller re-bases when the world position crosses
/// into another tile. A held turn key steps the heading when the turn clock
/// fills, and the walk is split there, so the part before a step runs on
/// the old heading and the part after on the new whatever the partition.
/// Everything runs in the tile's frame — its centre the origin, the offset
/// the position, neighbours and heights as tile differences — so the
/// result is the same however far out the tile is: a world vector would
/// keep only float steps there, and a tick's step is smaller than one.
pub fn calculate_movement(
    input: MovementInput,
    mut dt0: i16,
    map: &Map,
    nntree: &NNTree,
) -> MovementOutput {
    let tile = input.position.tile;
    let mut offset = input.position.offset;
    let mut airtime = input.airtime;
    let mut heading = input.heading;
    let mut since_step_ms = input.since_step_ms;

    while dt0 > 0 {
        let mut dt = dt0.min(PHYSICS_TIMESTEP_MS);
        if input.turn != 0 {
            if since_step_ms >= TURN_REPEAT_MS {
                heading = heading.turned(input.turn as i32);
                since_step_ms = 0;
            }
            dt = dt.min((TURN_REPEAT_MS - since_step_ms) as i16);
        }
        dt0 -= dt;
        let dir = if input.back { heading.reversed() } else { heading }.to_world_dir();

        let here: Qrz = tile + map.convert(offset);
        let floor = map.get_by_qr(here.q, here.r).map(|(floor, _)| floor);

        // Over nothing, or more than a level above the ground under it, a
        // grounded entity starts to fall. The ground is the surface, never
        // the tile's level: on a slope the surface stands well above it in
        // the uphill part of a tile, and a level test read there falls
        // every few ticks.
        if airtime.is_none() && floor.map_or(true, |floor| offset.y > surface_y_from(tile, offset.xz(), floor, map) + map.rise()) {
            airtime = Some(0);
        }

        if let Some(mut air) = airtime {
            if air > 0 {
                // Ascending: split the sub-step at the apex so the descent
                // starts on the exact millisecond whatever the partition.
                if air < dt {
                    dt0 += dt - air;
                    dt = air;
                }
                air -= dt;
                airtime = Some(air);
                offset.y += dt as f32 * JUMP_ASCENT;
            } else {
                // Falling: `air` counts the fall's age below zero.
                let dy = -fall(-(air as i32) as f32, dt as f32);
                air = air.saturating_sub(dt);
                airtime = Some(air);
                match floor.map(|floor| surface_y_from(tile, offset.xz(), floor, map)) {
                    Some(ground) if offset.y + dy <= ground => {
                        offset.y = ground;
                        airtime = None;
                    }
                    _ => offset.y += dy,
                }
            }
        }

        // After the apex split, which may have shortened the sub-step.
        since_step_ms = (since_step_ms + dt as u16).min(TURN_REPEAT_MS);

        if input.moving {
            let moved = walk(
                tile, offset.xz(), dir, input.movement_speed * dt as f32,
                here, floor, offset.y, airtime, input.collides, map, nntree,
            );
            offset.x += moved.x;
            offset.z += moved.y;
        }

        let here: Qrz = tile + map.convert(offset);
        if let Some((floor, _)) = map.get_by_qr(here.q, here.r) {
            let ground = surface_y_from(tile, offset.xz(), floor, map);
            if airtime.is_none() {
                offset.y = ground;
            } else {
                offset.y = offset.y.max(ground);
            }
        }
    }

    MovementOutput {
        position: Position::new(tile, offset),
        heading,
        since_step_ms,
        airtime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop))
    }

    fn create_test_nntree() -> NNTree {
        NNTree::new_for_test()
    }

    fn flat_ground(map: &Map, radius: i32) {
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -radius..=radius {
            for r in -radius..=radius {
                map.insert(Qrz { q, r, z: 0 }, ground);
            }
        }
    }

    fn walking(heading: Heading, moving: bool) -> MovementInput {
        MovementInput {
            position: Position::at_tile(Qrz { q: 0, r: 0, z: 1 }),
            heading,
            moving,
            back: false,
            turn: 0,
            since_step_ms: TURN_REPEAT_MS,
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
            collides: true,
        }
    }

    /// A burden slows a walk and never stops it or turns it.
    #[test]
    fn a_burden_slows_a_walk() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let free = calculate_movement(walking(Heading::NORTH, true), 200, &map, &nntree);
        let slow = MovementInput { movement_speed: speed(MOVEMENT_SPEED, true), ..walking(Heading::NORTH, true) };
        let slow = calculate_movement(slow, 200, &map, &nntree);
        let (free, slow) = (free.position.offset.xz(), slow.position.offset.xz());
        assert!(slow.length() > 0.0 && slow.length() < free.length());
        assert!(slow.normalize().dot(free.normalize()) > 0.999);
    }

    /// Flat open ground with `cover` on the tile north of the walker's, and
    /// the point in slot `k` of it from the walker's tile.
    fn wood(cover: common::Cover, k: usize) -> (Map, NNTree, Vec2) {
        let map = create_test_map();
        flat_ground(&map, 3);
        let north = Qrz { q: 0, r: -1, z: 0 };
        map.insert(north, EntityType::Decorator(Decorator { cover, is_solid: false }));
        let (x, z) = crate::geometry::slot_point(cover, (north.q, north.r), k, (0, 0));
        (map, create_test_nntree(), Vec2::new(x, z))
    }

    /// A walker heading north from `at`, from the walker's tile's centre.
    fn walking_from(at: Vec2) -> MovementInput {
        let mut input = walking(Heading::NORTH, true);
        input.position.offset = Vec3::new(at.x, input.position.offset.y, at.y);
        input
    }

    /// How far a walker keeps from the trunk in site 0 of the tile north.
    fn trunk_keep(cover: common::Cover) -> f32 {
        let north = (0, -1);
        let form = common::cover::tree_form(cover.content(common::SITE_SLOTS[0][0]), common::sway(north.0, north.1, 0).variation).unwrap();
        form.trunk * common::cover::tree_scale(form.height, common::cover::tree_growth(cover, north.0, north.1, 0)) + WALKER_RADIUS
    }

    /// Straight into a trunk, a walker stands against it, never in it.
    #[test]
    fn a_trunk_stands_a_walk_into_it() {
        let site = common::SITE_SLOTS[0][0];
        let cover = common::Cover::NONE.with(0, common::Content::Pine);
        let (map, nntree, trunk) = wood(cover, site);
        let out = calculate_movement(walking_from(trunk + Vec2::new(0.0, 2.0)), 800, &map, &nntree);
        let d = out.position.offset.xz().distance(trunk);
        assert!((d - trunk_keep(cover)).abs() < 1e-3, "stood {d} from the trunk's centre");
    }

    /// A chop stands the player on its heading's line through the trunk,
    /// as far from the trunk's drawn edge as the axe reaches.
    #[test]
    fn a_chop_stands_the_player_where_the_axe_meets_the_trunk() {
        let site = common::SITE_SLOTS[0][0];
        let cover = common::Cover::NONE.with(0, common::Content::Pine);
        let (map, nntree, trunk) = wood(cover, site);
        let from = trunk + Vec2::new(0.4, 2.5);
        let facing = Heading::facing(from, trunk).unwrap();
        let chop = common::gathering::Activity::Work(common::gathering::Work::Chop);
        let stand = stand_point(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(from.x, 0.0, from.y), facing, (0, -1), site, chop, &map, &nntree).unwrap();
        let (radius, _) = solid_extent(cover, 0, -1, site).unwrap();
        assert!((stand.distance(trunk) - (common::gathering::CHOP_REACH + radius)).abs() < 1e-4);
        let along = (trunk - stand).normalize();
        assert!(along.dot(facing.to_world_dir()) > 0.9999, "the trunk lies straight ahead of the stand");
    }

    /// A stand in a tile the player may not enter is no stand.
    #[test]
    fn a_stand_in_a_full_tile_is_none() {
        let site = common::SITE_SLOTS[0][0];
        let cover = common::Cover::NONE.with(0, common::Content::Pine);
        let (map, nntree, trunk) = wood(cover, site);
        let from = trunk + Vec2::new(0.0, 3.0);
        let chop = common::gathering::Activity::Work(common::gathering::Work::Chop);
        let stand = stand_point(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(from.x, 0.0, from.y), Heading::NORTH, (0, -1), site, chop, &map, &nntree).unwrap();
        let rel = map.convert(Vec3::new(stand.x, 0.0, stand.y));
        let full = common::Cover::NONE.with(0, common::Content::Pine).with(1, common::Content::Pine);
        map.insert(Qrz { q: rel.q, r: rel.r, z: 0 }, EntityType::Decorator(Decorator { cover: full, is_solid: false }));
        if (rel.q, rel.r) != (0, 0) {
            assert_eq!(stand_point(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(from.x, 0.0, from.y), Heading::NORTH, (0, -1), site, chop, &map, &nntree), None);
        }
    }

    /// Only a player's pill goes round a trunk; anything else walks through.
    #[test]
    fn only_a_pill_goes_round_a_trunk() {
        let site = common::SITE_SLOTS[0][0];
        let (map, nntree, trunk) = wood(common::Cover::NONE.with(0, common::Content::Pine), site);
        let input = MovementInput { collides: false, ..walking_from(trunk + Vec2::new(0.0, 2.0)) };
        let out = calculate_movement(input, 800, &map, &nntree);
        assert!(out.position.offset.z < trunk.y - 0.5, "walked through to {:?}", out.position.offset);
    }

    /// Grazing a trunk, a walker goes round it and on past it, as far out
    /// as it keeps, whatever the slicing.
    #[test]
    fn a_grazing_walk_goes_round_a_trunk() {
        let site = common::SITE_SLOTS[0][0];
        let cover = common::Cover::NONE.with(0, common::Content::Pine);
        let (map, nntree, trunk) = wood(cover, site);
        let keep = trunk_keep(cover);
        let input = walking_from(trunk + Vec2::new(keep * 0.5, 2.0));
        let whole = calculate_movement(input, 800, &map, &nntree);
        let end = whole.position.offset.xz();
        assert!(end.y < trunk.y - 0.3, "the walker got past: {end:?} vs trunk {trunk:?}");
        assert!(end.distance(trunk) >= keep - 1e-3);
        for dt in [7, 50, 125] {
            let (mut sliced, mut out, mut left) = (input, whole, 800);
            while left > 0 {
                let step = dt.min(left);
                out = calculate_movement(sliced, step, &map, &nntree);
                sliced = carried(sliced, &out);
                left -= step;
            }
            assert!(out.position.offset.distance(whole.position.offset) < 1e-3, "slice {dt}: {:?} vs {:?}", out.position.offset, whole.position.offset);
        }
    }

    /// A stump and a boulder lower than the waist are stepped over.
    #[test]
    fn what_stands_below_the_waist_is_stepped_over() {
        let site = common::SITE_SLOTS[0][0];
        let felled = common::gathering::harvest(common::Cover::NONE.with(0, common::Content::Pine), site).unwrap().cover;
        let (map, nntree, stump) = wood(felled, site);
        let out = calculate_movement(walking_from(stump + Vec2::new(0.0, 1.0)), 500, &map, &nntree);
        assert!(out.position.offset.z < stump.y - 0.5, "a stump stood the walker at {:?}", out.position.offset);

        let stone = common::Cover::NONE.with_boulder(0).with_rock(common::Rock::Sandstone);
        assert!(common::cover::BOULDER_HEIGHT * common::cover::boulder_scale(common::cover::boulder_growth(stone, 0, -1, 0)) <= WAIST);
        let (map, nntree, boulder) = wood(stone, 0);
        let out = calculate_movement(walking_from(boulder + Vec2::new(0.0, 1.0)), 500, &map, &nntree);
        assert!(out.position.offset.z < boulder.y - 0.5, "a low boulder stood the walker at {:?}", out.position.offset);
    }

    /// `input` carried on by `out`: position, heading and clock.
    fn carried(input: MovementInput, out: &MovementOutput) -> MovementInput {
        MovementInput { position: out.position, heading: out.heading, since_step_ms: out.since_step_ms, ..input }
    }

    #[test]
    fn a_tap_turns_at_once_and_a_held_key_repeats() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let held = MovementInput { turn: 1, ..walking(Heading::NORTH, false) };
        let tap = calculate_movement(held, 1, &map, &nntree);
        assert_eq!(tap.heading, Heading::NORTH.turned(1), "a rested key steps on the first millisecond");
        let long = calculate_movement(held, TURN_REPEAT_MS as i16 * 2 + 10, &map, &nntree);
        assert_eq!(long.heading, Heading::NORTH.turned(3), "then once per repeat");
        let left = calculate_movement(MovementInput { turn: -1, ..held }, 1, &map, &nntree);
        assert_eq!(left.heading, Heading::NORTH.turned(-1));
    }

    #[test]
    fn a_hammered_key_turns_no_faster_than_a_held_one() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let mut input = MovementInput { turn: 1, ..walking(Heading::NORTH, false) };
        let (mut steps, mut elapsed) = (0, 0);
        while elapsed < 800 {
            for turn in [1, 0] {
                let out = calculate_movement(MovementInput { turn, ..input }, 10, &map, &nntree);
                if out.heading != input.heading { steps += 1; }
                input = carried(input, &out);
                elapsed += 10;
            }
        }
        assert_eq!(steps, 800 / TURN_REPEAT_MS as i32, "one step per repeat of input time");
    }

    /// A turn while walking bends the path at the step, and the bend lands
    /// on the same millisecond however the time is sliced.
    #[test]
    fn turning_while_walking_is_partition_independent() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { turn: -1, since_step_ms: 30, ..walking(Heading::from_slot(6), true) };
        let whole = calculate_movement(input, 300, &map, &nntree);
        assert_ne!(whole.heading, input.heading);
        assert!(whole.position.offset.xz().length() > 0.0);
        for dt in [7, 50, 125] {
            let mut sliced = input;
            let mut out = whole;
            let mut left = 300;
            while left > 0 {
                let step = dt.min(left);
                out = calculate_movement(sliced, step, &map, &nntree);
                sliced = carried(sliced, &out);
                left -= step;
            }
            assert_eq!(out.heading, whole.heading, "slice {dt}");
            assert_eq!(out.since_step_ms, whole.since_step_ms, "slice {dt}");
            assert!(out.position.offset.distance(whole.position.offset) < 1e-3, "slice {dt}: {:?} vs {:?}", out.position.offset, whole.position.offset);
        }
    }

    #[test]
    fn walking_back_travels_opposite_the_heading_and_keeps_it() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let east = Heading::from_degrees(90.0);
        let forward = calculate_movement(walking(east, true), 100, &map, &nntree);
        let back = calculate_movement(MovementInput { back: true, ..walking(east, true) }, 100, &map, &nntree);
        assert!((forward.position.offset.xz() + back.position.offset.xz()).length() < 1e-4, "{:?} vs {:?}", forward.position.offset, back.position.offset);
        assert_eq!(back.heading, east);
    }

    #[test]
    fn movement_is_deterministic() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { airtime: Some(50), ..walking(Heading::from_degrees(90.0), true) };
        assert_eq!(calculate_movement(input, 125, &map, &nntree), calculate_movement(input, 125, &map, &nntree));
    }

    #[test]
    fn a_move_travels_along_its_heading() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        for slot in 0..crate::components::heading::HEADING_SLOTS {
            let heading = Heading::from_slot(slot);
            let out = calculate_movement(walking(heading, true), 100, &map, &nntree);
            let travelled = out.position.offset.xz();
            let expected = heading.to_world_dir() * MOVEMENT_SPEED * 100.0;
            assert!(travelled.abs_diff_eq(expected, 1e-4), "slot {slot}: {travelled} vs {expected}");
        }
    }

    #[test]
    fn standing_still_stays_put() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let start = Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(0.4, 0.0, -0.3));
        let input = MovementInput { position: start, ..walking(Heading::from_slot(5), false) };
        let out = calculate_movement(input, 500, &map, &nntree);
        assert_eq!(out.position.offset.xz(), start.offset.xz(), "nothing pulls an idle entity anywhere");
    }

    /// The pace never rises with fullness, costs nothing to one tree, and
    /// never falls to nothing: full refuses, it does not stall.
    #[test]
    fn pace_falls_with_fullness_and_never_to_nothing() {
        assert_eq!(pace(0), 1.0);
        assert_eq!(pace(COVER_FREE), 1.0);
        let mut last = 1.0;
        for n in 0..=COVER_FULL {
            let p = pace(n);
            assert!(p <= last && p >= COVER_PACE_MIN - 1e-6, "pace {p} at {n}");
            last = p;
        }
        assert!((pace(COVER_FULL - 1) - COVER_PACE_MIN).abs() < 1e-6);
    }

    /// A tile of `n` trees at (q, r), on the flat ground.
    fn wooded(map: &Map, q: i32, r: i32, n: usize) {
        use common::{Cover, Content};
        let mut cover = Cover::NONE;
        for k in 0..n {
            cover = cover.with(k, Content::Pine);
        }
        map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: false }));
    }

    /// A walk through a wood short of full covers what its pace gives, the
    /// same however the time is sliced; two trees make a wall.
    #[test]
    fn a_wood_short_of_full_is_walked_and_two_trees_stop_it() {
        let nntree = create_test_nntree();
        let heading = Heading::from_slot(0);
        let input = walking(heading, true);
        let open = {
            let map = create_test_map();
            flat_ground(&map, 6);
            calculate_movement(input, 1000, &map, &nntree).position.offset.xz().length()
        };
        for n in 1..2 {
            let map = create_test_map();
            flat_ground(&map, 6);
            for q in -6..=6 {
                for r in -6..=6 {
                    wooded(&map, q, r, n);
                }
            }
            let whole = calculate_movement(input, 1000, &map, &nntree);
            let mut sliced = input;
            for _ in 0..40 {
                sliced = carried(sliced, &calculate_movement(sliced, 25, &map, &nntree));
            }
            assert!(whole.position.offset.abs_diff_eq(sliced.position.offset, 1e-3), "{n} trees: whole {:?} vs sliced {:?}", whole.position.offset, sliced.position.offset);
            let went = whole.position.offset.xz().length();
            let fullness = map.cover_at(0, 0).fullness();
            assert!(fullness < COVER_FULL);
            // The pace sets how far the walk goes; going round a trunk on
            // the way can only shorten it.
            let paced = open * pace(fullness);
            assert!(went <= paced + 1e-3 && went > 0.8 * paced, "{n} trees: {went} against the pace's share {paced} of {open}");
        }
        let map = create_test_map();
        flat_ground(&map, 6);
        let out = calculate_movement(input, 2000, &map, &nntree);
        let far: Qrz = Qrz { q: 0, r: 0, z: 1 } + map.convert(out.position.offset);
        let first: Qrz = Qrz { q: 0, r: 0, z: 1 } + map.convert(out.position.offset / 20.0 * 3.0);
        assert!((first.q, first.r) != (0, 0) && (first.q, first.r) != (far.q, far.r), "the walk should cross more than one tile");
        wooded(&map, first.q, first.r, 2);
        let out = calculate_movement(input, 2000, &map, &nntree);
        let here: Qrz = Qrz { q: 0, r: 0, z: 1 } + map.convert(out.position.offset);
        assert_eq!((here.q, here.r), (0, 0), "walked into a full tile");
        assert!(out.position.offset.xz().length() > 0.5, "stopped short of the face");
    }

    /// The same duration, split into 16 ms and 250 ms slices, reaches the
    /// same place: what lets the client and the server reproduce each other.
    #[test]
    fn result_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 6);
        let nntree = create_test_nntree();
        let input = walking(Heading::from_slot(7), true);

        let whole = calculate_movement(input, 1000, &map, &nntree);

        let mut sliced = input;
        let mut left = 1000;
        while left > 0 {
            let dt = left.min(16);
            let out = calculate_movement(sliced, dt, &map, &nntree);
            sliced.position = out.position;
            sliced.airtime = out.airtime;
            left -= dt;
        }

        assert!(whole.position.offset.abs_diff_eq(sliced.position.offset, 1e-3),
            "whole {:?} vs sliced {:?}", whole.position.offset, sliced.position.offset);
        assert_eq!(whole.airtime, sliced.airtime);
    }

    /// A jump reaches the same apex and lands on the same millisecond
    /// however the time is sliced.
    #[test]
    fn jump_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { airtime: Some(JUMP_DURATION_MS), ..walking(Heading::NORTH, false) };

        let whole = calculate_movement(input, 500, &map, &nntree);
        let mut sliced = input;
        for _ in 0..50 {
            let out = calculate_movement(sliced, 10, &map, &nntree);
            sliced.position = out.position;
            sliced.airtime = out.airtime;
        }
        assert!((whole.position.offset.y - sliced.position.offset.y).abs() < 1e-3);
        assert_eq!(whole.airtime, sliced.airtime);
    }

    /// A walk down a hillside two levels a tile drops faster than any
    /// constant fall; gravity gathers speed, so a jump off the top still
    /// lands on the way down rather than floating to the foot.
    #[test]
    fn a_fall_catches_a_slope() {
        let map = create_test_map();
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -2..=40 {
            for r in -3..=3 {
                map.insert(Qrz { q, r, z: -2 * q.max(0) }, ground);
            }
        }
        let nntree = create_test_nntree();
        let mut input = MovementInput { airtime: Some(JUMP_DURATION_MS), ..walking(Heading::from_degrees(90.0), true) };
        let mut landed_at = None;
        for step in 1..=40 {
            let out = calculate_movement(input, 50, &map, &nntree);
            input.position = out.position;
            input.airtime = out.airtime;
            if out.airtime.is_none() {
                landed_at = Some(step * 50);
                break;
            }
        }
        let landed_at = landed_at.expect("still airborne after two seconds down the slope");
        assert!(landed_at < 1500, "landed after {landed_at} ms");
        let where_: Qrz = map.convert(input.position.to_world(&map));
        assert!(where_.q >= 2, "landed on tile {where_:?}, not down the slope");
    }

    /// Standing still on a slope a level a tile, near the corner where two
    /// uphill neighbours meet and the surface stands two thirds of a level
    /// above the tile's own, the entity stays on the ground: no fall
    /// starts, and its height holds. Stepped as the client replays its
    /// inputs, at a fraction of the server's tick: a fall short enough to
    /// land inside one server tick shows on the client as several airborne.
    #[test]
    fn standing_on_a_slope_stays_grounded() {
        const CLIENT_TICK_MS: i16 = 16;
        let map = create_test_map();
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -4..=4 {
            for r in -4..=4 {
                map.insert(Qrz { q, r, z: q }, ground);
            }
        }
        let nntree = create_test_nntree();
        // The corner is the centroid of the three cells meeting there.
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 0 });
        let corner = (centre + map.convert(Qrz { q: 1, r: 0, z: 0 }) + map.convert(Qrz { q: 1, r: -1, z: 0 })) / 3.0;
        let toward = (corner - centre) * 0.85;
        let start = Position::new(Qrz { q: 0, r: 0, z: 1 }, Vec3::new(toward.x, 0.0, toward.z));
        let mut input = MovementInput { position: start, ..walking(Heading::NORTH, false) };
        let settled = calculate_movement(input, CLIENT_TICK_MS, &map, &nntree);
        input.position = settled.position;
        input.airtime = settled.airtime;
        assert_eq!(settled.airtime, None, "on the ground once the feet find the surface");
        for tick in 0..60 {
            let out = calculate_movement(input, CLIENT_TICK_MS, &map, &nntree);
            assert_eq!(out.airtime, None, "fell at tick {tick}");
            assert!((out.position.offset.y - settled.position.offset.y).abs() < 1e-5, "moved at tick {tick}: {} from {}", out.position.offset.y, settled.position.offset.y);
            input.position = out.position;
            input.airtime = out.airtime;
        }
    }

    /// The physics reads nothing of where its tile is: the same ground
    /// laid around a tile at the origin and around one millions of units
    /// out, walked with the same inputs — across faces, along a refused
    /// face, off a ledge and down to a landing — gives the same offsets,
    /// bit for bit. A world vector that far out keeps only quarter-unit
    /// steps, less than a tick's walk; the tile stands five levels up, so
    /// the heights are tested the same way.
    #[test]
    fn the_walk_is_the_same_however_far_out_the_tile_is() {
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        let lay = |map: &Map, at: Qrz| {
            for q in -4..=4 {
                for r in -4..=4 {
                    // A slope to the east, a wall of two levels along r = 2.
                    let z = if r == 2 { 3 } else { q.max(0) };
                    map.insert(Qrz { q: at.q + q, r: at.r + r, z: at.z + z }, ground);
                }
            }
        };
        let run = |at: Qrz| {
            let map = create_test_map();
            lay(&map, at);
            let nntree = create_test_nntree();
            let mut input = MovementInput {
                position: Position::new(Qrz { z: at.z + 1, ..at }, Vec3::new(0.3, 0.0, -0.2)),
                turn: 1,
                since_step_ms: 0,
                ..walking(Heading::from_degrees(150.0), true)
            };
            let mut trace = Vec::new();
            for tick in 0..120 {
                if tick == 60 { input.airtime = Some(JUMP_DURATION_MS); }
                let out = calculate_movement(input, 16, &map, &nntree);
                input.position = out.position;
                input.heading = out.heading;
                input.since_step_ms = out.since_step_ms;
                input.airtime = out.airtime;
                trace.push((out.position.offset, out.airtime, out.heading));
            }
            trace
        };
        let near = run(Qrz { q: 0, r: 0, z: 0 });
        let far = run(Qrz { q: -1_600_000, r: 2_400_000, z: 5 });
        assert!(near.iter().any(|(o, _, _)| o.xz().length() > 2.0), "the walk crossed faces: {:?}", near.last());
        assert!(near.iter().any(|(_, air, _)| air.is_some()) && near.last().unwrap().1.is_none(), "it jumped and landed");
        for (tick, (n, f)) in near.iter().zip(&far).enumerate() {
            assert_eq!(n, f, "tick {tick}: near {n:?}, far {f:?}");
        }
    }

    /// A rise of two levels is a cliff: the step toward it is refused and
    /// the entity keeps its place.
    #[test]
    fn cliffs_block_and_steps_do_not() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        let nntree = create_test_nntree();
        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });

        map.insert(Qrz { q: 1, r: 0, z: 1 }, ground);
        let step = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(step.position.offset.x > 0.5, "a one-level step is walked: {:?}", step.position.offset);

        map.remove(Qrz { q: 1, r: 0, z: 1 });
        map.insert(Qrz { q: 1, r: 0, z: 2 }, ground);
        let cliff = calculate_movement(walking(east, true), 250, &map, &nntree);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let where_it_stands: Qrz = map.convert(centre + cliff.position.offset);
        assert_eq!((where_it_stands.q, where_it_stands.r), (0, 0), "held at the cliff: {:?}", cliff.position.offset);
    }

    /// A step of water is waded and deeper water blocks: the next tile
    /// under one step of water is entered, under two it is not, and a dry
    /// tile beside a flooded one is unaffected.
    #[test]
    fn deep_water_blocks_and_shallow_water_is_waded() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        map.set_water(1, 0, Some(1));
        map.set_water(2, 0, Some(2));
        assert!(!is_deep_water(&map, 1, 0), "a step of water is waded");
        assert!(is_deep_water(&map, 2, 0), "two steps of water block");
        assert!(!is_deep_water(&map, -1, 0), "dry ground is dry");

        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let wade = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(wade.position.offset.x > 0.5, "walks into a step of water");

        map.set_water(1, 0, Some(2));
        let blocked = calculate_movement(walking(east, true), 250, &map, &nntree);
        assert!(blocked.position.offset.x < wade.position.offset.x, "held back by deep water");
    }

    #[test]
    fn feet_stay_on_the_surface() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let out = calculate_movement(walking(Heading::from_slot(9), true), 300, &map, &nntree);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });
        let world = centre + out.position.offset;
        let here: Qrz = map.convert(world);
        let (floor, _) = map.get_by_qr(here.q, here.r).unwrap();
        assert!((world.y - surface_y(world.xz(), floor, &map)).abs() < 1e-5);
    }

    fn cliff(map: &Map, q: i32, r: i32) {
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        map.insert(Qrz { q, r, z: 2 }, ground);
    }

    /// A wall met at an angle is slid along: the heading's component along
    /// the face is kept and the rest is lost, so the run along the wall grows
    /// with the angle of incidence from nothing head-on, and the wall is
    /// never entered.
    #[test]
    fn a_wall_is_slid_along_by_the_angle_of_incidence() {
        let map = create_test_map();
        flat_ground(&map, 3);
        cliff(&map, 1, 0);
        let nntree = create_test_nntree();
        let here = Qrz { q: 0, r: 0, z: 1 };
        let wall = Qrz { q: 1, r: 0, z: 1 };
        let (normal, mid) = map.face(here, wall);
        let along = normal.perp();
        let centre: Vec3 = map.convert(here);
        let start = mid - normal * 0.1 - centre.xz();

        let run = |slot: u8| {
            let position = Position::new(here, Vec3::new(start.x, 0.0, start.y));
            let input = MovementInput { position, ..walking(Heading::from_slot(slot), true) };
            let out = calculate_movement(input, 50, &map, &nntree);
            let ended: Qrz = map.convert(centre + out.position.offset);
            assert_ne!((ended.q, ended.r), (wall.q, wall.r), "slot {slot} entered the wall");
            let moved = out.position.offset.xz() - start;
            assert!(moved.dot(normal) <= 0.1 + 1e-5, "slot {slot} passed the face");
            moved.dot(along)
        };

        // The face's normal is the slot-8 bearing; slots 3 to 13 press toward it.
        let runs: Vec<f32> = (3..=13).map(run).collect();
        assert!(runs[5].abs() < 1e-4, "head-on stands: {}", runs[5]);
        for pair in runs.windows(2) {
            assert!(pair[0] < pair[1], "the run along the face grows with the incidence: {runs:?}");
        }
    }

    /// A walk into a wall at an angle, along it and round its end reaches
    /// the same place in 16 ms and 125 ms slices, and walks on past it.
    #[test]
    fn a_slide_is_independent_of_partition() {
        let map = create_test_map();
        flat_ground(&map, 6);
        cliff(&map, 1, 0);
        let nntree = create_test_nntree();
        let input = walking(Heading::from_slot(9), true);

        let whole = calculate_movement(input, 1000, &map, &nntree);
        let mut sliced = input;
        let mut left = 1000;
        while left > 0 {
            let dt = left.min(16);
            let out = calculate_movement(sliced, dt, &map, &nntree);
            sliced.position = out.position;
            sliced.airtime = out.airtime;
            left -= dt;
        }
        assert!(whole.position.offset.abs_diff_eq(sliced.position.offset, 1e-3),
            "whole {:?} vs sliced {:?}", whole.position.offset, sliced.position.offset);

        let centre: Vec3 = map.convert(input.position.tile);
        let ended: Qrz = map.convert(centre + whole.position.offset);
        assert!(ended.q >= 1 && (ended.q, ended.r) != (1, 0), "walked on past the wall: {ended:?}");
    }

    /// Two walls meeting at a corner hold an entity pushed into it: each
    /// face's slide runs into the other, and the walk ends without progress.
    #[test]
    fn a_concave_corner_holds() {
        let map = create_test_map();
        flat_ground(&map, 3);
        cliff(&map, 1, 0);
        cliff(&map, 0, 1);
        let nntree = create_test_nntree();
        let into_corner = Heading::from_slot(10);
        let centre: Vec3 = map.convert(Qrz { q: 0, r: 0, z: 1 });

        let first = calculate_movement(walking(into_corner, true), 250, &map, &nntree);
        let input = MovementInput { position: first.position, ..walking(into_corner, true) };
        let more = calculate_movement(input, 250, &map, &nntree);

        let ended: Qrz = map.convert(centre + more.position.offset);
        assert_eq!((ended.q, ended.r), (0, 0), "held in the corner: {:?}", more.position.offset);
        assert!(more.position.offset.xz().abs_diff_eq(first.position.offset.xz(), 1e-4), "no progress once held");
    }
}
