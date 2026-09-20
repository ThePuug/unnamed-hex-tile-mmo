//! Which of an actor's clips plays: the walk while it moves, the idle while
//! it stands, and a one-shot over either when the server says it used an
//! ability, held until the one-shot ends.

use std::{ collections::HashMap, time::Duration };

use bevy::prelude::*;

use crate::components::*;
use common_bevy::{
    components::{ heading::Heading, position::VisualPosition, * },
    message::{ AbilityType, Do, Event },
};

/// An actor's clips, each found in its GLB by name: the rest pose, the
/// idle, the walk, then the one-shots. An actor's asset holds whichever it
/// has; `Clips` says which.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Clip {
    Tee,
    Idle,
    Walk,
    Attack,
    Counter,
}

impl Clip {
    pub const ALL: [Clip; 5] = [Clip::Tee, Clip::Idle, Clip::Walk, Clip::Attack, Clip::Counter];

    /// The animation's name in the asset.
    pub fn name(self) -> &'static str {
        match self {
            Clip::Tee => "_tee",
            Clip::Idle => "idle",
            Clip::Walk => "walk",
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

/// The node in an actor's animation graph for each clip its asset holds,
/// on the entity with its `AnimationPlayer`; a clip the asset lacks has
/// no node.
#[derive(Component, Default)]
pub struct Clips(HashMap<Clip, AnimationNodeIndex>);

impl Clips {
    /// Builds an actor's graph from the animations its GLB names, one node
    /// per clip it holds.
    pub fn from_gltf(gltf: &Gltf) -> (AnimationGraph, Clips) {
        let mut graph = AnimationGraph::new();
        let mut clips = Clips::default();
        for clip in Clip::ALL {
            if let Some(handle) = gltf.named_animations.get(clip.name()) {
                clips.0.insert(clip, graph.add_clip(handle.clone(), 1.0, graph.root));
            }
        }
        (graph, clips)
    }

    pub fn node(&self, clip: Clip) -> Option<AnimationNodeIndex> {
        self.0.get(&clip).copied()
    }

    /// Whether `node` plays `clip`.
    pub fn is(&self, node: AnimationNodeIndex, clip: Clip) -> bool {
        self.node(clip) == Some(node)
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

pub fn update(
    query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &Clips)>,
) {
    for (_entity, &airtime, &animates, vis_pos, &heading) in &query {
        // Entity is moving if VisualPosition is actively interpolating
        let travel = vis_pos.to - vis_pos.from;
        let is_moving = !vis_pos.is_complete() && travel.length_squared() > 0.001;
        // Walking against the facing plays the walk in reverse.
        let speed = if travel.xz().dot(heading.to_world_dir()) < 0.0 { -1. } else { 1. };

        let Ok((mut player, mut transitions, clips)) = q_anim.get_mut(animates.0) else { continue };
        let (Some(idle), Some(walk)) = (clips.node(Clip::Idle), clips.node(Clip::Walk)) else { continue };
        // A one-shot holds the actor until it ends.
        let main = transitions.get_main_animation();
        if let Some(node) = main {
            let one_shot = !clips.is(node, Clip::Idle) && !clips.is(node, Clip::Walk) && !clips.is(node, Clip::Tee);
            if one_shot && player.animation(node).is_some_and(|a| !a.is_finished()) {
                continue;
            }
        }
        if is_moving || airtime.step.is_some() {
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
