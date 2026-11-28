//! Which of an actor's clips plays: the walk or the run while it moves,
//! whichever its pace keeps the feet on the ground at, the idle while it
//! stands, the swim and the tread for each afloat, a one-shot over either as the server says it began a skill
//! or swung, held until the one-shot ends, and the jump in three parts
//! while the physics carries the actor through the air. A one-shot that
//! declares its strike is played against the server's clock, so the blow
//! is drawn as the activation ends (`Swing`).

use std::{ collections::HashMap, time::Duration };

use bevy::{ animation::ActiveAnimation, gltf::GltfNode, prelude::* };
use serde::Deserialize;

use crate::components::*;
use common_bevy::{
    components::{ entity_type::{ actor::ActorIdentity, EntityType }, heading::Heading, position::{ Position, VisualPosition }, status::Status, * },
    message::{ AbilityType, Do, Event, Outcome },
    moment::Moment,
    resources::map::Map,
    systems::movement::{ fall_time, support_y_from, swimming },
};

/// An actor's clips, each found in its GLB by name: the rest pose, the
/// idle, the walk and the run, the swim and the tread afloat, then the
/// one-shots. An actor's asset holds
/// whichever it has; `Clips` says which.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Clip {
    Tee,
    Idle,
    Walk,
    Run,
    Back,
    /// Afloat and moving: a cycle that declares the water it covers.
    Swim,
    /// Afloat and still.
    Tread,
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
    /// Busy with its hands at whatever it is raising.
    Work,
    /// Asleep on the ground.
    Sleep,
    Mounted,
}

impl Clip {
    pub const ALL: [Clip; 20] = [
        Clip::Tee, Clip::Idle, Clip::Walk, Clip::Run, Clip::Back, Clip::Swim, Clip::Tread, Clip::Jump, Clip::Attack, Clip::Counter,
        Clip::Frenzy, Clip::Parry, Clip::Rattle, Clip::Disengage,
        Clip::Chop, Clip::Mine, Clip::Pickup, Clip::Work, Clip::Sleep, Clip::Mounted,
    ];

    /// The one-shots an ability plays, each held until it ends.
    const ONE_SHOTS: [Clip; 6] = [Clip::Attack, Clip::Counter, Clip::Frenzy, Clip::Parry, Clip::Rattle, Clip::Disengage];

    /// The cycles that cover ground ahead, slowest first.
    const GAITS: [Clip; 2] = [Clip::Walk, Clip::Run];

    /// Every cycle that declares the ground it covers: the gaits ahead, the
    /// one moving back against the facing, and the swim.
    const STRIDED: [Clip; 4] = [Clip::Walk, Clip::Run, Clip::Back, Clip::Swim];

    /// The animation's name in the asset.
    pub fn name(self) -> &'static str {
        match self {
            Clip::Tee => "_tee",
            Clip::Idle => "idle",
            Clip::Walk => "walk",
            Clip::Run => "run",
            Clip::Back => "back",
            Clip::Swim => "swim",
            Clip::Tread => "tread",
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
            Clip::Work => "work",
            Clip::Sleep => "sleep",
            Clip::Mounted => "mounted",
        }
    }

    /// The clip what an actor does at a gather plays: its work's loop, or
    /// the stoop to its pile; none on the road, where it walks.
    pub fn of_gathering(activity: common::gathering::Activity) -> Option<Clip> {
        Some(match activity {
            common::gathering::Activity::Haul => return None,
            common::gathering::Activity::Work(common::gathering::Work::Chop) => Clip::Chop,
            common::gathering::Activity::Work(common::gathering::Work::Mine) => Clip::Mine,
            common::gathering::Activity::Work(common::gathering::Work::Build) => Clip::Work,
            common::gathering::Activity::Pickup => Clip::Pickup,
            common::gathering::Activity::Sleep => Clip::Sleep,
        })
    }

    /// The one-shot an ability plays: a swing, an Overpower and a Punish the attack,
    /// a Frenzy its snap, a Feint the rattle, a Parry its sweep, a Counter the counter,
    /// a Leap the disengage, whichever way it goes, a rune cut ahead the
    /// attack. A Perfect Stride plays none: the gait it keeps shows it.
    pub fn of(ability: AbilityType) -> Option<Clip> {
        match ability {
            AbilityType::AutoAttack | AbilityType::Overpower | AbilityType::Punish | AbilityType::Backstab
            | AbilityType::Kick | AbilityType::Defy | AbilityType::Drag => Some(Clip::Attack),
            AbilityType::Frenzy => Some(Clip::Frenzy),
            AbilityType::Feint => Some(Clip::Rattle),
            AbilityType::Parry => Some(Clip::Parry),
            AbilityType::Counter => Some(Clip::Counter),
            AbilityType::Leap => Some(Clip::Disengage),
            AbilityType::PerfectStride | AbilityType::Bloodlust | AbilityType::Brace | AbilityType::Surge | AbilityType::Vigil | AbilityType::Lurk => None,
            AbilityType::Clash => Some(Clip::Parry),
            AbilityType::Sidestep | AbilityType::Pounce => Some(Clip::Disengage),
            AbilityType::Yank => Some(Clip::Counter),
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
/// covers none — a jump's three moments, and the moment a blow meets what
/// it hits.
#[derive(Clone, Debug, Deserialize)]
struct Declaration {
    stride: Option<f32>,
    seconds: Option<f32>,
    flights: Option<Vec<[f32; 2]>>,
    leave: Option<f32>,
    freeze: Option<f32>,
    land: Option<f32>,
    strike: Option<f32>,
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
/// seconds (start, end) — an end may run past the cycle's.
#[derive(Clone, Debug)]
pub struct Stride {
    pub length: f32,
    pub seconds: f32,
    pub flights: Vec<[f32; 2]>,
}

impl Stride {
    /// The playback rate that keeps the feet planted on ground passing at
    /// `speed` world units a second, under an actor drawn at `scale`.
    fn rate(&self, speed: f32, scale: f32) -> f32 {
        speed * self.seconds / (self.length * scale)
    }

    /// The speed to play the gait at, `t` seconds into its cycle, when the
    /// ground asks `rate` of it. Faster than authored, a lofted gait keeps
    /// its footfall and its cycle: a contact plays at `rate`, so the
    /// planted foot stays put, and every flight slower than authored by
    /// the time that saves, so a cycle lasts as authored while the physics
    /// carries the actor on through the air. A gait with no flight, or
    /// ground slower than its pace, plays at `rate` throughout.
    fn lope(&self, rate: f32, t: f32) -> f32 {
        if rate <= 1.0 || self.flights.is_empty() {
            return rate;
        }
        let flying = self.flights.iter().any(|&[start, end]| {
            let t = if t < start { t + self.seconds } else { t };
            t < end
        });
        if !flying {
            return rate;
        }
        let air: f32 = self.flights.iter().map(|[start, end]| end - start).sum();
        air / (self.seconds - (self.seconds - air) / rate)
    }
}

/// The slowest and the fastest a clip's wind-up plays against its authored
/// rate. An activation longer than the slowest fills starts the clip late;
/// one shorter than the fastest fills starts it part-way in.
const SLOWEST: f32 = 0.5;
const FASTEST: f32 = 2.0;

/// How a clip that strikes `strike` seconds in is played against an
/// activation, so the strike falls as the activation ends: its wind-up at
/// `rate`, the clip's time a function of the time the activation has left
/// alone, so a client that hears of it late draws the same pose.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Swing {
    strike: f32,
    rate: f32,
}

impl Swing {
    /// The swing for an activation `length` seconds long: the rate that
    /// fills it with the wind-up, held within the band.
    fn new(strike: f32, length: f32) -> Swing {
        let rate = if length > 0.0 { (strike / length).clamp(SLOWEST, FASTEST) } else { FASTEST };
        Swing { strike, rate }
    }

    /// Where the clip stands with `left` seconds of the activation to
    /// run, None while it has yet to start.
    fn at(&self, left: f32) -> Option<f32> {
        let seconds = self.strike - left * self.rate;
        (seconds >= 0.0).then_some(seconds)
    }
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
    strikes: HashMap<Clip, f32>,
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
        clips.declare(&declared);
        (graph, clips)
    }

    /// Takes what the asset declares of each clip.
    fn declare(&mut self, declared: &Extras) {
        let clips = self;
        clips.jump = declared.animgen.get(Clip::Jump.name())
            .and_then(|d| Some(Moments { leave: d.leave?, freeze: d.freeze?, land: d.land? }));
        for clip in Clip::ALL {
            let Some(d) = declared.animgen.get(clip.name()) else { continue };
            if let Some(freeze) = d.freeze {
                clips.freezes.insert(clip, freeze);
            }
            if let Some(strike) = d.strike {
                clips.strikes.insert(clip, strike);
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

    /// The moment `clip`'s blow meets what it hits, where it declares one.
    fn strike(&self, clip: Clip) -> Option<f32> {
        self.strikes.get(&clip).copied()
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

    /// The swim of an actor afloat covering water at `speed` under
    /// `scale`, and its rate, where its asset has one: at the rate its
    /// declared stride asks, else as authored. Moving back, it plays in
    /// reverse.
    fn swim(&self, speed: f32, scale: f32) -> Option<(AnimationNodeIndex, f32)> {
        Some((self.node(Clip::Swim)?, self.strides.get(&Clip::Swim).map_or(1.0, |stride| stride.rate(speed, scale))))
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

/// A skill's swing under way: the clip winding up to the moment its
/// activation ends.
struct WindUp {
    ability: AbilityType,
    ends: Moment,
    node: AnimationNodeIndex,
    swing: Swing,
    /// Whether the clip has begun: an activation longer than its wind-up
    /// fills at the slowest rate starts it late.
    started: bool,
}

impl WindUp {
    /// Draws the clip where it stands with `left` seconds of the
    /// activation to run. Returns whether the wind-up is over: the blow
    /// landed and the follow-through playing as authored, or the body
    /// taken by something else.
    fn advance(&mut self, left: f32, player: &mut AnimationPlayer, transitions: &mut AnimationTransitions) -> bool {
        if self.started && transitions.get_main_animation() != Some(self.node) {
            return true;
        }
        let Some(seconds) = self.swing.at(left) else { return false };
        if !self.started {
            transitions.play(player, self.node, BLEND);
            self.started = true;
        }
        let Some(anim) = player.animation_mut(self.node) else { return true };
        let landed = left <= 0.0;
        anim.set_seek_time(seconds).set_speed(if landed { 1.0 } else { self.swing.rate });
        landed
    }
}

/// Every wind-up under way, by the actor swinging.
#[derive(Default)]
pub struct WindUps(HashMap<Entity, WindUp>);

/// Plays `clip` at `node` for a blow that has landed: from its strike
/// where it declares one, from its start where not.
fn play_landed(player: &mut AnimationPlayer, transitions: &mut AnimationTransitions, clips: &Clips, clip: Clip, node: AnimationNodeIndex) {
    let anim = transitions.play(player, node, BLEND).set_speed(1.0);
    if let Some(strike) = clips.strike(clip) {
        anim.set_seek_time(strike);
    }
}

/// Plays a skill's one-shot against its activation, and a swing's as it
/// resolves, an auto-attack having no activation, where the actor's graph
/// has that clip or its stand-in. A clip that declares its strike winds
/// up so the strike falls as the activation ends (`Swing`), and blends
/// back to the idle at once where the activation is interrupted or
/// cancelled; an auto-attack's starts at its strike, the blow having
/// landed. A clip declaring none plays from its start as authored. A swing
/// behind a leap's slide waits for it to end (`Held`). An activation told
/// again starts nothing (`Heard`).
pub fn play_abilities(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    actors: Query<(&Animates, Has<Displacing>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
    mut heard: Local<crate::systems::combat::Heard>,
    mut winding: Local<WindUps>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let (ent, ability, activation) = match message.event {
            Event::Activate { ent, activation } if heard.fresh(ent, &activation) => (ent, activation.ability, Some(activation)),
            Event::Resolved { ent, ability: AbilityType::AutoAttack, .. } => (ent, AbilityType::AutoAttack, None),
            Event::Resolved { ent, ability, outcome: Outcome::Interrupted | Outcome::Cancelled, .. } => {
                if winding.0.get(&ent).is_some_and(|wound| wound.ability == ability) {
                    let wound = winding.0.remove(&ent).expect("found above");
                    let Ok((animates, _)) = actors.get(ent) else { continue };
                    let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
                    if let Some(idle) = clips.node(Clip::Idle).filter(|_| wound.started && transitions.get_main_animation() == Some(wound.node)) {
                        transitions.play(&mut player, idle, BLEND).set_speed(1.).repeat();
                    }
                }
                continue;
            }
            _ => continue,
        };
        let Some(clip) = Clip::of(ability) else { continue };
        let Ok((animates, sliding)) = actors.get(ent) else { continue };
        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let Some((clip, node)) = clips.playing(clip) else { continue };
        if sliding && ability == AbilityType::AutoAttack && transitions.get_main_animation().is_some_and(|main| clips.is(main, Clip::Disengage)) {
            commands.entity(ent).insert(Held(clip));
            continue;
        }
        match (activation, clips.strike(clip)) {
            (Some(activation), Some(strike)) => {
                let swing = Swing::new(strike, activation.length.as_secs_f32());
                winding.0.insert(ent, WindUp { ability, ends: activation.ends(), node, swing, started: false });
            }
            (Some(_), None) => {
                transitions.play(&mut player, node, BLEND).set_speed(1.0);
            }
            (None, _) => play_landed(&mut player, &mut transitions, clips, clip, node),
        }
    }

    let now = server.now(time.elapsed().as_millis());
    winding.0.retain(|&ent, wound| {
        let Ok((animates, _)) = actors.get(ent) else { return false };
        let Ok((mut player, mut transitions, _)) = q_anim.get_mut(animates.0) else { return false };
        !wound.advance(now.until(wound.ends).as_secs_f32(), &mut player, &mut transitions)
    });
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
        let Some((clip, node)) = clips.playing(held.0) else { continue };
        play_landed(&mut player, &mut transitions, clips, clip, node);
    }
}

/// How long an actor's fall, `fallen_ms` old, has left, in seconds: it
/// falls onto the standing level of the tile under it.
fn time_to_land(at: Vec3, fallen_ms: f32, draft: f32, map: &Map, origin: &crate::resources::RenderOrigin) -> Option<f32> {
    let here = origin.tile_at(map, at);
    let (floor, _) = map.get_by_qr(here.q, here.r)?;
    Some(fall_time(at.y - support_y_from(origin.tile(), at.xz(), floor, draft, map), fallen_ms) / 1000.0)
}

pub fn update(
    mut commands: Commands,
    mut query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading, &Transform, Option<&mut Jumping>, Option<&crate::systems::gathering::Gathering>, Option<&crate::systems::mount::Mount>, Option<&EntityType>, Option<&Status>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
    map: Res<Map>,
    origin: Res<crate::resources::RenderOrigin>,
) {
    for (entity, &airtime, &animates, vis_pos, &heading, transform, jumping, gathering, mount, typ, status) in &mut query {
        // Entity is moving if VisualPosition is actively interpolating
        let travel = vis_pos.to - vis_pos.from;
        let is_moving = !vis_pos.is_complete() && travel.length_squared() > 0.001;
        // Every re-target starts from where the visual is, so in steady
        // motion it crosses each segment at the actor's own speed.
        let ground_speed = if vis_pos.duration > 0.0 { travel.xz().length() / vis_pos.duration } else { 0.0 };
        // Moving against the facing plays the gait in reverse.
        let direction = if travel.xz().dot(heading.to_world_dir()) < 0.0 { -1. } else { 1. };
        // Afloat where it is drawn, on its own body or its horse's. The
        // drawn point is taken in the render origin's frame, exact
        // however far out.
        let draft = ActorIdentity::draft_of(typ, status);
        let afloat = swimming(Position::new(origin.tile(), vis_pos.current()), draft, &map);

        // A rider's horse carries the gaits and the jump.
        let driven = mount.and_then(|m| m.driven()).unwrap_or(animates.0);
        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(driven) else { continue };
        let Some(idle) = clips.node(Clip::Idle) else { continue };
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
                    .and_then(|ms| time_to_land(vis_pos.current(), -(ms as i32) as f32, draft, &map, &origin));
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
        let clip = gathering.and_then(|g| Clip::of_gathering(g.0));
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
            // Afloat it swims, where its asset has a swim; moving back, an
            // actor with a gait for it plays that forward, and one without
            // plays its gait ahead, or its swim, in reverse.
            let swim = if afloat { clips.swim(ground_speed, transform.scale.y) } else { None };
            let back = if direction < 0.0 && swim.is_none() { clips.back(ground_speed, transform.scale.y) } else { None };
            let (gait, rate, direction) = match swim.or(back).or_else(|| clips.gait(ground_speed, transform.scale.y, current)) {
                Some((node, rate)) if back.is_some() => (node, rate, 1.0),
                Some((node, rate)) => (node, rate, direction),
                None => continue,
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
                // Forward, a lofted gait keeps its footfall and slows its
                // flights to cover the rest (`Stride::lope`).
                let speed = match clips.stride_of(gait) {
                    Some(stride) if direction > 0.0 => stride.lope(rate, anim.seek_time().rem_euclid(stride.seconds)),
                    _ => rate * direction,
                };
                anim.set_speed(speed);
            }
        } else {
            // Afloat and still it treads the water, where its asset can.
            let rest = clips.node(Clip::Tread).filter(|_| afloat).unwrap_or(idle);
            if main != Some(rest) {
                transitions.play(&mut player, rest, SETTLE).set_speed(1.).repeat();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lofted() -> Stride {
        Stride { length: 2.0, seconds: 1.0, flights: vec![[0.1, 0.45], [0.6, 0.95]] }
    }

    /// One cycle of `stride` played as the ground asks `rate` of it, in
    /// steps of `dt`: the real seconds it took, and the least speed it
    /// played at with a foot down.
    fn cycle(stride: &Stride, rate: f32, dt: f32) -> (f32, f32) {
        let (mut t, mut real, mut grounded) = (0.0f32, 0.0f32, f32::INFINITY);
        while t < stride.seconds {
            let speed = stride.lope(rate, t);
            if !stride.flights.iter().any(|&[a, b]| a <= t && t < b) {
                grounded = grounded.min(speed);
            }
            t += speed * dt;
            real += dt;
        }
        (real, grounded)
    }

    /// How long before the activation's end the clip starts, and where in
    /// it, stepping the time left down from `length`.
    fn start(swing: &Swing, length: f32) -> (f32, f32) {
        let mut left = length;
        loop {
            if let Some(seconds) = swing.at(left) {
                return (left, seconds);
            }
            left -= 0.001;
        }
    }

    #[test]
    fn inside_the_band_the_wind_up_fills_the_activation_and_strikes_at_its_end() {
        let strike = 0.6;
        for length in [0.3, 0.45, 0.6, 0.9, 1.2] {
            let swing = Swing::new(strike, length);
            let (left, seconds) = start(&swing, length);
            assert!(length - left < 0.01 && seconds < 0.01, "a {length}s activation starts its clip {left}s out, {seconds}s in");
            assert_eq!(swing.at(0.0), Some(strike), "a {length}s activation ends off its strike");
            let mut last = -1.0;
            for step in (0..10).rev() {
                let seconds = swing.at(length * step as f32 / 10.0).expect("under way");
                assert!(seconds >= last, "the clip ran back");
                last = seconds;
            }
        }
        assert!(Swing::new(strike, 0.4).rate > Swing::new(strike, 0.8).rate, "a shorter activation winds up no faster than a longer");
    }

    #[test]
    fn a_long_activation_starts_its_clip_late_and_never_crawls() {
        let strike = 0.6;
        let slowest = Swing::new(strike, strike / SLOWEST);
        for length in [1.5, 3.0, 10.0] {
            let swing = Swing::new(strike, length);
            assert_eq!(swing.rate, slowest.rate, "a {length}s activation crawls");
            assert_eq!(swing.at(length), None, "a {length}s activation starts at once");
            let (left, seconds) = start(&swing, length);
            assert!(seconds < 0.01 && (left - start(&slowest, strike / SLOWEST).0).abs() < 0.01, "it starts {left}s out, {seconds}s in");
            assert_eq!(swing.at(0.0), Some(strike));
        }
    }

    #[test]
    fn a_short_activation_starts_its_clip_part_way_in_and_never_blurs() {
        let strike = 0.6;
        let mut last = 0.0;
        for length in [0.25, 0.1, 0.0] {
            let swing = Swing::new(strike, length);
            assert_eq!(swing.rate, Swing::new(strike, strike / FASTEST).rate, "a {length}s activation blurs");
            let seconds = swing.at(length).expect("it starts at once");
            assert!(seconds > last && seconds <= strike, "a {length}s activation starts {seconds}s in");
            last = seconds;
            assert_eq!(swing.at(0.0), Some(strike));
        }
        assert_eq!(last, strike, "an activation with no length shows the strike");
    }

    #[test]
    fn a_clip_declares_its_strike_or_plays_as_authored() {
        let declared: Extras = serde_json::from_str(
            r#"{"animgen":{"attack":{"stride":0.0,"seconds":1.25,"strike":0.55},"counter":{"stride":0.0,"seconds":1.5}}}"#,
        ).expect("the extras parse");
        let mut clips = Clips::default();
        clips.declare(&declared);
        assert_eq!(clips.strike(Clip::Attack), Some(0.55));
        assert_eq!(clips.strike(Clip::Counter), None);
        assert_eq!(clips.strike(Clip::Frenzy), None);
    }

    #[test]
    fn a_lofted_gait_keeps_its_footfall_however_fast_the_ground() {
        for rate in [1.5, 2.0, 4.0] {
            let (real, grounded) = cycle(&lofted(), rate, 0.0005);
            assert!((real - 1.0).abs() < 0.01, "at x{rate} a cycle took {real}s, not the authored second");
            assert_eq!(grounded, rate, "at x{rate} a planted foot played at {grounded}");
        }
    }

    #[test]
    fn a_flight_never_stops() {
        for rate in [1.5, 2.0, 4.0, 16.0] {
            assert!(lofted().lope(rate, 0.3) > 0.0, "at x{rate} the flight stopped");
        }
    }

    #[test]
    fn a_flight_running_past_the_cycle_plays_on_into_the_next() {
        let wrapped = Stride { length: 2.0, seconds: 1.0, flights: vec![[0.8, 1.1]] };
        assert_eq!(wrapped.lope(2.0, 0.05), wrapped.lope(2.0, 0.9));
        assert_eq!(wrapped.lope(2.0, 0.5), 2.0);
    }

    #[test]
    fn slower_than_authored_or_without_flight_it_plays_at_the_rate() {
        assert_eq!(lofted().lope(0.6, 0.3), 0.6);
        let flat = Stride { flights: Vec::new(), ..lofted() };
        assert_eq!(flat.lope(3.0, 0.3), 3.0);
    }
}
