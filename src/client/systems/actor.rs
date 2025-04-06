use std::time::Duration;

use bevy::{
    prelude::*, 
    scene::SceneInstanceReady
};
use qrz::Convert;

use crate::{
    client::components::*,
    common::{
        components::{ heading::*, keybits::*, offset::*, * },
        message::{ Event, * }, 
        resources::map::Map,
    }
};

pub fn setup() {}

fn ready(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    mut query: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    for child in q_child.iter_descendants(trigger.entity()) {
        if let Ok(mut player) = query.get_mut(child) {
            commands.entity(trigger.entity()).insert(Animator::new(child));
            let (graph, _) = AnimationGraph::from_clips([
                asset_server.load(GltfAssetLabel::Animation(0).from_asset("models/actor-blank.glb")),
                asset_server.load(GltfAssetLabel::Animation(1).from_asset("models/actor-blank.glb")),
                asset_server.load(GltfAssetLabel::Animation(2).from_asset("models/actor-blank.glb"))]);
            let handle = AnimationGraphHandle(graphs.add(graph));
            let mut transitions = AnimationTransitions::new();
            transitions.play(&mut player, 2.into(), Duration::ZERO).set_speed(1.).repeat();
            commands.entity(child)
                .insert(handle)
                .insert(transitions);
        }
    }
}

pub fn update(
    time: Res<Time>,
    mut query: Query<(&Loc, &Offset, &Heading, &KeyBits, &mut Transform)>,
    map: Res<Map>,
) {
    for (&loc, &offset, &heading, &keybits, mut transform0) in &mut query {
        let target = match (keybits, offset, heading) {
            (keybits, offset, _) if keybits != KeyBits::default() => map.convert(*loc) + offset.step,
            (_, _, heading) => map.convert(*loc) + map.convert(*heading) * HERE,
        };
        
        let dpx = transform0.translation.distance(target);
        let ratio = 0_f32.max((dpx - 0.0045 * time.delta().as_millis() as f32) / dpx);
        let lpx = transform0.translation.lerp(target, 1. - ratio);
        transform0.translation = lpx;
        transform0.rotation = heading.into();

        // if we are getting too far away, apply some correction
        let dist = transform0.translation.distance_squared(target);
        if dist > 1. { transform0.translation = transform0.translation.lerp(target,1.-0.5f32.powf(time.delta_secs())); }
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    asset_server: Res<AssetServer>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Spawn { ent, typ: EntityType::Actor, qrz } } = message {
            commands.entity(ent).insert((
                Loc::new(qrz),
                SceneRoot(asset_server.load(
                    GltfAssetLabel::Scene(0).from_asset("models/actor-blank.glb"),
                )),
                Transform {
                    translation: map.convert(qrz),
                    scale: Vec3::ONE * map.radius(),
                    ..default()},
                AirTime { state: Some(0), step: None },
                EntityType::Actor,
                Heading::default(),
                Offset::default(),
                KeyBits::default(),
                Visibility::default(),
            )).observe(ready);
        }
    }
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            debug!("try gcd {ent} {typ:?}");
            writer.send(Do { event: Event::Gcd { ent, typ }});
        }
    }
}