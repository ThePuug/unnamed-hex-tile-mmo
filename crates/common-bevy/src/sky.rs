//! The night sky's turning, and the stars a player steers and tells the
//! time by. The stars are fixed to a sphere that turns about a pole in
//! the north, once a day against the sun and once more a year, as a real
//! sky does: every star rises a little earlier each night, and each
//! season has its own evening sky. The year it turns by is the four
//! seasons' (`Date::turned`). Through a leap season the world is off its
//! path and round the visitor ([`visit`]): it is set down where it was
//! taken, so the sun is where it was among the stars, and a star rises at
//! much the hour it did the night before.
//!
//! North is −z and east +x, as `Heading` has them. A place on the sphere
//! is a unit vector as the sky stands at midnight on the year's first
//! day; [`turning`] takes it to where it stands at any other moment.

use std::f32::consts::{PI, TAU};

use bevy::math::{Quat, Vec2, Vec3, Vec4};

/// How high the pole stands over the northern horizon, in radians: high
/// enough that the stars close about it never set, and low enough that a
/// look raised to the sky from level ground holds it and the Hand.
pub const POLE_ELEVATION: f32 = 35. * PI / 180.;

/// The pole the sky turns about: due north, [`POLE_ELEVATION`] up.
pub fn pole() -> Vec3 {
    Vec3::new(0., POLE_ELEVATION.sin(), -POLE_ELEVATION.cos())
}

/// The point of the sphere's equator that stands highest in the south.
fn south() -> Vec3 {
    Vec3::new(0., POLE_ELEVATION.cos(), POLE_ELEVATION.sin())
}

/// How far the sphere has turned, in turns, at `day` of the way through
/// the day from midnight and `year` of the way through the year: once a
/// day and once a year besides.
pub fn turn(day: f32, year: f32) -> f32 {
    (day + year).rem_euclid(1.)
}

/// The rotation that takes a place on the sphere to where it stands in
/// the sky at `day` and `year`: about the pole, rising in the east and
/// setting in the west.
pub fn turning(day: f32, year: f32) -> Quat {
    Quat::from_axis_angle(pole(), -TAU * turn(day, year))
}

/// A place on the sphere `from_pole` radians from the pole, that stands
/// highest, on the meridian over the south, when the sphere has turned
/// `highest` of a turn.
pub fn place(from_pole: f32, highest: f32) -> Vec3 {
    let on_meridian = Quat::from_axis_angle(Vec3::X, from_pole - PI / 2.) * south();
    Quat::from_axis_angle(pole(), TAU * highest) * on_meridian
}

/// How many seasons' signs ring the sphere's equator: one stands highest
/// at midnight in the middle of each of the four seasons. The leap season
/// has none.
pub const SIGNS: usize = 4;

/// How far from the pole a sign's middle lies, in degrees: south of the
/// sphere's equator, so that at its highest it stands a third of the way
/// up the southern sky, in a look raised from level ground. On the
/// equator it would pass over the top of that frame, seen only setting.
const SIGN_FROM_POLE: f32 = 112.;

/// How many stars the player steers and tells the time by.
pub const GUIDES: usize = 1 + 4 + 3 * SIGNS;

/// The guide stars, each a place on the sphere and, in `w`, how bright
/// it is, the brightest 1.
///
/// The first is the pole star, which marks north and never moves. The
/// next four are the Hand, a line out from the pole that sweeps round it
/// once a night, against the sun: straight up from the pole at midnight
/// on the year's first day, and a quarter of a turn further on at each
/// season after. The rest are the signs, three stars to each, a wedge
/// south of the sphere's equator: the sign highest in the south at
/// midnight is the season's own, and for most of a night the only one up.
pub fn guides() -> [Vec4; GUIDES] {
    let mut stars = [Vec4::ZERO; GUIDES];
    stars[0] = pole().extend(1.);
    for (k, degrees) in [6.0_f32, 10., 14., 18.].into_iter().enumerate() {
        stars[1 + k] = place(degrees.to_radians(), 0.).extend(0.8 - 0.1 * k as f32);
    }
    for sign in 0..SIGNS {
        let highest = (sign as f32 + 0.5) / SIGNS as f32;
        // A wedge, its point toward the pole, wider for each later sign
        let spread = (0.012 + 0.004 * sign as f32, 5. + 1.5 * sign as f32);
        let wedge = [(SIGN_FROM_POLE - spread.1, 0., 0.9), (SIGN_FROM_POLE + spread.1 * 0.6, -spread.0, 0.7), (SIGN_FROM_POLE + spread.1 * 0.6, spread.0, 0.7)];
        for (k, (degrees, aside, bright)) in wedge.into_iter().enumerate() {
            stars[5 + 3 * sign + k] = place(f32::to_radians(degrees), highest + aside).extend(bright);
        }
    }
    stars
}

/// The world's path round the visitor, in the world's own path round the
/// sun as one: a circle a quarter as wide, touching it, gone round the other
/// way in a quarter of the time, so the world keeps its pace through the
/// whole figure of eight and only the way it bends changes.
const LOOP: f32 = 0.25;
/// How near the sun the visitor's oval comes, on the far side of the sun
/// from its far turn, which is the middle of the world's loop.
const NEAR: f32 = 0.15;
/// How many seasons the visitor takes round its oval: the world's year, so
/// it meets the world at one place every year.
const ROUND: f32 = 4.;
/// How many seasons about the middle of a leap season the visitor is slowed
/// over, the year it takes the world. Wider than two, or it would stand
/// still at the middle.
const SLOWED: f32 = 3.;

/// How the sky stands on the visitor's round, as the world sees it.
#[derive(Clone, Copy, Debug)]
pub struct Visit {
    /// How far the visitor stands east of the sun, in turns: behind it
    /// by that share of a day.
    pub east: f32,
    /// How near the visitor is, as a share of its nearest at a meeting:
    /// its width in the sky goes by it.
    pub near: f32,
    /// How far the sun has moved on among the stars since the meeting,
    /// in turns.
    pub sun: f32,
    /// How far off the sun is, in the year's own distance: its width
    /// goes by one over it.
    pub far: f32,
    /// How far off the visitor is, in the same distance.
    pub off: f32,
    /// Which way the visitor lies from the world in their plane, in
    /// turns: the world sees it from every side as this goes round.
    pub bearing: f32,
}

/// The sky `t` seasons from the visitor's far turn, on the round it takes
/// the world or on one it does not (`Date::visitor`): two seasons either
/// side of the turn, or two and a half where it takes the world. The
/// world's path, the visitor's and the sun lie in one plane, the sun at
/// its middle.
///
/// The visitor goes round a narrow oval, the sun at one end of it, by the
/// area law: quick past the sun and slow at its far turn, just outside the
/// world's path. Every year it reaches that turn as the world passes
/// inside it, and stands opposite the sun, as near as it comes. That is
/// the meeting. The year it takes the world it comes late to its turn, a
/// season lost smoothly across [`SLOWED`], and the world leaves its path
/// at the meeting place, goes once round the loop the visitor is climbing
/// through, the other way, and is set down where it left, a season on:
/// the visitor is across the sun at the loop's middle, with the sun at
/// its furthest. Both have lost the one season, so they meet the year
/// after as every year. Half a year from a meeting the visitor rounds the
/// sun, between it and the world.
pub fn visit(t: f32, taken: bool) -> Visit {
    let (world, visitor) = places(t, taken);
    let (to_visitor, to_sun) = (visitor - world, -world);
    Visit {
        east: (to_visitor.to_angle() - to_sun.to_angle()) / TAU,
        near: LOOP / to_visitor.length(),
        sun: world.to_angle() / TAU,
        far: world.length(),
        off: to_visitor.length(),
        bearing: to_visitor.to_angle() / TAU,
    }
}

/// Where the world and the visitor are, the sun at the middle and the
/// meeting place out along +x, at `t` as [`visit`] takes it.
fn places(t: f32, taken: bool) -> (Vec2, Vec2) {
    let on_path = |seasons: f32| Vec2::from_angle(TAU / 4. * seasons);
    if !taken {
        return (on_path(t), oval(t));
    }
    // The season the visitor has lost by `t`: half of it before the
    // loop's middle and half after, and its pace never less than a third.
    let lost = if t.abs() >= SLOWED / 2. { 0.5 * t.signum() } else { t / SLOWED + (TAU * t / SLOWED).sin() / TAU };
    let world = if t.abs() <= 0.5 {
        let (sin, cos) = (TAU * (t + 0.5)).sin_cos();
        Vec2::new(1. + LOOP - LOOP * cos, LOOP * sin)
    } else {
        on_path(t - 0.5 * t.signum())
    };
    (world, oval(t - lost))
}

/// Where the visitor is on its oval, `seasons` of its own pace from its
/// far turn: equal areas swept from the sun in equal times, the way the
/// world goes round.
fn oval(seasons: f32) -> Vec2 {
    let (reach, near) = (1. + LOOP, NEAR);
    let (half, squash) = ((reach + near) / 2., (reach - near) / (reach + near));
    // Kepler's equation for the angle from the oval's middle, by Newton
    // from a start that converges for an oval this narrow.
    let mean = PI + TAU * seasons / ROUND;
    let mut angle = mean + squash * mean.sin();
    for _ in 0..12 {
        angle -= (angle - squash * angle.sin() - mean) / (1. - squash * angle.cos());
    }
    -Vec2::new(half * (angle.cos() - squash), half * (1. - squash * squash).sqrt() * angle.sin())
}

/// The brightest place of the pale band, where it is widest: highest in
/// the south, well clear of the hills, two hours before midnight in the
/// middle of the second season, so its nights have the core and the
/// fourth season's the thin far side. It stands west of that season's
/// sign by more than its own width, so the sign is read on dark sky.
pub fn core() -> Vec3 {
    place(105_f32.to_radians(), 1.2 / 4.)
}

/// The pole of the great circle the pale band lies along. The band
/// crosses the sphere's equator steeply at the [`core`] and passes the
/// sky's pole further off than the Hand reaches.
pub fn band() -> Vec3 {
    let core = core();
    let north = (pole() - core * pole().dot(core)).normalize();
    let along = Quat::from_axis_angle(core, 30_f32.to_radians()) * north;
    core.cross(along).normalize()
}

/// The glowing clouds in the pale band, each a place on it and, in `w`,
/// its radius as a sine: one beside the core and one out along the band.
pub fn nebulae() -> [Vec4; 2] {
    let (core, band) = (core(), band());
    let side = band.cross(core);
    let on_band = |degrees: f32, off: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (core * cos + side * sin + band * off).normalize()
    };
    [on_band(24., 0.06).extend(0.07), on_band(-72., -0.03).extend(0.05)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apart(a: f32, b: f32) -> f32 {
        ((a - b + 0.5).rem_euclid(1.) - 0.5).abs()
    }

    /// Every year the visitor stands opposite the sun at its nearest as
    /// the world passes, and is across the sun half a year on, far off;
    /// its place in the sky never jumps; and one round ends as the next
    /// begins, whichever takes the world.
    #[test]
    fn the_visitor_meets_the_world_every_year() {
        let meeting = visit(0., false);
        assert!(apart(meeting.east, 0.5) < 1e-4 && (meeting.near - 1.).abs() < 1e-4 && (meeting.far - 1.).abs() < 1e-5, "{meeting:?}");
        for (t, taken) in [(-2., false), (2., false), (-2.5, true), (2.5, true)] {
            let (world, visitor) = places(t, taken);
            let (ends, there) = (places(2., false), visit(t, taken));
            assert!((world - ends.0).length() < 1e-4 && (visitor - ends.1).length() < 1e-4, "every round ends alike, at {t}");
            assert!(apart(there.east, 0.) < 1e-3 && there.off < there.far && there.near < 0.5, "rounding the sun before it, far off: {there:?}");
        }
        for taken in [false, true] {
            let reach = if taken { 2.5 } else { 2. };
            let mut east = visit(-reach, taken).east;
            for step in 1..=4000 {
                let now = visit(-reach + 2. * reach * step as f32 / 4000., taken);
                assert!(apart(now.east, east) < 0.02, "it moved {} at step {step}", apart(now.east, east));
                assert!(now.near < 1.1, "never much nearer than at a meeting: {now:?}");
                east = now.east;
            }
        }
    }

    /// The year it takes the world: the world leaves its path where they
    /// meet and is set down there a season on, the sun where it was among
    /// the stars; the visitor is across the sun halfway, with the sun at
    /// its furthest; it stays near all the while and never stands still.
    #[test]
    fn the_visitor_takes_the_world_once_round_it() {
        let (taken, across, let_go) = (visit(-0.5, true), visit(0., true), visit(0.5, true));
        for seen in [taken, across, let_go] {
            assert!(seen.sun.abs() < 1e-4, "the sun is where it was among the stars: {seen:?}");
        }
        assert!((taken.far - 1.).abs() < 1e-5 && (let_go.far - 1.).abs() < 1e-5 && (across.far - (1. + 2. * LOOP)).abs() < 1e-5);
        assert!(apart(across.east, 0.) < 1e-4 && across.off < across.far, "across the sun halfway: {across:?}");
        assert!(apart(taken.east, 0.5) < 0.1 && apart(let_go.east, 0.5) < 0.1, "and all but opposite it at either end");
        assert!(visit(-0.25, true).sun > 0.01 && visit(0.25, true).sun < -0.01, "the sun sways on and back while the world is held");
        let mut was = places(-1.5, true).1;
        for step in 1..=3000 {
            let t = -1.5 + step as f32 / 1000.;
            let (_, visitor) = places(t, true);
            assert!((visitor - was).length() > 1e-5, "the visitor never stands still: {t}");
            assert!(t.abs() > 0.5 || visit(t, true).near > 0.8, "and is near all the while it holds the world: {t}");
            was = visitor;
        }
    }

    /// The band's core stands high in the south on the second season's
    /// nights and under the ground on the fourth's, and the band lies
    /// through it, clear of the Hand.
    #[test]
    fn the_band_keeps_its_season() {
        let (core, band) = (core(), band());
        assert!(core.dot(band).abs() < 1e-5 && (band.length() - 1.).abs() < 1e-5, "the core lies on the band");
        let blaze = turning(0., 1.5 / 4.) * core;
        assert!(blaze.y > 0.5 && blaze.z > 0., "high in the south: {blaze:?}");
        assert!((turning(0., 3.5 / 4.) * core).y < 0., "set, half a year on");
        let reach = guides()[1..5].iter().map(|hand| hand.truncate().dot(pole())).fold(1., f32::min);
        assert!(band.dot(pole()).abs() > (1. - reach * reach).sqrt(), "the band passes outside the Hand");
        assert!(nebulae().iter().all(|cloud| cloud.truncate().dot(band).abs() < 0.1), "the clouds lie in the band");
    }

    fn stands(star: Vec3, day: f32, year: f32) -> Vec3 {
        turning(day, year) * star
    }

    /// The pole star never moves, and every other star keeps its
    /// distance from it.
    #[test]
    fn the_sky_turns_about_its_pole() {
        for (day, year) in [(0., 0.), (0.3, 0.1), (0.9, 0.77)] {
            assert!((stands(pole(), day, year) - pole()).length() < 1e-5);
            for star in guides() {
                let star = star.truncate();
                assert!((stands(star, day, year).dot(pole()) - star.dot(pole())).abs() < 1e-5);
            }
        }
        assert!(pole().z < 0. && pole().y > 0., "north and up");
    }

    /// A star in the south moves east to west through the night, is back
    /// where it stood a day less a little later, and at one hour of the
    /// night goes once round in a year.
    #[test]
    fn a_star_crosses_the_south_westward_and_earlier_each_night() {
        let star = place(PI / 2., 0.);
        assert!((star - south()).length() < 1e-5, "at midnight on the first day it stands highest in the south");
        assert!(stands(star, 0.02, 0.).x < 0., "then it goes west");
        assert!(stands(star, -0.02, 0.).x > 0., "from the east");
        let (days, night) = (360., 0.25);
        let next = stands(star, night, 1. / days);
        assert!((stands(star, night + 1. / days, 0.) - next).length() < 1e-4, "a day on is a little later that night");
        assert!((stands(star, night, 1.) - stands(star, night, 0.)).length() < 1e-4, "a year on, the same sky");
        assert!((stands(star, night, 0.5) - stands(star, night, 0.)).length() > 0.5, "half a year on, the other half of it");
    }

    /// Each sign stands highest in the south at midnight in the middle
    /// of its own season, and the Hand points up from the pole at
    /// midnight on the year's first day.
    #[test]
    fn the_signs_keep_the_seasons_and_the_hand_the_hour() {
        let stars = guides();
        for sign in 0..SIGNS {
            let year = (sign as f32 + 0.5) / SIGNS as f32;
            let lead = stands(stars[5 + 3 * sign].truncate(), 0., year);
            assert!(lead.x.abs() < 1e-3 && lead.z > 0. && lead.y > 0.5, "sign {sign} at its midnight: {lead:?}");
            assert!(stars[5 + 3 * sign..][..3].iter().all(|star| stands(star.truncate(), 0., year).y < pole().y + 0.15), "sign {sign} stands in the frame that holds the pole");
            let other = stands(stars[5 + 3 * ((sign + 2) % SIGNS)].truncate(), 0., year);
            assert!(other.y < lead.y, "no other sign stands as high then");
        }
        let tip = stars[4].truncate();
        let up = stands(tip, 0., 0.);
        assert!(up.x.abs() < 1e-4 && up.y > pole().y, "up from the pole");
        let later = stands(tip, 0.25, 0.);
        assert!(later.x < -0.1, "a quarter of the night on, it has swung west of the pole");
        assert!(guides().iter().all(|star| (star.truncate().length() - 1.).abs() < 1e-4 && star.w > 0.));
    }
}
