//! The hubs view: a scenario of hearths named on the command line,
//! settled by the game's own rule (`common_bevy::hubs`) and drawn over
//! whatever is beneath. It reads no world: what ground a hub holds,
//! where its sites and its wall are and how far its influence reaches
//! are all asked of [`Hubs`], tile by tile, so the picture is the rule
//! and never a second one.

use common::network::Identity;
use common_bevy::{
    hubs::{HubId, Hubs, HAVEN_GROUND},
    plan::What,
    systems::When,
};

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

/// The hubs the first `count` of `hearths` make, laid in the order given
/// round the haven at `haven`, each with a hitching post raised. A
/// hearth's players each claim a home of its hub, which is planned for
/// two more residents and raises a home whenever it has none open. A
/// hearth named again is not laid again: the players at home in its hub
/// become the number it is named with, and with fewer the hub shrinks to
/// what they defend. One a hub's ground already holds, or would touch, is
/// not laid.
pub fn settle(haven: (i32, i32), hearths: &[Hearth], count: usize) -> Hubs {
    let mut hubs = Hubs::new(haven);
    let mut next = 0u32;
    let mut living: std::collections::HashMap<(i32, i32), Vec<Identity>> = std::collections::HashMap::new();
    for hearth in &hearths[..count.min(hearths.len())] {
        let tile = hearth.tile;
        if !living.contains_key(&tile) && hubs.of_hearth(tile).is_none() {
            if !hubs.founds(tile) {
                log::warn!("  a hub holds the ground the hearth at {:?} would: it is not laid", hearth.tile);
                continue;
            }
            let id = hubs.founded(tile);
            if let Some(plan) = hubs.raised(id, What::Post) {
                hubs.carry(&plan);
            }
        }
        let Some(id) = hubs.of_hearth(tile) else { continue };
        let at = living.entry(tile).or_default();
        let fewer = at.len() > hearth.players as usize;
        while at.len() > hearth.players as usize {
            hubs.gone(at.pop().expect("more than none"));
        }
        while at.len() < hearth.players as usize {
            let open = hubs.items(id).iter().filter(|item| item.kind.what == What::Home).map(|item| item.tiles[0]).find(|&home| hubs.open(home).is_some());
            let Some(home) = open else {
                // No home has room: one more is raised, on ground planned for two more where it has none
                let Some(plan) = hubs.raised(id, What::Home).or_else(|| hubs.planned(id, hubs.wants(id) + 2)) else { break };
                hubs.carry(&plan);
                continue;
            };
            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&next.to_le_bytes());
            next += 1;
            hubs.resident(Identity::Key(key), Some(home));
            at.push(Identity::Key(key));
        }
        if let Some(plan) = fewer.then(|| hubs.shrunk(id)).flatten() {
            hubs.carry(&plan);
        }
    }
    // Drawn as the country has given way to them: aged from the
    // calendar's first moment to one past any hub's last level
    hubs.age(When(0), 1);
    hubs.age(When(u64::MAX / 2), 1);
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
}

/// What a pixel's tile is to the hubs: the hub whose ground holds it,
/// which hubs' influence reaches it, and whether the haven's guard keeps
/// it.
#[derive(Clone, PartialEq)]
struct Read {
    held: Option<HubId>,
    reached: Vec<HubId>,
    guarded: bool,
}

/// Draws `hubs` over `canvas`: every hub's influence as a wash, whiter
/// the stronger, as it is once the country has given way to it; a hub's
/// ground filled in its hue and outlined, which is its wall; its reach
/// outlined thin; a dot at each of its sites, white for a home a player
/// lives in, grey for any other, and a larger one in its hue at its
/// hearth. The haven is gold: its guarded ground outlined, a ringed dot
/// at its point.
pub fn draw(canvas: &mut Canvas, hubs: &Hubs, _hearths: &[Hearth]) {
    let standing: Vec<(HubId, (i32, i32), u32)> = hubs.each().collect();
    let place = |id: HubId| standing.iter().position(|hub| hub.0 == id).unwrap_or(0);
    let read = |tile: (i32, i32)| Read {
        held: hubs.holding(tile),
        reached: standing.iter().filter(|hub| hubs.reaches(hub.0, tile)).map(|hub| hub.0).collect(),
        guarded: hubs.guarded(tile),
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
            if edge(&|other| other.held != here.held) {
                let id = here.held.or(beside.iter().flatten().find_map(|other| other.held));
                canvas.blend(px, py, id.map_or([1.0; 3], |id| hue(place(id))), 1.0);
            }
        }
    }
    let size = (2.0 / canvas.scale).clamp(2.0, 6.0) as i32;
    for &(id, hearth, _) in &standing {
        for item in hubs.items(id).iter().filter(|item| item.kind.what != What::Hearth) {
            for &tile in &item.tiles {
                canvas.dot(tile, size, if item.players > 0 { [1.0, 1.0, 1.0] } else { [0.55, 0.55, 0.55] });
            }
        }
        canvas.dot(hearth, size + 2, [0.05, 0.05, 0.05]);
        canvas.dot(hearth, size + 1, hue(place(id)));
    }
    canvas.dot(hubs.haven(), size + 2, [0.05, 0.05, 0.05]);
    canvas.dot(hubs.haven(), size + 1, GOLD);
}

/// Says how every hub of `hubs` stands.
pub fn report(hubs: &Hubs) {
    log::info!("  the haven: at {:?}, guarding {HAVEN_GROUND:.0} tiles", hubs.haven());
    for (id, hearth, size) in hubs.each() {
        log::info!(
            "  {id:?}: hearth {hearth:?}, planned for {}, {} things, a wall of {}, {size} players who defend {}, influence {:.2} reaching {:.0} tiles",
            hubs.wants(id),
            hubs.items(id).len(),
            hubs.ground(id).map_or(0, |ground| ground.wall.len()),
            hubs.held(id),
            hubs.strength(id),
            hubs.reach(id)
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
    /// a hub grows to house its players, and shrinks when it is named
    /// again with fewer.
    #[test]
    fn a_scenario_settles_a_hearth_at_a_time() {
        let hearth = |tile, players| Hearth { tile, players };
        let hearths = [hearth((300, 0), 1), hearth((330, 0), 0), hearth((300, 0), 9), hearth((300, 0), 1)];
        let apart = settle((0, 0), &hearths, 2);
        assert_eq!((apart.each().count(), apart.size(HubId(300, 0))), (2, 1), "two camps");
        let grown = settle((0, 0), &hearths, 3);
        assert_eq!(grown.size(HubId(300, 0)), 9, "grown to house nine");
        assert!(grown.wants(HubId(300, 0)) > apart.wants(HubId(300, 0)));
        let shrunk = settle((0, 0), &hearths, 4);
        assert_eq!(shrunk.size(HubId(300, 0)), 1);
        assert!(shrunk.ground(HubId(300, 0)).unwrap().wall.len() < grown.ground(HubId(300, 0)).unwrap().wall.len());
    }
}
