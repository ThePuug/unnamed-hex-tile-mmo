//! The hubs view: a scenario of hearths named on the command line,
//! settled by the game's own rule (`common_bevy::hubs`) and drawn over
//! whatever is beneath. It reads no world: where a hub's centre stands,
//! what its core holds, what took what in and how far its influence
//! reaches are all asked of [`Hubs`], tile by tile, so the picture is the
//! rule and never a second one.

use common::network::Identity;
use common_bevy::hubs::{haven_reach, HubId, Hubs, HAVEN_GROUND, OUTPOST_CORE};

/// A hearth of the scenario: its tile, and the players at home at it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hearth {
    pub tile: (i32, i32),
    pub players: u32,
}

impl Hearth {
    /// The hearth `spec` names: `<q>,<r>` or `<q>,<r>:<players>`.
    pub fn parse(spec: &str) -> Result<Hearth, String> {
        let (tile, players) = spec.split_once(':').unwrap_or((spec, "0"));
        let (q, r) = tile.split_once(',').ok_or_else(|| format!("a hearth is <q>,<r>[:<players>], and {spec:?} is not"))?;
        let number = |s: &str| s.trim().parse::<i32>().map_err(|_| format!("{s:?} in {spec:?} is not a whole number"));
        Ok(Hearth { tile: (number(q)?, number(r)?), players: number(players)?.max(0) as u32 })
    }
}

/// The hubs the first `count` of `hearths` make, built in the order given
/// round the haven at `haven`, each with its players at home. A hearth
/// named again is not built again: the players at home at it become the
/// number it is named with, the rest having stopped coming.
pub fn settle(haven: (i32, i32), hearths: &[Hearth], count: usize) -> Hubs {
    let mut hubs = Hubs::new(haven);
    hubs.sheds = true;
    let mut next = 0u32;
    let mut living: std::collections::HashMap<(i32, i32), Vec<Identity>> = std::collections::HashMap::new();
    for hearth in &hearths[..count.min(hearths.len())] {
        if !living.contains_key(&hearth.tile) {
            hubs.founded(hearth.tile);
        }
        let at = living.entry(hearth.tile).or_default();
        while at.len() > hearth.players as usize {
            hubs.gone(at.pop().expect("more than none"));
        }
        while at.len() < hearth.players as usize {
            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&next.to_le_bytes());
            next += 1;
            hubs.resident(Identity::Key(key), Some(hearth.tile));
            at.push(Identity::Key(key));
        }
    }
    hubs
}

/// The picture being drawn over: its pixels, three bytes each, and where
/// it lies in the world.
pub struct Canvas<'a> {
    pub buf: &'a mut [u8],
    pub w: usize,
    pub h: usize,
    pub origin: (f64, f64),
    pub scale: f64,
}

/// The haven's hue, which no hub takes.
const GOLD: [f64; 3] = [1.0, 0.82, 0.2];

/// A hub's own hue, by its place among the hubs standing: hues no terrain
/// fill uses.
fn hue(place: usize) -> [f64; 3] {
    const HUES: [[f64; 3]; 6] = [[0.95, 0.25, 0.55], [0.25, 0.85, 0.95], [0.75, 0.45, 1.0], [1.0, 0.55, 0.15], [0.45, 1.0, 0.45], [0.6, 0.75, 1.0]];
    HUES[place % HUES.len()]
}

impl Canvas<'_> {
    fn tile(&self, px: usize, py: usize) -> (i32, i32) {
        world::world_to_hex(self.origin.0 + px as f64 * self.scale, self.origin.1 + py as f64 * self.scale)
    }

    fn blend(&mut self, px: usize, py: usize, rgb: [f64; 3], share: f64) {
        let at = (py * self.w + px) * 3;
        for (c, tint) in rgb.iter().enumerate() {
            let was = self.buf[at + c] as f64 / 255.0;
            self.buf[at + c] = ((was + (tint - was) * share).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    fn dot(&mut self, tile: (i32, i32), radius: i32, rgb: [f64; 3]) {
        let (wx, wy) = common::hex_to_world(tile.0, tile.1);
        let (cx, cy) = (((wx - self.origin.0) / self.scale) as i32, ((wy - self.origin.1) / self.scale) as i32);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (x, y) = (cx + dx, cy + dy);
                if dx * dx + dy * dy <= radius * radius && x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
                    self.blend(x as usize, y as usize, rgb, 1.0);
                }
            }
        }
    }

    fn line(&mut self, from: (i32, i32), to: (i32, i32), rgb: [f64; 3]) {
        let at = |tile: (i32, i32)| {
            let (wx, wy) = common::hex_to_world(tile.0, tile.1);
            ((wx - self.origin.0) / self.scale, (wy - self.origin.1) / self.scale)
        };
        let ((x0, y0), (x1, y1)) = (at(from), at(to));
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let (x, y) = ((x0 + (x1 - x0) * t).round() as i32, (y0 + (y1 - y0) * t).round() as i32);
            if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
                self.blend(x as usize, y as usize, rgb, 0.9);
            }
        }
    }
}

/// What a pixel's tile is to the hubs: the hub whose core holds it, which
/// hubs' influence reaches it, which hearths' own ground it is, the core
/// each would hold standing alone, and whether the haven's guard keeps it
/// and its influence reaches it.
#[derive(Clone, PartialEq)]
struct Read {
    held: Option<HubId>,
    reached: Vec<HubId>,
    alone: Vec<usize>,
    guarded: bool,
    haven: bool,
}

/// Draws `hubs` over `canvas`: the haven's and every hub's influence as a
/// wash, whiter the stronger; a hub's core filled in its hue and
/// outlined; its reach outlined thin; the core each of `hearths` would
/// hold alone, dotted, so what was taken in shows inside what took it; a
/// line from each hearth to its hub's centre, a dot at each hearth and a
/// larger one at each centre. The haven is gold: its guarded ground
/// outlined, its reach outlined thin, a ringed dot at its point.
pub fn draw(canvas: &mut Canvas, hubs: &Hubs, hearths: &[Hearth]) {
    let mut standing: Vec<(HubId, (i32, i32), f32, u32)> = hubs.each().collect();
    standing.sort_by_key(|hub| hub.0);
    let place = |id: HubId| standing.iter().position(|hub| hub.0 == id).unwrap_or(0);
    let far = |a: (i32, i32), b: (i32, i32)| ((a.0 - b.0).abs().max((a.1 - b.1).abs()).max((a.0 + a.1 - b.0 - b.1).abs())) as f32;
    let read = |tile: (i32, i32)| Read {
        held: hubs.holding(tile),
        reached: standing.iter().filter(|hub| hubs.reaches(hub.0, tile)).map(|hub| hub.0).collect(),
        alone: hearths.iter().enumerate().filter(|(_, hearth)| far(hearth.tile, tile) <= OUTPOST_CORE).map(|(k, _)| k).collect(),
        guarded: hubs.guarded(tile),
        haven: hubs.haven_reaches(tile),
    };
    let (w, h) = (canvas.w, canvas.h);
    let reads: Vec<Read> = (0..w * h).map(|i| read(canvas.tile(i % w, i / w))).collect();
    for py in 0..h {
        for px in 0..w {
            let here = &reads[py * w + px];
            let beside = [(px + 1 < w).then(|| &reads[py * w + px + 1]), (py + 1 < h).then(|| &reads[(py + 1) * w + px])];
            let edge = |differs: &dyn Fn(&Read) -> bool| beside.iter().flatten().any(|other| differs(other));
            canvas.blend(px, py, [1.0, 1.0, 1.0], 0.45 * hubs.influence(canvas.tile(px, py)) as f64);
            if let Some(id) = here.held {
                canvas.blend(px, py, hue(place(id)), 0.3);
            }
            if edge(&|other| other.haven != here.haven) {
                canvas.blend(px, py, GOLD, 0.55);
            }
            if edge(&|other| other.guarded != here.guarded) {
                canvas.blend(px, py, GOLD, 1.0);
            }
            // A reach's edge is its own hub's: the one reaching one side only
            let crossed = beside.iter().flatten().find_map(|other| {
                here.reached.iter().find(|id| !other.reached.contains(id)).or(other.reached.iter().find(|id| !here.reached.contains(id))).copied()
            });
            if let Some(id) = crossed {
                canvas.blend(px, py, hue(place(id)), 0.55);
            }
            if edge(&|other| other.alone != here.alone) && (px + py) % 4 < 2 {
                canvas.blend(px, py, [1.0, 1.0, 1.0], 0.8);
            }
            if edge(&|other| other.held != here.held) {
                let id = here.held.or(beside.iter().flatten().find_map(|other| other.held));
                canvas.blend(px, py, id.map_or([1.0; 3], |id| hue(place(id))), 1.0);
            }
        }
    }
    let size = (2.0 / canvas.scale).clamp(2.0, 6.0) as i32;
    for &(id, centre, ..) in &standing {
        for hearth in hubs.hearths(id) {
            canvas.line(hearth, centre, hue(place(id)));
        }
    }
    for &(id, centre, ..) in &standing {
        for hearth in hubs.hearths(id) {
            canvas.dot(hearth, size, [1.0, 1.0, 1.0]);
        }
        canvas.dot(centre, size + 2, [0.05, 0.05, 0.05]);
        canvas.dot(centre, size + 1, hue(place(id)));
    }
    canvas.dot(hubs.haven(), size + 2, [0.05, 0.05, 0.05]);
    canvas.dot(hubs.haven(), size + 1, GOLD);
}

/// Says how every hub of `hubs` stands, and what was taken in by what.
pub fn report(hubs: &Hubs) {
    log::info!("  the haven: at {:?}, guarding {HAVEN_GROUND:.0} tiles, influence whole reaching {:.0} tiles", hubs.haven(), haven_reach());
    let mut standing: Vec<_> = hubs.each().collect();
    standing.sort_by_key(|hub| hub.0);
    for (id, centre, core, size) in standing {
        log::info!(
            "  {id:?}: centre {centre:?}, {size} at home, core {core:.1} tiles, influence {:.2} reaching {:.0} tiles, {} hearths",
            hubs.strength(id),
            hubs.reach(id),
            hubs.hearths(id).count()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hearth_is_named_by_its_tile_and_its_players() {
        assert_eq!(Hearth::parse("12,-7"), Ok(Hearth { tile: (12, -7), players: 0 }));
        assert_eq!(Hearth::parse("12,-7:40"), Ok(Hearth { tile: (12, -7), players: 40 }));
        assert!(Hearth::parse("12").is_err() && Hearth::parse("a,b").is_err());
    }

    /// A scenario settles as the game would settle it, a hearth at a time:
    /// what stood apart is one once a hearth between joins it.
    #[test]
    fn a_scenario_settles_a_hearth_at_a_time() {
        let hearth = |tile, players| Hearth { tile, players };
        let hearths = [hearth((300, 0), 3), hearth((300, 40), 0), hearth((300, 24), 0)];
        let apart = settle((0, 0), &hearths, 2);
        assert_eq!(apart.each().count(), 2, "two outposts");
        let joined = settle((0, 0), &hearths, 3);
        assert_eq!(joined.each().count(), 1);
        assert_eq!(joined.size(HubId(300, 0)), 3);
        let fewer = [hearths.as_slice(), &[hearth((300, 0), 1)]].concat();
        let left = settle((0, 0), &fewer, 4);
        assert_eq!((left.each().count(), left.size(HubId(300, 0))), (2, 1), "named again, a hearth's players change, and the shrunk hub sheds its far hearth");
    }
}
