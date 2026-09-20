//! Which of an actor's clips plays: the walk while it moves, the idle while
//! it stands, a one-shot over either when the server says it used an
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
    systems::movement::{ standing_y, GRAVITY },
};

/// An actor's clips, each found in its GLB by name: the rest pose, the
/// idle, the walk, then the one-shots. An actor's asset holds whichever it
/// has; `Clips` says which.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Clip {
    Tee,
    Idle,
    Walk,
    Jump,
    Attack,
    Counter,
}

impl Clip {
    pub const ALL: [Clip; 6] = [Clip::Tee, Clip::Idle, Clip::Walk, Clip::Jump, Clip::Attack, Clip::Counter];

    /// The animation's name in the asset.
    pub fn name(self) -> &'static str {
        match self {
            Clip::Tee => "_tee",
            Clip::Idle => "idle",
            Clip::Walk => "walk",
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
/// armature node, keyed by the clip's name — as much of it as is read:
/// a jump's three moments. Its length and the ground a cycle covers are
/// there too.
#[derive(Clone, Copy, Debug, Deserialize)]
struct Declaration {
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

/// The node in an actor's animation graph for each clip its asset holds,
/// and the jump's moments where it declares them, on the entity with its
/// `AnimationPlayer`; a clip the asset lacks has no node.
#[derive(Component, Default)]
pub struct Clips {
    nodes: HashMap<Clip, AnimationNodeIndex>,
    jump: Option<Moments>,
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
        clips.jump = gltf.nodes.iter()
            .filter_map(|h| nodes.get(h)?.extras.as_ref())
            .find_map(|extras| serde_json::from_str::<Extras>(&extras.value).ok())
            .and_then(|extras| {
                let d = extras.animgen.get(Clip::Jump.name())?;
                Some(Moments { leave: d.leave?, freeze: d.freeze?, land: d.land? })
            });
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

/// How long a one-shot blends in and the walk and idle blend between.
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

/// How long an actor's fall has left, in seconds: it falls at `GRAVITY`
/// onto the standing level of the tile under it.
fn time_to_land(world: Vec3, map: &Map) -> Option<f32> {
    let here: qrz::Qrz = map.convert(world);
    let (floor, _) = map.get_by_qr(here.q, here.r)?;
    Some(((world.y - standing_y(floor, map)) / GRAVITY).max(0.0) / 1000.0)
}

pub fn update(
    mut commands: Commands,
    mut query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading, Option<&mut Jumping>)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
    map: Res<Map>,
) {
    for (entity, &airtime, &animates, vis_pos, &heading, jumping) in &mut query {
        // Entity is moving if VisualPosition is actively interpolating
        let travel = vis_pos.to - vis_pos.from;
        let is_moving = !vis_pos.is_complete() && travel.length_squared() > 0.001;
        // Walking against the facing plays the walk in reverse.
        let speed = if travel.xz().dot(heading.to_world_dir()) < 0.0 { -1. } else { 1. };

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
                let to_land = (airtime.step == Some(0)).then(|| time_to_land(vis_pos.current(), &map)).flatten();
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

        // A one-shot holds the actor until it ends.
        if let Some(node) = main {
            let one_shot = !clips.is(node, Clip::Idle) && !clips.is(node, Clip::Walk) && !clips.is(node, Clip::Tee);
            if one_shot && player.animation(node).is_some_and(|a| !a.is_finished()) {
                continue;
            }
        }
        // An actor with no jump walks in the air.
        if is_moving || (airborne && jump.is_none()) {
            if main != Some(walk) {
                transitions.play(&mut player, walk, SETTLE).set_speed(speed).repeat();
            } else if let Some(walk) = player.animation_mut(walk) {
                walk.set_speed(speed);
            }
        } else if main != Some(idle) {
            transitions.play(&mut player, idle, SETTLE).set_speed(1.).repeat();
        }
    }
}
