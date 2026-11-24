//! The night sky's turning, and the stars a player steers and tells the
//! time by. The stars are fixed to a sphere that turns about a pole in
//! the north, once a day against the sun and once more a year, as a real
//! sky does: every star rises a little earlier each night, and each
//! season has its own evening sky.
//!
//! North is −z and east +x, as `Heading` has them. A place on the sphere
//! is a unit vector as the sky stands at midnight on the year's first
//! day; [`turning`] takes it to where it stands at any other moment.

use std::f32::consts::{PI, TAU};

use bevy::math::{Quat, Vec3, Vec4};

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
/// at midnight in the middle of each fifth of the year.
pub const SIGNS: usize = 5;

/// How many stars the player steers and tells the time by.
pub const GUIDES: usize = 1 + 4 + 3 * SIGNS;

/// The guide stars, each a place on the sphere and, in `w`, how bright
/// it is, the brightest 1.
///
/// The first is the pole star, which marks north and never moves. The
/// next four are the Hand, a line out from the pole that sweeps round it
/// once a night, against the sun: straight up from the pole at midnight
/// on the year's first day, and a fifth of a turn further on at each
/// season after. The rest are the signs, three stars to each, a wedge on
/// the sphere's equator: the sign highest in the south at midnight is
/// the season's own.
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
        let wedge = [(90. - spread.1, 0., 0.9), (90. + spread.1 * 0.6, -spread.0, 0.7), (90. + spread.1 * 0.6, spread.0, 0.7)];
        for (k, (degrees, aside, bright)) in wedge.into_iter().enumerate() {
            stars[5 + 3 * sign + k] = place(f32::to_radians(degrees), highest + aside).extend(bright);
        }
    }
    stars
}

#[cfg(test)]
mod tests {
    use super::*;

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
