//! Which of an actor's clips plays: the walk while it moves, the idle while
//! it stands, and a one-shot over either when the server says it used an
//! ability, held until the one-shot ends.

use std::time::Duration;

use bevy::prelude::*;

use crate::components::*;
use common_bevy::{
    components::{ heading::Heading, position::VisualPosition, * },
    message::{ AbilityType, Do, Event },
};

/// An actor's clips, numbered as its asset orders them: the rest pose, the
/// idle, the walk, then the one-shots. The animation graph's node for clip
/// `k` is `k + 1`, the root being node 0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Clip {
    Tee,
    Idle,
    Walk,
    Attack,
    Counter,
}

impl Clip {
    pub fn node(self) -> AnimationNodeIndex {
        (self as u32 + 1).into()
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

/// How long a one-shot blends in and the walk and idle blend between.
const BLEND: Duration = Duration::from_millis(120);
const SETTLE: Duration = Duration::from_millis(300);

/// Plays the one-shot for each ability the server confirms, where the
/// actor's graph has that clip.
pub fn play_abilities(
    mut reader: MessageReader<Do>,
    actors: Query<&Animates>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &AnimationGraphHandle)>,
    graphs: Res<Assets<AnimationGraph>>,
) {
    for message in reader.read() {
        let Do { event: Event::UseAbility { ent, ability, .. } } = message else { continue };
        let Some(clip) = Clip::of(*ability) else { continue };
        let Ok(animates) = actors.get(*ent) else { continue };
        let Ok((mut player, mut transitions, graph)) = q_anim.get_mut(animates.0) else { continue };
        let has = graphs.get(&graph.0).is_some_and(|g| g.get(clip.node()).is_some());
        if !has {
            continue;
        }
        transitions.play(&mut player, clip.node(), BLEND).set_speed(1.);
    }
}

pub fn update(
    query: Query<(Entity, &AirTime, &Animates, &VisualPosition, &Heading)>,
    mut q_anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (_entity, &airtime, &animates, vis_pos, &heading) in &query {
        // Entity is moving if VisualPosition is actively interpolating
        let travel = vis_pos.to - vis_pos.from;
        let is_moving = !vis_pos.is_complete() && travel.length_squared() > 0.001;
        // Walking against the facing plays the walk in reverse.
        let speed = if travel.xz().dot(heading.to_world_dir()) < 0.0 { -1. } else { 1. };

        let (mut player, mut transitions) = q_anim.get_mut(animates.0).unwrap();
        // A one-shot holds the actor until it ends.
        let main = transitions.get_main_animation();
        if let Some(node) = main {
            let one_shot = node != Clip::Idle.node() && node != Clip::Walk.node() && node != Clip::Tee.node();
            if one_shot && player.animation(node).is_some_and(|a| !a.is_finished()) {
                continue;
            }
        }
        if is_moving || airtime.step.is_some() {
            if main != Some(Clip::Walk.node()) {
                transitions.play(&mut player, Clip::Walk.node(), SETTLE).set_speed(speed).repeat();
            } else if let Some(walk) = player.animation_mut(Clip::Walk.node()) {
                walk.set_speed(speed);
            }
        } else if main != Some(Clip::Idle.node()) {
            transitions.play(&mut player, Clip::Idle.node(), SETTLE).set_speed(1.).repeat();
        }
    }
}
