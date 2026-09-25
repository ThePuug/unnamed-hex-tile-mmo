//! The shot script the recorder reads: JSON, one file per film.
//!
//! A shot puts the player on a tile facing a bearing, waits for the world
//! there to have arrived, then records `seconds` of frames while the camera
//! runs its path, the lighting clock its keys and the player its keys.
//! A shot of zero seconds is a still, saved as a PNG.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Script {
    /// Frames per second of the recorded video, and of the game while it
    /// records: each frame steps the game exactly `1 / fps`.
    pub fps: u32,
    /// The window's physical size while recording, and the video's.
    pub size: [u32; 2],
    /// Directory the videos and stills are written to, one file per shot.
    pub out: PathBuf,
    /// Record only the shots of these names, when given.
    #[serde(default)]
    pub only: Option<Vec<String>>,
    /// Quit the client once the last shot is written.
    #[serde(default)]
    pub exit: bool,
    pub shots: Vec<Shot>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Shot {
    pub name: String,
    /// The tile, `[q, r]`, the player is put on.
    pub at: [i32; 2],
    /// The bearing the player turns to before the shot, in degrees
    /// clockwise from north. The server streams the far view about it.
    pub face: f32,
    pub seconds: f32,
    /// Real seconds the world is given after it has settled before the
    /// shot rolls, for what arrives after the terrain: trees, far regions.
    #[serde(default = "default_settle")]
    pub settle: f32,
    #[serde(default)]
    pub hide_player: bool,
    /// The lighting clock: `[t, hours]` keys, hours counted from the start
    /// of the year, held linear between keys.
    pub clock: Vec<[f64; 2]>,
    #[serde(default)]
    pub camera: CameraPath,
    /// Keys the player presses: `[t, "hold" | "release" | "tap", key]`.
    #[serde(default)]
    pub input: Vec<(f32, Press, String)>,
}

fn default_settle() -> f32 {
    4.0
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Press {
    Hold,
    Release,
    Tap,
}

/// Where the camera is through a shot.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CameraPath {
    /// The gameplay camera, as a player sees it.
    #[default]
    Follow,
    /// A path through keys, each a place and a point looked at, in world
    /// units from the anchor: x east, y up, z south.
    Path {
        #[serde(default)]
        anchor: Anchor,
        /// Ease in at the first key and out at the last; otherwise the
        /// path leaves and arrives moving.
        #[serde(default)]
        ease: bool,
        keys: Vec<CameraKey>,
        /// When given, the gameplay camera takes over from the path at
        /// this time, blended in over `blend` seconds; it follows the
        /// player throughout, so it is where play would have it.
        #[serde(default)]
        follow_at: Option<f32>,
        #[serde(default = "default_blend")]
        blend: f32,
    },
}

fn default_blend() -> f32 {
    1.5
}

/// How far the gameplay camera has taken over from a path at time `t`:
/// none before `follow_at`, all of it `blend` seconds after, eased in and
/// out between.
pub fn handover(follow_at: Option<f32>, blend: f32, t: f32) -> f32 {
    let Some(at) = follow_at else { return 0.0 };
    let s = ((t - at) / blend.max(1e-3)).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// What a camera path is measured from.
#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    /// The player's feet where the shot rolled.
    #[default]
    Start,
    /// The player's feet as they move.
    Player,
    /// The player's feet as they move, turned with the heading: -z is the
    /// way the player faces and x its right hand.
    Heading,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct CameraKey {
    pub t: f32,
    pub from: [f32; 3],
    pub look: [f32; 3],
    /// Vertical field of view, in degrees.
    pub fov: f32,
}

/// The camera at time `t` of a path: where it stands, the point it looks
/// at, and its vertical field of view in radians. A cubic through the keys
/// with Catmull-Rom tangents over their times, so it passes through each
/// key and its speed is continuous across them; before the first key and
/// after the last it holds.
pub fn camera_at(keys: &[CameraKey], ease: bool, t: f32) -> (Vec3, Vec3, f32) {
    let from: Vec<Vec3> = keys.iter().map(|k| Vec3::from(k.from)).collect();
    let look: Vec<Vec3> = keys.iter().map(|k| Vec3::from(k.look)).collect();
    let fov: Vec<Vec3> = keys.iter().map(|k| Vec3::splat(k.fov.to_radians())).collect();
    let times: Vec<f32> = keys.iter().map(|k| k.t).collect();
    (
        spline(&times, &from, ease, t),
        spline(&times, &look, ease, t),
        spline(&times, &fov, ease, t).x,
    )
}

/// Cubic Hermite through `points` at `times`. Inner tangents span the
/// neighbours; the ends' are zero when easing, else one-sided.
fn spline(times: &[f32], points: &[Vec3], ease: bool, t: f32) -> Vec3 {
    let n = points.len();
    assert!(n > 0 && times.len() == n, "a path needs keys");
    if n == 1 || t <= times[0] {
        return points[0];
    }
    if t >= times[n - 1] {
        return points[n - 1];
    }
    let tangent = |i: usize| -> Vec3 {
        if i == 0 {
            if ease { Vec3::ZERO } else { (points[1] - points[0]) / (times[1] - times[0]) }
        } else if i == n - 1 {
            if ease { Vec3::ZERO } else { (points[i] - points[i - 1]) / (times[i] - times[i - 1]) }
        } else {
            (points[i + 1] - points[i - 1]) / (times[i + 1] - times[i - 1])
        }
    };
    let i = times.windows(2).position(|w| t < w[1]).unwrap_or(n - 2);
    let span = times[i + 1] - times[i];
    let s = (t - times[i]) / span;
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    points[i] * h00 + tangent(i) * (h10 * span) + points[i + 1] * h01 + tangent(i + 1) * (h11 * span)
}

/// The lighting clock at time `t`, in hours from the start of the year:
/// linear between keys, held outside them.
pub fn clock_at(keys: &[[f64; 2]], t: f32) -> f64 {
    let t = t as f64;
    let first = keys.first().expect("a shot needs a clock");
    if t <= first[0] {
        return first[1];
    }
    for w in keys.windows(2) {
        if t < w[1][0] {
            let s = (t - w[0][0]) / (w[1][0] - w[0][0]);
            return w[0][1] + (w[1][1] - w[0][1]) * s;
        }
    }
    keys.last().unwrap()[1]
}

/// The key a script names, by Bevy's name for it.
pub fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name {
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "KeyG" => KeyCode::KeyG,
        "Numpad0" => KeyCode::Numpad0,
        "Numpad1" => KeyCode::Numpad1,
        "NumpadEnter" => KeyCode::NumpadEnter,
        "NumpadDecimal" => KeyCode::NumpadDecimal,
        "ShiftLeft" => KeyCode::ShiftLeft,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f32, x: f32) -> CameraKey {
        CameraKey { t, from: [x, 0.0, 0.0], look: [x, 0.0, -1.0], fov: 30.0 }
    }

    /// The path passes through every key and holds beyond the ends.
    #[test]
    fn the_path_passes_through_its_keys() {
        let keys = [key(0.0, 0.0), key(2.0, 10.0), key(5.0, 4.0)];
        for ease in [false, true] {
            for k in &keys {
                let (from, _, fov) = camera_at(&keys, ease, k.t);
                assert!((from.x - k.from[0]).abs() < 1e-4, "{ease} at {}: {from}", k.t);
                assert!((fov - 30_f32.to_radians()).abs() < 1e-6);
            }
            assert_eq!(camera_at(&keys, ease, -1.0).0.x, 0.0);
            assert_eq!(camera_at(&keys, ease, 9.0).0.x, 4.0);
        }
    }

    /// No jump anywhere along the path: small steps in time move the
    /// camera by small steps, across keys as much as between them.
    #[test]
    fn the_path_is_continuous() {
        let keys = [key(0.0, 0.0), key(1.0, 10.0), key(3.0, -5.0), key(4.0, 0.0)];
        let dt = 1.0 / 240.0;
        let mut last = camera_at(&keys, false, 0.0).0;
        let mut t = dt;
        while t <= 4.0 {
            let here = camera_at(&keys, false, t).0;
            assert!(here.distance(last) < 0.5, "jump at {t}: {last} -> {here}");
            last = here;
            t += dt;
        }
    }

    /// Eased, the path starts and ends at rest; not eased, it is already
    /// moving at its first key.
    #[test]
    fn easing_starts_and_ends_at_rest() {
        let keys = [key(0.0, 0.0), key(4.0, 10.0)];
        let early = |ease| camera_at(&keys, ease, 0.01).0.x;
        assert!(early(true) < early(false) / 10.0, "{} vs {}", early(true), early(false));
    }

    /// The gameplay camera takes over only once its time comes, all of it
    /// by the end of the blend, never going back; without a time, never.
    #[test]
    fn the_gameplay_camera_takes_over_across_the_blend() {
        assert_eq!(handover(None, 1.5, 9.0), 0.0);
        assert_eq!(handover(Some(4.0), 1.5, 3.9), 0.0);
        assert_eq!(handover(Some(4.0), 1.5, 5.5), 1.0);
        let mut last = 0.0;
        for i in 0..=30 {
            let s = handover(Some(4.0), 1.5, 4.0 + i as f32 * 0.05);
            assert!(s >= last && (0.0..=1.0).contains(&s), "{s} after {last}");
            last = s;
        }
    }

    /// The clock runs linear between keys, across midnight as much as
    /// within a day, and holds outside them.
    #[test]
    fn the_clock_runs_between_its_keys() {
        let keys = [[0.0, 20.0], [10.0, 30.0]];
        assert_eq!(clock_at(&keys, -1.0), 20.0);
        assert!((clock_at(&keys, 5.0) - 25.0).abs() < 1e-9);
        assert_eq!(clock_at(&keys, 11.0), 30.0);
    }

    #[test]
    fn a_script_reads_from_json() {
        let text = r#"{
            "fps": 30, "size": [1920, 1080], "out": "proofs/teaser",
            "shots": [
                { "name": "vista", "at": [10, -4], "face": 90, "seconds": 6,
                  "clock": [[0, 6.5], [6, 7.0]],
                  "camera": { "kind": "path", "ease": true, "keys": [
                      { "t": 0, "from": [0, 40, 60], "look": [0, 10, -200], "fov": 35 },
                      { "t": 6, "from": [0, 80, 60], "look": [0, 10, -200], "fov": 35 } ] } },
                { "name": "chop", "at": [1, 2], "face": 0, "seconds": 10,
                  "clock": [[0, 14]], "input": [[1.0, "tap", "KeyG"]] }
            ] }"#;
        let script: Script = serde_json::from_str(text).expect("parses");
        assert_eq!(script.shots.len(), 2);
        assert!(matches!(script.shots[0].camera, CameraPath::Path { ease: true, .. }));
        assert!(matches!(script.shots[1].camera, CameraPath::Follow));
        assert_eq!(script.shots[1].input[0].1, Press::Tap);
        assert!(key_code(&script.shots[1].input[0].2).is_some());
    }
}
