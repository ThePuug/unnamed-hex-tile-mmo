use std::time::Duration;

use bevy::{
    prelude::*, 
    scene::SceneInstanceReady
};
use qrz::{Convert, Qrz};

use crate::{
    client::components::*,
    common::{
        components::{ 
            behaviour::*,
            entity_type::{ actor::*, * }, 
            heading::*, keybits::*, offset::*, * 
        }, 
        message::{ Event, * }, 
        plugins::nntree::NearestNeighbor, 
        resources::{map::Map, *}
    }
};

pub fn setup() {}

fn ready(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    query: Query<&EntityType>,
    mut q_player: Query<&mut AnimationPlayer>,
    q_child: Query<&Children>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    for child in q_child.iter_descendants(trigger.target()) {
        if let Ok(mut player) = q_player.get_mut(child) {
            commands.entity(trigger.target()).insert(Animates(child));

            let &typ = query.get(trigger.target()).expect("couldn't get entity type");
            let asset = get_asset(typ);
            let (graph, _) = AnimationGraph::from_clips([
                asset_server.load(GltfAssetLabel::Animation(0).from_asset(asset.clone())),
                asset_server.load(GltfAssetLabel::Animation(1).from_asset(asset.clone())),
                asset_server.load(GltfAssetLabel::Animation(2).from_asset(asset.clone()))]);
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
    mut query: Query<(Entity, &Loc, &Offset, &Heading, &KeyBits, &mut Transform)>,
    map: Res<Map>,
    buffers: Res<InputQueues>,
) {
    for (entity, &loc, &offset, &heading, &keybits, mut transform0) in &mut query {
        // Only local player (with input buffer) uses offset.step for physics movement
        let is_local_player = buffers.get(&entity).is_some();
        
        let target = match (is_local_player, offset, heading) {
            // Local player actively moving: use physics position
            (true, offset, _) if offset.step.length_squared() > 0.01 => map.convert(*loc) + offset.step,
            // Player has a heading set: position them in that triangle of the hex
            (_, _, heading) if *heading != Qrz::default() => {
                let dir = map.convert(*loc + *heading) - map.convert(*loc);
                map.convert(*loc) + dir * HERE
            },
            // Default: center of tile
            _ => map.convert(*loc),
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
        let Do { event: Event::Spawn { ent, typ, qrz } } = message else { continue };
        let EntityType::Actor(desc) = typ else { continue };
        let loc = Loc::new(qrz);
        commands.entity(ent).insert((
            loc,
            typ,
            Behaviour::Controlled,  // Remote players are controlled by network updates
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(get_asset(EntityType::Actor(desc))))),
            Transform { 
                translation: map.convert(qrz),
                scale: Vec3::ONE * map.radius(),
                ..default()},
            AirTime { state: Some(0), step: None },
            NearestNeighbor::new(ent, loc),
            Heading::default(),
            Offset::default(),
            KeyBits::default(),
            Visibility::default(),
            Physics::default(),
        )).observe(ready);
    }
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            writer.write(Do { event: Event::Gcd { ent, typ }});
        }
    }
}

fn get_asset(typ: EntityType) -> String {
    match typ {
        EntityType::Actor(desc) => format!("actors/{}-{}.glb", 
            match desc.origin {
                Origin::Starborn => "starborn",
                Origin::Fauna => "fauna",
                _ => panic!("couldn't find asset for origin {:?}", desc.origin)
            },
            match desc.form {
                Form::Humanoid => "humanoid",
                Form::Bestial => "bestial",
                _ => panic!("couldn't find asset for form {:?}", desc.form)
            }),
        _ => panic!("couldn't find asset for entity type {:?}", typ)
    }
}