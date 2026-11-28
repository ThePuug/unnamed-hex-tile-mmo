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
use common::grid::RISE;
use qrz::{Convert, Qrz};

use crate::{
    components::{
        entity_type::{decorator::*, EntityType},
        heading::Heading,
        position::Position,
        Loc,
    },
    components::status::Status,
    plugins::nntree::NNTree,
    resources::map::Map,
};

/// Gravity: how fast a fall gathers speed, in world units per millisecond
/// squared. A fall must outrun a walk down any slope or an entity that
/// leaves the ground on a hillside floats to its foot, and a slope's
/// descent has no bound, so no constant fall speed does. Stated in levels:
/// a slope is so many levels a tile, and a fall through a level takes the
/// same time however tall a level stands.
pub const GRAVITY: f32 = 2.5e-5 * RISE;

/// The fastest a fall drops, in world units per millisecond: gravity
/// gathers speed to it and no further. Stated in levels, as [`GRAVITY`]
/// is, and well past a walk down any slope a tile may be entered from.
pub const TERMINAL_SPEED: f32 = 0.04 * RISE;

/// A jump's ascent: its speed in world units per millisecond, held for
/// `JUMP_DURATION_MS`, then the fall. A jump climbs one level more than a
/// step does: a ledge two levels up, never three.
pub const JUMP_ASCENT: f32 = 0.025;

/// Jump duration in milliseconds
pub const JUMP_DURATION_MS: i16 = 125;

/// Longest sub-step in milliseconds. Bounds how far a step can carry
/// before blocking and landing are checked again.
pub const PHYSICS_TIMESTEP_MS: i16 = 125;

/// Base movement speed in world units per millisecond
pub const MOVEMENT_SPEED: f32 = 0.0075;

/// The share of its speed an entity keeps backing away: less than it runs,
/// so fleeing what it faces costs the turn to run from it.
pub const BACK_PACE: f32 = 0.35;

/// Keeps the burden effect on every entity whose bag weighs past the limit:
/// on the server for every player, on the client for its own, whose bag it
/// holds. A remote entity's comes with its intent.
pub fn update_burden(
    mut commands: Commands,
    mut bags: Query<(Entity, &crate::components::equipment::Inventory, Option<&mut Status>), Changed<crate::components::equipment::Inventory>>,
) {
    for (ent, bag, status) in &mut bags {
        let burden = bag.is_burdened();
        match status {
            Some(mut status) => { if status.burden != burden { status.burden = burden; } }
            None if burden => { commands.entity(ent).insert(Status { burden, ..default() }); }
            None => {}
        }
    }
}

/// Milliseconds of input time between steps of a held turn key at whole
/// pace, and the least time between any two steps: a tap after a rest turns
/// at once, and a hammered key turns no faster than a held one. A turn pace
/// scales it ([`turn_interval_ms`]). Longer than a tap is held, or a tap
/// turns two steps.
pub const TURN_REPEAT_MS: u16 = 150;

/// Milliseconds between steps of a held turn at `pace`, the share of its
/// turn an actor's status effects leave it ([`Status::turn_pace`]): a slowed
/// actor turns as much slower as it walks. None at no pace, a root or a
/// hold, which leave it no turning. Never longer than [`TURN_RESTED_MS`].
pub fn turn_interval_ms(pace: f32) -> Option<u16> {
    (pace > 0.0).then(|| (TURN_REPEAT_MS as f32 / pace).round().clamp(1.0, TURN_RESTED_MS as f32) as u16)
}

/// The turn clock of an actor long at rest, and the most it counts to: past
/// every interval, so a tap after a rest steps at once at any pace, though
/// the slow landed during the rest. The longest `dt` the physics takes.
pub const TURN_RESTED_MS: u16 = i16::MAX as u16;

/// Maximum entity count per tile before considering it solid
pub const MAX_ENTITIES_PER_TILE: usize = 7;

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
/// the integral of the speed gravity has built up to [`TERMINAL_SPEED`], so
/// a fall sliced any way drops the same.
pub fn fall(fallen_ms: f32, dt: f32) -> f32 {
    let gathering = (TERMINAL_SPEED / GRAVITY - fallen_ms).clamp(0.0, dt);
    GRAVITY * (fallen_ms * gathering + gathering * gathering / 2.0) + TERMINAL_SPEED * (dt - gathering)
}

/// How long a fall `fallen_ms` old takes to drop `height` more, in
/// milliseconds.
pub fn fall_time(height: f32, fallen_ms: f32) -> f32 {
    let height = height.max(0.0);
    let gathering = (TERMINAL_SPEED / GRAVITY - fallen_ms).max(0.0);
    let gathered = fall(fallen_ms, gathering);
    if height > gathered {
        return gathering + (height - gathered) / TERMINAL_SPEED;
    }
    let v = GRAVITY * fallen_ms;
    ((v * v + 2.0 * GRAVITY * height).sqrt() - v) / GRAVITY
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
    let from = |g: common::grid::Ground| common::grid::Ground { z: g.z - tile.z, ..g };
    // The floor a walker stands on may be one the map no longer holds at
    // that level; it is then walked at the level itself.
    let own = map.ground_at(floor.q, floor.r).filter(|g| g.z == floor.z).unwrap_or(common::grid::Ground::level(floor.z));
    crate::surface::surface_y(xz, (floor.q, floor.r), from(own), floor_centre, |q, r| map.ground_at(q, r).map(from))
}

/// A swimmer's pace, as a share of its pace on land: water deep enough to
/// float it is a crossing, never a road.
pub const SWIM_PACE: f32 = 0.4;

/// The water's surface over tile `(q, r)` as a height over the level of
/// `tile`, where it is drawn (`surface::water_z`); None where it is dry.
pub fn water_y_from(tile: Qrz, q: i32, r: i32, map: &Map) -> Option<f32> {
    map.water_at(q, r).map(|w| (crate::surface::water_z(w) + 1.0 - tile.z as f32) * map.rise())
}

/// The height an actor whose body floats at `draft` is held at over the
/// level of `tile`, at `xz` from that tile's centre, over `floor`: the
/// surface, or where the water stands more than its draft over it, the
/// water less its draft, its waterline on the water's surface. Water is a
/// floor to a body it floats, and walked through by one it does not.
pub fn support_y_from(tile: Qrz, xz: Vec2, floor: Qrz, draft: f32, map: &Map) -> f32 {
    let ground = surface_y_from(tile, xz, floor, map);
    water_y_from(tile, floor.q, floor.r, map).map_or(ground, |water| ground.max(water - draft))
}

/// The height [`support_y_from`] holds an actor at, at the world point
/// `world_xz` over `floor`, as [`surface_y`] is to [`surface_y_from`].
pub fn support_y(world_xz: Vec2, floor: Qrz, draft: f32, map: &Map) -> f32 {
    let floor_centre: Vec3 = map.convert(floor);
    floor_centre.y + support_y_from(floor, world_xz - floor_centre.xz(), floor, draft, map)
}

/// Whether the water over `floor` stands deeper than `draft` over its
/// level, so a body that floats at `draft` swims there.
fn floats(floor: Qrz, draft: f32, map: &Map) -> bool {
    water_y_from(floor, floor.q, floor.r, map).is_some_and(|water| water - draft > map.rise())
}

/// Whether an actor at `position`, whose body floats at `draft`, is held up
/// by the water there rather than the ground: it swims, and in the air it
/// is over water that will take it back.
pub fn swimming(position: Position, draft: f32, map: &Map) -> bool {
    let here: Qrz = position.tile + map.convert(position.offset);
    map.get_by_qr(here.q, here.r).is_some_and(|(floor, _)| {
        let xz = position.offset.xz();
        support_y_from(position.tile, xz, floor, draft, map) > surface_y_from(position.tile, xz, floor, map)
    })
}

/// The level an actor whose body floats at `draft` stands at over `floor`,
/// for what it may enter and where it falls: the floor's own, or where the
/// water floats it, the highest level the water's surface covers, so a
/// bank one level above that is climbed out onto and one two above is
/// jumped.
fn standing_level(floor: Qrz, draft: f32, map: &Map) -> i32 {
    match map.water_at(floor.q, floor.r) {
        Some(w) if floats(floor, draft, map) => floor.z.max(crate::surface::water_z(w).floor() as i32),
        _ => floor.z,
    }
}

/// A walker's pace through the water over `floor`, as a share of its pace
/// on land, for a body that floats at `draft`: it eases from whole on dry
/// ground as the water deepens over the tile's level, to [`SWIM_PACE`]
/// where the body floats. Read for the tile, as its fullness is.
pub fn wading(floor: Qrz, draft: f32, map: &Map) -> f32 {
    let depth = water_y_from(floor, floor.q, floor.r, map).map_or(0.0, |water| water - map.rise());
    1.0 - (1.0 - SWIM_PACE) * (depth / draft).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
pub struct MovementInput {
    pub position: Position,
    /// The facing, and the direction a forward move travels along.
    pub heading: Heading,
    pub moving: bool,
    /// The move travels opposite the heading, at [`BACK_PACE`].
    pub back: bool,
    /// Bearings the heading steps per repeat: -1 counter-clockwise, 0, 1
    /// clockwise.
    pub turn: i8,
    /// Milliseconds since the heading last stepped, at most
    /// [`TURN_RESTED_MS`]: it counts on at any pace, a root's included, so
    /// the first turn after a rest at least the interval at its pace
    /// ([`turn_interval_ms`]) steps at once.
    pub since_step_ms: u16,
    /// Some(positive) = ascending, Some(negative or zero) = falling, None = grounded
    pub airtime: Option<i16>,
    /// Its own speed, in world units per millisecond, before its pace
    pub movement_speed: f32,
    /// The share of its speed its status effects leave it
    /// ([`Status::pace_of`]); it scales the walk
    pub pace: f32,
    /// The share of its turn its status effects leave it
    /// ([`Status::turn_pace_of`]); it scales the turn clock
    pub turn_pace: f32,
    /// Whether the walker is a player's pill, which goes round the
    /// footprints of what stands in a tile; anything else walks through
    /// them, and only a tile's own rules stop it.
    pub collides: bool,
    /// How deep water must stand to float it, in world units
    /// ([`ActorIdentity::draft`](crate::components::entity_type::actor::ActorIdentity::draft)):
    /// shallower it wades, deeper it swims on the surface. Infinite for a
    /// body that never floats.
    pub draft: f32,
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
/// is measured from, and whose body floats at `draft`. Refused by a rise of
/// more than one level unless airborne at or above its standing height,
/// each tile's level the one it stands at there ([`standing_level`]), by a
/// solid decorator with no floor, by a tile at its entity capacity, and by
/// trees in every slot. Water never refuses a step: it is waded or swum.
#[allow(clippy::too_many_arguments)]
pub fn is_tile_blocked(
    tile: Qrz,
    here_floor: Option<Qrz>,
    next: Qrz,
    y: f32,
    airtime: Option<i16>,
    draft: f32,
    map: &Map,
    nntree: &NNTree,
) -> bool {
    let next_floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);
    let level = |floor: Qrz| standing_level(floor, draft, map);

    let cliff = match (here_floor.map(level), next_floor.map(level)) {
        (Some(here), Some(there)) if there - here > 1 => {
            let standing = (there + 1 - tile.z) as f32 * map.rise();
            airtime.is_none() || y < standing
        }
        _ => false,
    };

    let solid = match map.get(next) {
        Some(EntityType::Decorator(Decorator { is_solid, .. })) => is_solid,
        _ => nntree.locate_all_at_point(&Loc::new(next)).count() >= MAX_ENTITIES_PER_TILE,
    };

    cliff || (solid && next_floor.is_none()) || is_full_cover(map, next.q, next.r)
}

/// How far short of a refused face a walk stops, in world units: a hair, so
/// the position still converts to its own tile and never the refused one. A
/// distance from the face, not per step, so the stop is the same however
/// the time is sliced.
const FACE_MARGIN: f32 = 0.01;

/// Faces one walk may meet. A sub-step crosses at most a tile or two,
/// mounted, so a few crossings and slides cover it; the cap only ends the bounce between the
/// two walls of a concave corner, which makes no progress.
const FACES_PER_WALK: usize = 16;

/// Displacement of a walk of `reach` world units along the unit `heading`
/// from `from`, a ground-plane point given from the centre of `tile`, in
/// `here` whose floor is `floor`. The walk crosses the faces
/// [`is_tile_blocked`] allows and slides along the ones it refuses: against
/// a face only the heading's component along it is kept, so a graze keeps
/// most of its speed, a head-on push stands, and past the face's end the
/// heading resumes. Each tile is crossed at its own [`pace`] and its water's
/// ([`wading`]), read at the face, so a slow tile spends the reach faster
/// over the part of the walk inside it and the same however the walk is
/// sliced. The faces are read
/// with the tiles taken relative to `tile`, so the geometry is exact
/// however far out the tile is; only the lookups name the tiles
/// themselves. `y` is the feet's height over the level of `tile`.
///
/// A grounded walk leaves the ground where it first passes into a tile
/// more than a level below, as [`is_tile_blocked`] refuses one more than a
/// level above, each the level it stands at there: the second value is
/// then the reach spent getting there and the height it was held at
/// there, the edge. The walk carries on past it.
#[allow(clippy::too_many_arguments)]
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
    draft: f32,
    map: &Map,
    nntree: &NNTree,
) -> (Vec2, Option<(f32, f32)>) {
    let footprints = |here: Qrz| if collides { footprints(tile, here, map) } else { Vec::new() };
    let drops = |from: Option<Qrz>, to: Option<Qrz>| match (from, to) {
        (Some(from), Some(to)) => standing_level(from, draft, map) - standing_level(to, draft, map) > 1,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let pace_of = |floor: Option<Qrz>, q: i32, r: i32| {
        pace(map.cover_at(q, r).fullness()) * floor.map_or(1.0, |floor| wading(floor, draft, map))
    };
    let mut leaves = None;
    let mut pos = from;
    let mut left = reach;
    let mut dir = heading;
    // Speed along `dir` as a fraction of full: the cosine of the incidence
    // while sliding, so a slide spends `left` faster than it covers ground.
    let mut rate = 1.0;
    let mut pace_here = pace_of(floor, here.q, here.r);
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
                if is_tile_blocked(tile, floor, at, y, airtime, draft, map, nntree) {
                    break;
                }
                // Where the round passes the face is not solved for, so a
                // round over a drop leaves the ground at the contact.
                let at_floor = map.get_by_qr(at.q, at.r).map(|(floor, _)| floor);
                if airtime.is_none() && leaves.is_none() && drops(floor, at_floor) {
                    leaves = floor.map(|floor| (reach - left, support_y_from(tile, pos, floor, draft, map)));
                }
                here = at;
                floor = at_floor;
                pace_here = pace_of(floor, at.q, at.r);
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
        if is_tile_blocked(tile, floor, next, y, airtime, draft, map, nntree) {
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
            let next_floor = map.get_by_qr(next.q, next.r).map(|(floor, _)| floor);
            if airtime.is_none() && leaves.is_none() && drops(floor, next_floor) {
                leaves = floor.map(|floor| (reach - left, support_y_from(tile, pos, floor, draft, map)));
            }
            here = next;
            floor = next_floor;
            pace_here = pace_of(floor, next.q, next.r);
            about = footprints(here);
            dir = heading;
            rate = 1.0;
        }
    }
    (pos - from, leaves)
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
            let sway = common::sway(q, r, site);
            let form = common::cover::tree_form(content, sway.variation)?;
            let scale = common::cover::tree_scale(form.height, sway.growth as f32);
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
/// drawn and the walker's own half-width more; and each den's circle
/// taller than the waist that a walker in `about` may meet.
fn footprints(tile: Qrz, about: Qrz, map: &Map) -> Vec<(Vec2, f32)> {
    let mut out = Vec::new();
    for solid in map.solids_at(about.q, about.r).into_iter().filter(|s| s.height > WAIST) {
        let anchor = Qrz { q: solid.anchor.0, r: solid.anchor.1, z: 0 };
        let home = map.convert(anchor - Qrz { q: tile.q, r: tile.r, z: 0 }).xz();
        out.push((home + solid.offset, solid.radius + WALKER_RADIUS));
    }
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
/// None where the spot's tile may not be entered from the player's on
/// foot, as no gather is begun swimming, or a footprint other than what it
/// works stands in the way there.
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
        common::gathering::Activity::Pickup | common::gathering::Activity::Sleep | common::gathering::Activity::Haul => 0.0,
        common::gathering::Activity::Work(_) => solid_extent(cover, q, r, k).map_or(0.0, |(radius, _)| radius),
    };
    let stand = target - heading.to_world_dir() * (common::gathering::reach(activity) + edge);

    let here: Qrz = tile + map.convert(from);
    let rel = map.convert(Vec3::new(stand.x, 0.0, stand.y));
    let at = Qrz { q: tile.q + rel.q, r: tile.r + rel.r, z: here.z };
    if (at.q, at.r) != (here.q, here.r) {
        let floor = map.get_by_qr(here.q, here.r).map(|(floor, _)| floor);
        if is_tile_blocked(tile, floor, at, from.y, None, f32::INFINITY, map, nntree) {
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
/// there, and held on the tile it reaches.
pub fn stepped(position: Position, stand: Vec2, map: &Map) -> Position {
    let here: Qrz = position.tile + map.convert(Vec3::new(stand.x, 0.0, stand.y));
    let y = map.get_by_qr(here.q, here.r).map_or(position.offset.y, |(floor, _)| surface_y_from(position.tile, stand, floor, map));
    let mut stepped = Position { tile: position.tile, offset: Vec3::new(stand.x, y, stand.y) };
    stepped.rebase(stepped.reached(map), map);
    stepped
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
    let interval = turn_interval_ms(input.turn_pace);

    while dt0 > 0 {
        let mut dt = dt0.min(PHYSICS_TIMESTEP_MS);
        if let (true, Some(interval)) = (input.turn != 0, interval) {
            if since_step_ms >= interval {
                heading = heading.turned(input.turn as i32);
                since_step_ms = 0;
            }
            dt = dt.min((interval - since_step_ms) as i16);
        }
        dt0 -= dt;
        let (dir, pace) = if input.back { (heading.reversed(), BACK_PACE) } else { (heading, 1.0) };
        let dir = dir.to_world_dir();

        let here: Qrz = tile + map.convert(offset);
        let floor = map.get_by_qr(here.q, here.r).map(|(floor, _)| floor);

        // Over nothing, or more than a level above what holds it up, a
        // grounded entity starts to fall. The ground is the surface, never
        // the tile's level: on a slope the surface stands well above it in
        // the uphill part of a tile, and a level test read there falls
        // every few ticks.
        let support = |floor: Qrz, offset: Vec3| support_y_from(tile, offset.xz(), floor, input.draft, map);
        if airtime.is_none() && floor.map_or(true, |floor| offset.y > support(floor, offset) + map.rise()) {
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
                match floor.map(|floor| support(floor, offset)) {
                    Some(ground) if offset.y + dy <= ground => {
                        offset.y = ground;
                        airtime = None;
                    }
                    _ => offset.y += dy,
                }
            }
        }

        if input.moving {
            let per_ms = input.movement_speed * input.pace * pace;
            let step = |dt: i16| walk(
                tile, offset.xz(), dir, per_ms * dt as f32,
                here, floor, offset.y, airtime, input.collides, input.draft, map, nntree,
            );
            let (mut moved, leaves) = step(dt);
            if let Some((spent, edge)) = leaves {
                // Split at the first whole millisecond past the edge, so the
                // fall starts on the same one however the time is sliced.
                let ms = ((spent / per_ms).ceil() as i16).max(1);
                if ms < dt {
                    dt0 += dt - ms;
                    dt = ms;
                    moved = step(dt).0;
                }
                offset.y = edge;
                airtime = Some(0);
            }
            offset.x += moved.x;
            offset.z += moved.y;
        }

        // After the apex and edge splits, which may have shortened the sub-step.
        since_step_ms = since_step_ms.saturating_add(dt as u16).min(TURN_RESTED_MS);

        let here: Qrz = tile + map.convert(offset);
        if let Some((floor, _)) = map.get_by_qr(here.q, here.r) {
            let ground = support(floor, offset);
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

    /// The game's grid: the surface the feet are held to reads its rise.
    fn create_test_map() -> Map {
        Map::new(qrz::Map::new(common::grid::HEX_RADIUS, RISE))
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
            since_step_ms: TURN_RESTED_MS,
            airtime: None,
            movement_speed: MOVEMENT_SPEED,
            pace: 1.0,
            turn_pace: 1.0,
            collides: true,
            draft: f32::INFINITY,
        }
    }

    /// A burden slows a walk and never stops it or turns it.
    #[test]
    fn a_burden_slows_a_walk() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let free = calculate_movement(walking(Heading::NORTH, true), 200, &map, &nntree);
        let slow = MovementInput { pace: Status { burden: true, ..default() }.pace(), ..walking(Heading::NORTH, true) };
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
        let sway = common::sway(north.0, north.1, 0);
        let form = common::cover::tree_form(cover.content(common::SITE_SLOTS[0][0]), sway.variation).unwrap();
        form.trunk * common::cover::tree_scale(form.height, sway.growth as f32) + WALKER_RADIUS
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

    /// A den's circle taller than the waist stands a walk into it at the
    /// walker's keep, held by another tile's den in that tile's frame; one
    /// lower is stepped over.
    #[test]
    fn a_dens_tall_circle_stands_a_walk_and_a_low_one_is_stepped_over() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let anchor = (1, -2);
        let home = map.convert(Qrz { q: anchor.0, r: anchor.1, z: 0 }).xz();
        let centre = Vec2::new(0.0, -3.0);
        map.set_solids(anchor, &[(centre - home, 0.8, 2.0)]);
        let out = calculate_movement(walking_from(Vec2::ZERO), 2000, &map, &nntree);
        let d = out.position.offset.xz().distance(centre);
        assert!((d - (0.8 + WALKER_RADIUS)).abs() < 1e-3, "stood {d} from the circle's centre");

        map.set_solids(anchor, &[(centre - home, 0.8, WAIST * 0.5)]);
        let out = calculate_movement(walking_from(Vec2::ZERO), 2000, &map, &nntree);
        assert!(out.position.offset.z < centre.y - 0.8, "a low circle stood the walker at {:?}", out.position.offset);
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

    /// A slowed actor turns as much slower as it walks: a held key steps
    /// once per repeat at its pace, the first step still at once.
    #[test]
    fn a_slowed_turn_steps_at_its_pace() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let pace = 0.5;
        let interval = turn_interval_ms(pace).unwrap();
        assert!(interval > TURN_REPEAT_MS, "slower than at whole pace");
        let held = MovementInput { turn: 1, turn_pace: pace, ..walking(Heading::NORTH, false) };
        let short = calculate_movement(held, interval as i16, &map, &nntree);
        assert_eq!(short.heading, Heading::NORTH.turned(1), "one step, the rested one, before a whole interval passes");
        let long = calculate_movement(held, interval as i16 * 2 + 1, &map, &nntree);
        assert_eq!(long.heading, Heading::NORTH.turned(3), "then once per interval at its pace");
        let whole = calculate_movement(MovementInput { turn_pace: 1.0, ..held }, interval as i16 * 2 + 1, &map, &nntree);
        assert!(whole.heading.turn_toward(Heading::NORTH).1 > long.heading.turn_toward(Heading::NORTH).1, "at whole pace it turns further");
    }

    /// A root holds the heading as it holds the walk, and the clock keeps
    /// counting, so the first turn once it lifts steps at once.
    #[test]
    fn a_rooted_actor_does_not_turn() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let rooted = Status { root: Some(crate::components::status::Timed { pace: 0.0, remaining: 1.0 }), ..default() }.pace();
        assert_eq!(turn_interval_ms(rooted), None);
        let input = MovementInput { turn: 1, since_step_ms: 0, pace: rooted, turn_pace: rooted, ..walking(Heading::NORTH, true) };
        let out = calculate_movement(input, 500, &map, &nntree);
        assert_eq!(out.heading, Heading::NORTH);
        assert_eq!(out.position.offset.xz(), Vec2::ZERO);
        let freed = calculate_movement(MovementInput { pace: 1.0, turn_pace: 1.0, ..carried(input, &out) }, 1, &map, &nntree);
        assert_eq!(freed.heading, Heading::NORTH.turned(1));
    }

    /// Under a slow the bend still lands on the same millisecond however
    /// the time is sliced.
    #[test]
    fn a_slowed_turn_is_partition_independent() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let input = MovementInput { turn: 1, since_step_ms: 30, pace: 0.7, turn_pace: 0.7, ..walking(Heading::from_slot(6), true) };
        let whole = calculate_movement(input, 500, &map, &nntree);
        assert_ne!(whole.heading, input.heading);
        for dt in [7, 50, 125] {
            let (mut sliced, mut out, mut left) = (input, whole, 500);
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
    fn walking_back_travels_opposite_the_heading_slower_and_keeps_it() {
        let map = create_test_map();
        flat_ground(&map, 3);
        let nntree = create_test_nntree();
        let east = Heading::from_degrees(90.0);
        let forward = calculate_movement(walking(east, true), 100, &map, &nntree);
        let back = calculate_movement(MovementInput { back: true, ..walking(east, true) }, 100, &map, &nntree);
        assert!(forward.position.offset.xz().dot(back.position.offset.xz()) < 0.0, "{:?} vs {:?}", forward.position.offset, back.position.offset);
        assert!(back.position.offset.xz().length() < forward.position.offset.xz().length(), "backing away is slower than running");
        assert_eq!(back.heading, east);
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

    /// A walk through a wood of one tree a tile covers what its pace gives,
    /// the same however the time is sliced; two trees make a wall.
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
        let map = create_test_map();
        flat_ground(&map, 6);
        for q in -6..=6 {
            for r in -6..=6 {
                wooded(&map, q, r, 1);
            }
        }
        let whole = calculate_movement(input, 1000, &map, &nntree);
        let mut sliced = input;
        for _ in 0..40 {
            sliced = carried(sliced, &calculate_movement(sliced, 25, &map, &nntree));
        }
        assert!(whole.position.offset.abs_diff_eq(sliced.position.offset, 1e-3), "whole {:?} vs sliced {:?}", whole.position.offset, sliced.position.offset);
        let went = whole.position.offset.xz().length();
        let fullness = map.cover_at(0, 0).fullness();
        assert!(fullness < COVER_FULL);
        // The pace sets how far the walk goes; going round a trunk on
        // the way can only shorten it.
        let paced = open * pace(fullness);
        assert!(went <= paced + 1e-3 && went > 0.8 * paced, "{went} against the pace's share {paced} of {open}");

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

    /// A walk off a ledge four levels high falls: the feet leave the
    /// ground at the edge and reach the foot only after the fall's time,
    /// never at the step that crosses the face.
    #[test]
    fn a_walk_off_a_ledge_falls() {
        let (map, nntree, input) = ledge();
        let mut input = input;
        let (mut airborne, mut y) = (0, input.position.offset.y);
        for tick in 0..80 {
            let out = calculate_movement(input, 16, &map, &nntree);
            if out.airtime.is_some() {
                airborne += 1;
            }
            assert!(y - out.position.offset.y < RISE, "dropped more than a level in one tick at tick {tick}");
            y = out.position.offset.y;
            input.position = out.position;
            input.airtime = out.airtime;
        }
        assert!(airborne > 10, "stepped down four levels in {airborne} ticks of air");
        assert_eq!(input.airtime, None, "still falling: {:?}", input.position);
        assert!((input.position.offset.y + 4.0 * RISE).abs() < 1e-3, "never reached the foot: {:?}", input.position);
    }

    /// The walk leaves the ledge, and the fall drops, at the same moments
    /// however the time is sliced.
    #[test]
    fn a_fall_off_a_ledge_is_independent_of_partition() {
        let (map, nntree, start) = ledge();
        let at = |slice: i16| {
            let mut input = start;
            for _ in 0..400 / slice {
                let out = calculate_movement(input, slice, &map, &nntree);
                input.position = out.position;
                input.airtime = out.airtime;
            }
            (input.position.offset, input.airtime)
        };
        let whole = at(400);
        assert!(whole.1.is_some_and(|air| air < 0), "mid-fall at 400 ms: {whole:?}");
        for slice in [8, 16, 25, 50, 100] {
            let sliced = at(slice);
            assert!((whole.0 - sliced.0).length() < 1e-3, "{slice} ms slices: {sliced:?}, whole: {whole:?}");
            assert_eq!(whole.1, sliced.1, "{slice} ms slices");
        }
    }

    /// Ground four levels high to the west of q = 1 and low past it, and a
    /// walk east from the high ground's centre.
    fn ledge() -> (Map, NNTree, MovementInput) {
        let map = create_test_map();
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        for q in -3..=6 {
            for r in -3..=3 {
                map.insert(Qrz { q, r, z: if q <= 0 { 4 } else { 0 } }, ground);
            }
        }
        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let input = MovementInput { position: Position::at_tile(Qrz { q: 0, r: 0, z: 5 }), ..walking(east, true) };
        (map, create_test_nntree(), input)
    }

    /// A fall gathers speed and then holds it: each equal slice drops more
    /// than the last until the speed is reached, then the same; however it
    /// is sliced it drops the same; and `fall_time` answers `fall`.
    #[test]
    fn a_fall_gathers_speed_to_its_terminal() {
        let slices: Vec<f32> = (0..60).map(|k| fall(k as f32 * 50.0, 50.0)).collect();
        let reached = slices.iter().position(|&d| (d - TERMINAL_SPEED * 50.0).abs() < 1e-5).expect("never reached its terminal speed");
        assert!(reached > 0 && slices[..reached].windows(2).all(|w| w[1] > w[0]), "gathered: {slices:?}");
        assert!(slices[reached..].iter().all(|&d| (d - TERMINAL_SPEED * 50.0).abs() < 1e-5), "held: {slices:?}");
        for (age, t) in [(0.0, 3000.0), (500.0, 2000.0), (1000.0, 100.0), (5000.0, 700.0)] {
            let split = fall(age, t / 3.0) + fall(age + t / 3.0, 2.0 * t / 3.0);
            assert!((fall(age, t) - split).abs() < 1e-3, "age {age}, {t} ms: {} whole, {split} split", fall(age, t));
            assert!((fall_time(fall(age, t), age) - t).abs() < 1e-2, "age {age}, {t} ms: fall_time {}", fall_time(fall(age, t), age));
        }
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

    /// A jump's top clears a ledge two levels up and falls short of one
    /// three levels up: one level past what a step climbs.
    #[test]
    fn a_jump_climbs_one_level_past_a_step() {
        let top = JUMP_ASCENT * JUMP_DURATION_MS as f32;
        assert!(top > 2.0 * RISE, "a jump of {top} misses a two-level ledge");
        assert!(top < 3.0 * RISE, "a jump of {top} clears a three-level ledge");
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

    /// How deep the bodies in the water tests float: a little under a level.
    const DRAFT: f32 = 1.0;

    /// Flat ground at z = 0 out to `radius`, water standing at the z-level
    /// `w` over every tile with `q` at or below `shore`, and a walker of
    /// [`DRAFT`] standing on the ground at the centre.
    fn lake(w: i32, shore: i32) -> (Map, NNTree, MovementInput) {
        let map = create_test_map();
        flat_ground(&map, 6);
        for q in -6..=shore {
            for r in -6..=6 {
                map.set_water(q, r, Some(w));
            }
        }
        (map, create_test_nntree(), MovementInput { draft: DRAFT, ..walking(Heading::NORTH, false) })
    }

    /// Runs `input` `ms` milliseconds in `slice` millisecond steps.
    fn run(mut input: MovementInput, ms: i16, slice: i16, map: &Map, nntree: &NNTree) -> MovementInput {
        for _ in 0..ms / slice {
            let out = calculate_movement(input, slice, map, nntree);
            input = MovementInput { airtime: out.airtime, ..carried(input, &out) };
        }
        input
    }

    /// Water deeper than its draft holds a body with its waterline on the
    /// surface, and the water is its floor; water shallower is waded, the
    /// feet on the bed.
    #[test]
    fn deep_water_floats_a_body_and_shallow_water_is_waded() {
        let (map, nntree, input) = lake(3, 6);
        let out = calculate_movement(input, 16, &map, &nntree);
        let water = water_y_from(input.position.tile, 0, 0, &map).unwrap();
        assert!((out.position.offset.y - (water - DRAFT)).abs() < 1e-5, "held at {}, the water at {water}", out.position.offset.y);
        assert_eq!(out.airtime, None, "the water holds it up");
        assert!(swimming(out.position, DRAFT, &map));

        let (map, nntree, input) = lake(1, 6);
        let out = calculate_movement(input, 16, &map, &nntree);
        assert!(out.position.offset.y.abs() < 1e-5, "wading stands on the bed: {}", out.position.offset.y);
        assert!(!swimming(out.position, DRAFT, &map));
    }

    /// A body that declares no draft walks the bed at any depth.
    #[test]
    fn a_body_that_never_floats_walks_the_bed() {
        let (map, nntree, input) = lake(6, 6);
        let input = MovementInput { draft: f32::INFINITY, ..input };
        let out = calculate_movement(input, 16, &map, &nntree);
        assert!(out.position.offset.y.abs() < 1e-5, "{}", out.position.offset.y);
        assert!(!swimming(out.position, f32::INFINITY, &map));
    }

    /// Dry ground is walked fastest, wading slower, and swimming slowest of
    /// all, however deep the water past its draft.
    #[test]
    fn the_deeper_the_water_the_slower_until_it_swims() {
        let reach = |w: Option<i32>| {
            let (map, nntree, input) = lake(w.unwrap_or(0), if w.is_some() { 6 } else { -7 });
            calculate_movement(MovementInput { moving: true, ..input }, 400, &map, &nntree).position.offset.xz().length()
        };
        let (dry, wading, swum, deeper) = (reach(None), reach(Some(1)), reach(Some(3)), reach(Some(6)));
        assert!(dry > wading && wading > swum, "dry {dry}, wading {wading}, swimming {swum}");
        assert!((swum - deeper).abs() < 1e-5, "swimming deeper went {deeper}, not {swum}");
    }

    /// A swimmer jumps from the water and comes back down onto it.
    #[test]
    fn a_jump_from_the_water_lands_back_on_it() {
        let (map, nntree, input) = lake(3, 6);
        let floating = calculate_movement(input, 16, &map, &nntree);
        let input = MovementInput { airtime: Some(JUMP_DURATION_MS), ..carried(input, &floating) };
        let up = calculate_movement(input, JUMP_DURATION_MS, &map, &nntree);
        assert!(up.position.offset.y > floating.position.offset.y + 1.0, "rose to {}", up.position.offset.y);
        let down = run(MovementInput { airtime: up.airtime, ..carried(input, &up) }, 2000, 16, &map, &nntree);
        assert_eq!(down.airtime, None, "still in the air");
        assert!((down.position.offset.y - floating.position.offset.y).abs() < 1e-5, "came down to {}", down.position.offset.y);
    }

    /// A walk off a ledge into deep water falls onto its surface, not the
    /// bed under it.
    #[test]
    fn a_fall_into_deep_water_lands_on_it() {
        let (map, nntree, input) = ledge();
        for q in 1..=6 {
            for r in -3..=3 {
                map.set_water(q, r, Some(2));
            }
        }
        let input = MovementInput { draft: DRAFT, ..input };
        let out = run(input, 2000, 16, &map, &nntree);
        let here: Qrz = out.position.tile + map.convert(out.position.offset);
        let water = water_y_from(out.position.tile, here.q, here.r, &map).expect("in the water");
        assert_eq!(out.airtime, None);
        assert!((out.position.offset.y - (water - DRAFT)).abs() < 1e-5, "landed at {}, the water at {water}", out.position.offset.y);
    }

    /// A swimmer climbs out onto a bank one level above the water's and is
    /// held at one two above, which it jumps.
    #[test]
    fn a_swimmer_climbs_one_level_out_and_jumps_two() {
        let ground = EntityType::Decorator(Decorator { cover: common::Cover::NONE, is_solid: false });
        let east = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let out_onto = |bank: i32, jump: bool| {
            let (map, nntree, input) = lake(3, 0);
            for q in 1..=6 {
                for r in -6..=6 {
                    map.insert(Qrz { q, r, z: bank }, ground);
                }
            }
            // Floating near the bank, so a jump meets it high.
            let near = east.to_world_dir() * 0.55;
            let input = MovementInput { position: Position::new(input.position.tile, Vec3::new(near.x, 0.0, near.y)), ..input };
            let input = MovementInput { airtime: None, ..carried(input, &calculate_movement(input, 16, &map, &nntree)) };
            let input = MovementInput { heading: east, moving: true, airtime: jump.then_some(JUMP_DURATION_MS), ..input };
            let out = run(input, 1500, 16, &map, &nntree);
            let here: Qrz = out.position.tile + map.convert(out.position.offset);
            here.q
        };
        // The water at 3 covers the level 2 and stands under 3.
        assert!(out_onto(3, false) > 0, "never climbed a bank one level up");
        assert_eq!(out_onto(4, false), 0, "walked up a bank two levels up");
        assert!(out_onto(4, true) > 0, "never jumped a bank two levels up");
    }

    /// Wading into deep water and swimming on reaches the same place however
    /// the time is sliced.
    #[test]
    fn swimming_is_independent_of_partition() {
        let (map, nntree, input) = lake(2, -1);
        for q in 0..=6 {
            for r in -6..=6 {
                map.set_water(q, r, Some(1));
            }
        }
        let west = Heading::from_hex(Qrz { q: -1, r: 0, z: 0 });
        let input = MovementInput { heading: west, moving: true, position: Position::at_tile(Qrz { q: 2, r: 0, z: 1 }), ..input };
        let whole = run(input, 1000, 1000, &map, &nntree);
        assert!(swimming(whole.position, DRAFT, &map), "never swam: {:?}", whole.position);
        for slice in [8, 10, 20, 25, 40, 50, 125] {
            let sliced = run(input, 1000, slice, &map, &nntree);
            assert!((whole.position.offset - sliced.position.offset).length() < 1e-3, "{slice} ms slices: {:?}, whole: {:?}", sliced.position, whole.position);
        }
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
