//! Which of an actor's clips plays: the walk or the run while it moves,
//! whichever its pace keeps the feet on the ground at, the idle while it
//! stands, a one-shot over either when the server says it used an
//! ability, held until the one-shot ends, and the jump in three parts
//! while the physics carries the actor through the air.

use std::{ collections::HashMap, time::Duration };

use bevy::{ animation::ActiveAnimation, gltf::GltfNode, prelude::* };
use qrz::Convert;
use serde::Deserialize;

use crate::components::*;
use common_bevy::{
    components::{ heading::Heading, position::VisualPosition, * },
    message::{ AbilityType, Do, Event },
    resources::map::Map,
    systems::movement::{ fall_time, standing_y },
};

/// An actor's clips, each found in its GLB by name: the rest pose, the
/// idle, the walk and the run, then the one-shots. An actor's asset holds
/// whichever it has; `Clips` says which.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Clip {
    Tee,
    Idle,
    Walk,
    Run,
    Back,
    Jump,
    Attack,
    Counter,
    Frenzy,
    Parry,
    Rattle,
    Disengage,
    Chop,
    Mine,
    Pickup,
}

impl Clip {
    pub const ALL: [Clip; 15] = [
        Clip::Tee, Clip::Idle, Clip::Walk, Clip::Run, Clip::Back, Clip::Jump, Clip::Attack, Clip::Counter,
        Clip::Frenzy, Clip::Parry, Clip::Rattle, Clip::Disengage,
        Clip::Chop, Clip::Mine, Clip::Pickup,
    ];

    /// The one-shots an ability plays, each held until it ends.
    const ONE_SHOTS: [Clip; 6] = [Clip::Attack, Clip::Counter, Clip::Frenzy, Clip::Parry, Clip::Rattle, Clip::Disengage];

    /// The cycles that cover ground ahead, slowest first.
    const GAITS: [Clip; 2] = [Clip::Walk, Clip::Run];

    /// Every cycle that declares the ground it covers: the gaits ahead and
    /// the one moving back against the facing.
    const STRIDED: [Clip; 3] = [Clip::Walk, Clip::Run, Clip::Back];

    /// The animation's name in the asset.
    pub fn name(self) -> &'static str {
        match self {
            Clip::Tee => "_tee",
            Clip::Idle => "idle",
            Clip::Walk => "walk",
            Clip::Run => "run",
            Clip::Back => "back",
            Clip::Jump => "jump",
            Clip::Attack => "attack",
            Clip::Counter => "counter",
            Clip::Frenzy => "frenzy",
            Clip::Parry => "parry",
            Clip::Rattle => "rattle",
            Clip::Disengage => "disengage",
            Clip::Chop => "chop",
            Clip::Mine => "mine",
            Clip::Pickup => "pickup",
        }
    }

    /// The clip what an actor does at a gather plays: its work's loop, or
    /// the stoop to its pile.
    pub fn of_gathering(activity: common::gathering::Activity) -> Clip {
        match activity {
            common::gathering::Activity::Work(common::gathering::Work::Chop) => Clip::Chop,
            common::gathering::Activity::Work(common::gathering::Work::Mine) => Clip::Mine,
            common::gathering::Activity::Pickup => Clip::Pickup,
        }
    }

    /// The one-shot an ability plays: a swing, an Overpower and a Punish the attack,
    /// a Frenzy its snap, a Feint the rattle, a Parry its sweep, a Counter the counter,
    /// a Leap the disengage, whichever way it goes. A Perfect Stride plays
    /// none: the gait it keeps shows it.
    pub fn of(ability: AbilityType) -> Option<Clip> {
        match ability {
            AbilityType::AutoAttack | AbilityType::Overpower | AbilityType::Punish => Some(Clip::Attack),
            AbilityType::Frenzy => Some(Clip::Frenzy),
            AbilityType::Feint => Some(Clip::Rattle),
            AbilityType::Parry => Some(Clip::Parry),
            AbilityType::Counter => Some(Clip::Counter),
            AbilityType::Leap => Some(Clip::Disengage),
            AbilityType::PerfectStride => None,
        }
    }

    /// What an actor whose asset lacks this one-shot plays instead: a
    /// Frenzy and a Feint fall back to the attack, a Parry to the counter.
    /// A Leap has none, drawn by its displacement alone.
    fn stand_in(self) -> Option<Clip> {
        match self {
            Clip::Frenzy | Clip::Rattle => Some(Clip::Attack),
            Clip::Parry => Some(Clip::Counter),
            _ => None,
        }
    }
}

/// The actor's GLB, held from spawn so the root asset — which names the
/// animations — is loaded and kept by the time its scene is ready; loaded
/// by its scene label alone, nothing holds the root and it can be gone.
#[derive(Component)]
pub struct Rig(pub Handle<Gltf>);

/// What a clip declares in the asset's extras, under `animgen` on the
/// armature node, keyed by the clip's name: its length in seconds, the
/// ground one cycle covers in the model's units — zero for a clip that
/// covers none — and a jump's three moments.
#[derive(Clone, Debug, Deserialize)]
struct Declaration {
    stride: Option<f32>,
    seconds: Option<f32>,
    flights: Option<Vec<[f32; 3]>>,
    leave: Option<f32>,
    freeze: Option<f32>,
    land: Option<f32>,
}

#[derive(Deserialize)]
struct Extras {
    animgen: HashMap<String, Declaration>,
}

/// A jump's three moments, seconds into its clip: when the body leaves its
/// stance, the pose held through the air, and when it takes the ground
/// back. The clip never rises; the physics carries the actor, and the clip
/// is played up to the freeze, held, then on to the end.
#[derive(Clone, Copy, Debug)]
pub struct Moments {
    pub leave: f32,
    pub freeze: f32,
    pub land: f32,
}

/// A cycle that covers ground: `length` in the model's units per cycle,
/// over `seconds` as authored, and each moment of it no foot is down,
/// seconds (start, top, end) — an end may run past the cycle's.
#[derive(Clone, Debug)]
pub struct Stride {
    pub length: f32,
    pub seconds: f32,
    pub flights: Vec<[f32; 3]>,
}

impl Stride {
    /// The playback rate that keeps the feet planted on ground passing at
    /// `speed` world units a second, under an actor drawn at `scale`.
    fn rate(&self, speed: f32, scale: f32) -> f32 {
        speed * self.seconds / (self.length * scale)
    }

    /// How long each flight's top is held when the ground asks `rate` of
    /// the gait: the time a contact played at `rate` saves on one played
    /// as authored, shared among the flights, so a cycle lasts as long as
    /// authored however fast the ground passes.
    fn hold(&self, rate: f32) -> f32 {
        let air: f32 = self.flights.iter().map(|[start, _, end]| end - start).sum();
        (self.seconds - air).max(0.0) * (1.0 - 1.0 / rate) / self.flights.len() as f32
    }

    /// The speed to play the gait at, `t` seconds into its cycle, when the
    /// ground asks `rate` of it, `dt` seconds on from the last. Faster
    /// than authored, a lofted gait keeps its footfall: a contact plays
    /// at `rate`, so the planted foot stays put, a flight as authored,
    /// and the top of each flight is held (`hold`) while the physics
    /// carries the actor on. A gait with no flight, or ground slower than
    /// its pace, plays at `rate` throughout.
    fn lope(&self, rate: f32, t: f32, dt: f32, lope: &mut Lope) -> f32 {
        if rate <= 1.0 || self.flights.is_empty() {
            *lope = Lope::default();
            return rate;
        }
        if lope.left > 0.0 {
            lope.left -= dt;
            return 0.0;
        }
        let within = |a: f32, b: f32| {
            let t = if t < a { t + self.seconds } else { t };
            a <= t && t < b
        };
        match self.flights.iter().position(|&[start, _, end]| within(start, end)) {
            None => {
                lope.held = None;
                rate
            }
            Some(i) => {
                let [_, top, end] = self.flights[i];
                if lope.held != Some(i) && within(top, end) {
                    lope.held = Some(i);
                    lope.left = self.hold(rate);
                    return 0.0;
                }
                1.0
            }
        }
    }
}

/// Where a lofted gait's holds stand for one actor: the flight last held,
/// so each top is held once a pass, and how long the hold has left.
#[derive(Component, Default, Debug)]
pub struct Lope {
    held: Option<usize>,
    left: f32,
}

/// How much nearer its authored pace the other gait must play before an
/// actor changes to it, as a ratio, so a pace between the two holds one.
const GAIT_SWITCH: f32 = 1.25;

/// The node in an actor's animation graph for each clip its asset holds,
/// the jump's moments and each gait's stride where it declares them, on
/// the entity with its `AnimationPlayer`; a clip the asset lacks has no
/// node.
#[derive(Component, Default)]
pub struct Clips {
    nodes: HashMap<Clip, AnimationNodeIndex>,
    jump: Option<Moments>,
    strides: HashMap<Clip, Stride>,
    freezes: HashMap<Clip, f32>,
}

impl Clips {
    /// Builds an actor's graph from the animations its GLB names, one node
    /// per clip it holds, and reads what its armature node declares.
    pub fn from_gltf(gltf: &Gltf, nodes: &Assets<GltfNode>) -> (AnimationGraph, Clips) {
        let mut graph = AnimationGraph::new();
        let mut clips = Clips::default();
        for clip in Clip::ALL {
            if let Some(handle) = gltf.named_animations.get(clip.name()) {
                clips.nodes.insert(clip, graph.add_clip(handle.clone(), 1.0, graph.root));
            }
        }
        let Some(declared) = gltf.nodes.iter()
            .filter_map(|h| nodes.get(h)?.extras.as_ref())
            .find_map(|extras| serde_json::from_str::<Extras>(&extras.value).ok())
        else {
            return (graph, clips);
        };
        clips.jump = declared.animgen.get(Clip::Jump.name())
            .and_then(|d| Some(Moments { leave: d.leave?, freeze: d.freeze?, land: d.land? }));
        for clip in Clip::ALL {
            if let Some(freeze) = declared.animgen.get(clip.name()).and_then(|d| d.freeze) {
                clips.freezes.insert(clip, freeze);
            }
        }
        for clip in Clip::STRIDED {
            let Some(d) = declared.animgen.get(clip.name()) else { continue };
            if let (Some(length), Some(seconds)) = (d.stride, d.seconds) {
                if length > 0.0 && seconds > 0.0 {
                    clips.strides.insert(clip, Stride { length, seconds, flights: d.flights.clone().unwrap_or_default() });
                }
            }
        }
        (graph, clips)
    }

    pub fn node(&self, clip: Clip) -> Option<AnimationNodeIndex> {
        self.nodes.get(&clip).copied()
    }

    /// The node an ability's `clip` plays at, the clip itself where the
    /// asset has it and its stand-in where not, with the clip it is.
    fn playing(&self, clip: Clip) -> Option<(Clip, AnimationNodeIndex)> {
        [Some(clip), clip.stand_in()].into_iter().flatten().find_map(|clip| Some((clip, self.node(clip)?)))
    }

    /// Whether `node` plays `clip`.
    pub fn is(&self, node: AnimationNodeIndex, clip: Clip) -> bool {
        self.node(clip) == Some(node)
    }

    /// The moment `clip` is held at, where it declares one.
    fn freeze(&self, clip: Clip) -> Option<f32> {
        self.freezes.get(&clip).copied()
    }

    /// The jump and its moments, where the asset has one and declares them.
    pub fn jump(&self) -> Option<(AnimationNodeIndex, Moments)> {
        Some((self.node(Clip::Jump)?, self.jump?))
    }

    /// Whether `node` plays a gait, ahead or back.
    fn is_gait(&self, node: AnimationNodeIndex) -> bool {
        Clip::STRIDED.iter().any(|&clip| self.is(node, clip))
    }

    /// The stride of the gait `node` plays, where the asset declares one.
    fn stride_of(&self, node: AnimationNodeIndex) -> Option<&Stride> {
        Clip::STRIDED.iter().find(|&&clip| self.is(node, clip)).and_then(|clip| self.strides.get(clip))
    }

    /// The gait an actor moving back against its facing at `speed` plays
    /// under `scale`, and its rate, where its asset has one that declares
    /// its stride; played forward, as authored.
    fn back(&self, speed: f32, scale: f32) -> Option<(AnimationNodeIndex, f32)> {
        Some((self.node(Clip::Back)?, self.strides.get(&Clip::Back)?.rate(speed, scale)))
    }

    /// The gait an actor covering ground at `speed` plays under `scale`,
    /// and its rate: of the gaits with a declared stride, the one whose
    /// rate lies nearest its authored pace, `current` held until another
    /// is nearer by [`GAIT_SWITCH`]. With none declared, the walk as
    /// authored.
    fn gait(&self, speed: f32, scale: f32, current: Option<AnimationNodeIndex>) -> Option<(AnimationNodeIndex, f32)> {
        // How far a rate lies from the authored pace, the current gait's
        // taken as nearer by the switch margin.
        let cost = |node, rate: f32| rate.ln().abs() - if Some(node) == current { GAIT_SWITCH.ln() } else { 0.0 };
        Clip::GAITS.iter()
            .filter_map(|clip| Some((self.node(*clip)?, self.strides.get(clip)?.rate(speed, scale))))
            .min_by(|a, b| cost(a.0, a.1).total_cmp(&cost(b.0, b.1)))
            .or_else(|| Some((self.node(Clip::Walk)?, 1.0)))
    }
}

/// Where an actor's jump clip is, on the actor in the air: rising to the
/// freeze; held there while the physics carries it; reaching for the
/// ground as the arc ends; caught at the landing pose until the ground
/// arrives; landed, playing out to the end.
#[derive(Component)]
pub struct Jumping(Phase);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Rising,
    Held,
    Reaching,
    Caught,
    Landed,
}

impl Jumping {
    /// Starts the jump: from the leave for a jump, which is still rising;
    /// straight to the held pose for a fall.
    fn start(anim: &mut ActiveAnimation, moments: Moments, rising: bool) -> Jumping {
        anim.set_speed(1.0);
        if rising {
            anim.set_seek_time(moments.leave);
            Jumping(Phase::Rising)
        } else {
            Self::hold(anim, moments.freeze);
            Jumping(Phase::Held)
        }
    }

    /// Holds the clip at `seconds`. By speed, not `pause`: a transition
    /// away from the clip — an ability played over it — fades a playing
    /// animation out and leaves a paused one in at full weight.
    fn hold(anim: &mut ActiveAnimation, seconds: f32) {
        anim.set_seek_time(seconds).set_speed(0.0);
    }

    /// Moves the clip on for this frame: `airborne` whether the physics
    /// still has the actor up, `to_land` how long its fall has left, if
    /// falling. Returns whether the jump is over.
    fn advance(&mut self, anim: &mut ActiveAnimation, moments: Moments, airborne: bool, to_land: Option<f32>) -> bool {
        if !airborne && self.0 != Phase::Landed {
            // The ground came: whatever was left before the landing pose
            // is skipped, and the clip plays out from there.
            anim.set_seek_time(anim.seek_time().max(moments.land)).set_speed(1.0);
            self.0 = Phase::Landed;
            return false;
        }
        match self.0 {
            Phase::Rising if anim.seek_time() >= moments.freeze => {
                Self::hold(anim, moments.freeze);
                self.0 = Phase::Held;
            }
            Phase::Held if to_land.is_some_and(|t| t <= moments.land - moments.freeze) => {
                anim.set_speed(1.0);
                self.0 = Phase::Reaching;
            }
            Phase::Reaching if anim.seek_time() >= moments.land => {
                Self::hold(anim, moments.land);
                self.0 = Phase::Caught;
            }
            Phase::Landed => return anim.is_finished(),
            _ => {}
        }
        false
    }
}

/// A swing that came in behind a leap's slide, played once the slide ends:
/// a Leap onto a target lands the swing due the frame after, and the
/// swing's clip would cut the flight off at once.
#[derive(Component)]
pub struct Held(Clip);

/// How long a one-shot blends in and the gaits and idle blend between.
const BLEND: Duration = Duration::from_millis(120);
const SETTLE: Duration = Duration::from_millis(300);

/// Plays the one-shot for each ability the server confirms, where the
/// actor's graph has that clip or its stand-in. A swing behind a leap's
/// slide waits for it to end (`Held`).
pub fn play_abilities(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    actors: Query<(&Animates, Has<Displacing>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
) {
    for message in reader.read() {
        let Do { event: Event::UseAbility { ent, ability, .. } } = message else { continue };
        let Some(clip) = Clip::of(*ability) else { continue };
        let Ok((animates, sliding)) = actors.get(*ent) else { continue };
        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let Some((clip, node)) = clips.playing(clip) else { continue };
        if sliding && *ability == AbilityType::AutoAttack && transitions.get_main_animation().is_some_and(|main| clips.is(main, Clip::Disengage)) {
            commands.entity(*ent).insert(Held(clip));
            continue;
        }
        transitions.play(&mut player, node, BLEND);
    }
}

/// Plays each held swing once its actor's slide has ended.
pub fn play_held(
    mut commands: Commands,
    actors: Query<(Entity, &Animates, &Held), Without<Displacing>>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
) {
    for (ent, animates, held) in &actors {
        commands.entity(ent).remove::<Held>();
        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let Some((_, node)) = clips.playing(held.0) else { continue };
        transitions.play(&mut player, node, BLEND).set_speed(1.);
    }
}

/// How long an actor's fall, `fallen_ms` old, has left, in seconds: it
/// falls onto the standing level of the tile under it.
fn time_to_land(world: Vec3, fallen_ms: f32, map: &Map) -> Option<f32> {
    let here: qrz::Qrz = map.convert(world);
    let (floor, _) = map.get_by_qr(here.q, here.r)?;
    Some(fall_time(world.y - standing_y(floor, map), fallen_ms) / 1000.0)
}

pub fn update(
    mut commands: Commands,
    mut query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading, &Transform, Option<&mut Jumping>, Option<&crate::systems::gathering::Gathering>, Option<&mut Lope>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
    map: Res<Map>,
    origin: Res<crate::resources::RenderOrigin>,
    time: Res<Time>,
) {
    for (entity, &airtime, &animates, vis_pos, &heading, transform, jumping, gathering, mut lope) in &mut query {
        // Entity is moving if VisualPosition is actively interpolating
        let travel = vis_pos.to - vis_pos.from;
        let is_moving = !vis_pos.is_complete() && travel.length_squared() > 0.001;
        // Every re-target starts from where the visual is, so in steady
        // motion it crosses each segment at the actor's own speed.
        let ground_speed = if vis_pos.duration > 0.0 { travel.xz().length() / vis_pos.duration } else { 0.0 };
        // Moving against the facing plays the gait in reverse.
        let direction = if travel.xz().dot(heading.to_world_dir()) < 0.0 { -1. } else { 1. };

        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let (Some(idle), Some(walk)) = (clips.node(Clip::Idle), clips.node(Clip::Walk)) else { continue };
        let main = transitions.get_main_animation();

        // The jump: started as the actor leaves the ground, held at its
        // freeze while the physics carries the actor, let reach for the
        // ground as the fall ends, and played out once the ground comes.
        // Anything else playing over it — an ability — ends it.
        let airborne = airtime.step.is_some();
        let jump = clips.jump();
        match (jump, jumping) {
            (Some((node, moments)), Some(mut jumping)) if main == Some(node) => {
                if airborne && jumping.0 == Phase::Landed {
                    let anim = transitions.play(&mut player, node, BLEND);
                    *jumping = Jumping::start(anim, moments, airtime.step.is_some_and(|ms| ms > 0));
                    continue;
                }
                // Falling, the airtime counts the fall's age below zero.
                let to_land = airtime.step.filter(|&ms| ms <= 0)
                    .and_then(|ms| time_to_land(origin.world(vis_pos.current()), -(ms as i32) as f32, &map));
                let Some(anim) = player.animation_mut(node) else { continue };
                if jumping.advance(anim, moments, airborne, to_land) {
                    commands.entity(entity).remove::<Jumping>();
                } else {
                    continue;
                }
            }
            (Some((node, moments)), None) if airborne => {
                let anim = transitions.play(&mut player, node, BLEND);
                commands.entity(entity).insert(Jumping::start(anim, moments, airtime.step.is_some_and(|ms| ms > 0)));
                continue;
            }
            (_, Some(_)) => {
                commands.entity(entity).remove::<Jumping>();
            }
            _ => {}
        }

        // An ability's one-shot holds the actor until it ends.
        if let Some(node) = main {
            let one_shot = Clip::ONE_SHOTS.iter().any(|&clip| clips.is(node, clip));
            if one_shot && player.animation(node).is_some_and(|a| !a.is_finished()) {
                continue;
            }
        }
        // At work, the actor loops its work's clip; stooped to its pile and
        // standing, it plays the pickup to its freeze and holds there by
        // speed. Either where its asset has the clip.
        let clip = gathering.map(|g| Clip::of_gathering(g.0));
        if let Some(node) = clip.filter(|&c| c != Clip::Pickup || !is_moving).and_then(|c| clips.node(c)) {
            if main != Some(node) {
                let anim = transitions.play(&mut player, node, SETTLE).set_speed(1.);
                if clip != Some(Clip::Pickup) {
                    anim.repeat();
                }
            } else if let (Some(freeze), Some(anim)) = (clip.and_then(|c| clips.freeze(c)), player.animation_mut(node)) {
                if anim.seek_time() >= freeze {
                    anim.set_seek_time(freeze).set_speed(0.0);
                }
            }
            continue;
        }
        // An actor with no jump keeps its gait in the air.
        if is_moving || (airborne && jump.is_none()) {
            let current = main.filter(|&node| clips.is_gait(node));
            // Moving back, an actor with a gait for it plays that forward;
            // one without plays its gait ahead in reverse.
            let back = if direction < 0.0 { clips.back(ground_speed, transform.scale.y) } else { None };
            let (gait, rate, direction) = match back {
                Some((node, rate)) => (node, rate, 1.0),
                None => {
                    let (gait, rate) = clips.gait(ground_speed, transform.scale.y, current).unwrap_or((walk, 1.0));
                    (gait, rate, direction)
                }
            };
            if main != Some(gait) {
                // The gaits all land the same foot at the start of their
                // cycle, so a change carries the cycle's phase across.
                let phase = current
                    .and_then(|node| Some(player.animation(node)?.seek_time() / clips.stride_of(node)?.seconds))
                    .map(|phase| phase.rem_euclid(1.0));
                let anim = transitions.play(&mut player, gait, SETTLE).set_speed(rate * direction).repeat();
                if let (Some(phase), Some(stride)) = (phase, clips.stride_of(gait)) {
                    anim.set_seek_time(phase * stride.seconds);
                }
            } else if let Some(anim) = player.animation_mut(gait) {
                // Forward, a lofted gait keeps its footfall and holds its
                // flights' tops to cover the rest (`Stride::lope`).
                let speed = match (clips.stride_of(gait), lope.as_deref_mut()) {
                    (Some(stride), Some(lope)) if direction > 0.0 =>
                        stride.lope(rate, anim.seek_time().rem_euclid(stride.seconds), time.delta_secs(), lope),
                    _ => rate * direction,
                };
                anim.set_speed(speed);
            }
            if lope.is_none() {
                commands.entity(entity).insert(Lope::default());
            }
        } else if main != Some(idle) {
            transitions.play(&mut player, idle, SETTLE).set_speed(1.).repeat();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lofted() -> Stride {
        Stride { length: 2.0, seconds: 1.0, flights: vec![[0.1, 0.25, 0.45], [0.6, 0.75, 0.95]] }
    }

    /// One cycle of `stride` played as the ground asks `rate` of it, in
    /// steps of `dt`: the real seconds it took, and the clip seconds it
    /// was held at inside a flight and outside one.
    fn cycle(stride: &Stride, rate: f32, dt: f32) -> (f32, f32, f32) {
        let (mut t, mut real, mut held_in, mut held_out) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut lope = Lope::default();
        while t < stride.seconds {
            let speed = stride.lope(rate, t, dt, &mut lope);
            if speed == 0.0 {
                let flying = stride.flights.iter().any(|&[a, _, b]| a <= t && t < b);
                if flying { held_in += dt } else { held_out += dt }
            }
            t += speed * dt;
            real += dt;
        }
        (real, held_in, held_out)
    }

    #[test]
    fn a_lofted_gait_keeps_its_footfall_however_fast_the_ground() {
        for rate in [1.5, 2.0, 4.0] {
            let (real, held_in, held_out) = cycle(&lofted(), rate, 0.0005);
            assert!((real - 1.0).abs() < 0.01, "at x{rate} a cycle took {real}s, not the authored second");
            assert!(held_in > 0.0, "at x{rate} no flight's top was held");
            assert_eq!(held_out, 0.0, "at x{rate} it held with a foot down");
        }
    }

    #[test]
    fn slower_than_authored_or_without_flight_it_plays_at_the_rate() {
        let mut lope = Lope::default();
        assert_eq!(lofted().lope(0.6, 0.3, 0.01, &mut lope), 0.6);
        let flat = Stride { flights: Vec::new(), ..lofted() };
        assert_eq!(flat.lope(3.0, 0.3, 0.01, &mut lope), 3.0);
    }
}
