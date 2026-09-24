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
    Jump,
    Attack,
    Counter,
}

impl Clip {
    pub const ALL: [Clip; 7] = [Clip::Tee, Clip::Idle, Clip::Walk, Clip::Run, Clip::Jump, Clip::Attack, Clip::Counter];

    /// The cycles that cover ground, slowest first.
    const GAITS: [Clip; 2] = [Clip::Walk, Clip::Run];

    /// The animation's name in the asset.
    pub fn name(self) -> &'static str {
        match self {
            Clip::Tee => "_tee",
            Clip::Idle => "idle",
            Clip::Walk => "walk",
            Clip::Run => "run",
            Clip::Jump => "jump",
            Clip::Attack => "attack",
            Clip::Counter => "counter",
        }
    }

    /// The one-shot an ability plays, if any: a strike for whatever hits,
    /// the counter for whatever wards.
    pub fn of(ability: AbilityType) -> Option<Clip> {
        match ability {
            AbilityType::AutoAttack | AbilityType::Overpower | AbilityType::Lunge | AbilityType::Kick | AbilityType::Volley => Some(Clip::Attack),
            AbilityType::Counter | AbilityType::Deflect => Some(Clip::Counter),
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
#[derive(Clone, Copy, Debug, Deserialize)]
struct Declaration {
    stride: Option<f32>,
    seconds: Option<f32>,
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
/// over `seconds` as authored.
#[derive(Clone, Copy, Debug)]
pub struct Stride {
    pub length: f32,
    pub seconds: f32,
}

impl Stride {
    /// The playback rate that keeps the feet planted on ground passing at
    /// `speed` world units a second, under an actor drawn at `scale`.
    fn rate(self, speed: f32, scale: f32) -> f32 {
        speed * self.seconds / (self.length * scale)
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
        for clip in Clip::GAITS {
            let Some(d) = declared.animgen.get(clip.name()) else { continue };
            if let (Some(length), Some(seconds)) = (d.stride, d.seconds) {
                if length > 0.0 && seconds > 0.0 {
                    clips.strides.insert(clip, Stride { length, seconds });
                }
            }
        }
        (graph, clips)
    }

    pub fn node(&self, clip: Clip) -> Option<AnimationNodeIndex> {
        self.nodes.get(&clip).copied()
    }

    /// Whether `node` plays `clip`.
    pub fn is(&self, node: AnimationNodeIndex, clip: Clip) -> bool {
        self.node(clip) == Some(node)
    }

    /// The jump and its moments, where the asset has one and declares them.
    pub fn jump(&self) -> Option<(AnimationNodeIndex, Moments)> {
        Some((self.node(Clip::Jump)?, self.jump?))
    }

    /// Whether `node` plays a gait.
    fn is_gait(&self, node: AnimationNodeIndex) -> bool {
        Clip::GAITS.iter().any(|&clip| self.is(node, clip))
    }

    /// The stride of the gait `node` plays, where the asset declares one.
    fn stride_of(&self, node: AnimationNodeIndex) -> Option<Stride> {
        Clip::GAITS.iter().find(|&&clip| self.is(node, clip)).and_then(|clip| self.strides.get(clip).copied())
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

/// How long a one-shot blends in and the gaits and idle blend between.
const BLEND: Duration = Duration::from_millis(120);
const SETTLE: Duration = Duration::from_millis(300);

/// Plays the one-shot for each ability the server confirms, where the
/// actor's graph has that clip.
pub fn play_abilities(
    mut reader: MessageReader<Do>,
    actors: Query<&Animates>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
) {
    for message in reader.read() {
        let Do { event: Event::UseAbility { ent, ability, .. } } = message else { continue };
        let Some(clip) = Clip::of(*ability) else { continue };
        let Ok(animates) = actors.get(*ent) else { continue };
        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let Some(node) = clips.node(clip) else { continue };
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
    mut query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading, &Transform, Option<&mut Jumping>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
    map: Res<Map>,
    origin: Res<crate::resources::RenderOrigin>,
) {
    for (entity, &airtime, &animates, vis_pos, &heading, transform, jumping) in &mut query {
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
            let one_shot = clips.is(node, Clip::Attack) || clips.is(node, Clip::Counter);
            if one_shot && player.animation(node).is_some_and(|a| !a.is_finished()) {
                continue;
            }
        }
        // An actor with no jump keeps its gait in the air.
        if is_moving || (airborne && jump.is_none()) {
            let current = main.filter(|&node| clips.is_gait(node));
            let (gait, rate) = clips.gait(ground_speed, transform.scale.y, current).unwrap_or((walk, 1.0));
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
                anim.set_speed(rate * direction);
            }
        } else if main != Some(idle) {
            transitions.play(&mut player, idle, SETTLE).set_speed(1.).repeat();
        }
    }
}
