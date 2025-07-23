use std::time::Duration;

use bevy::{
    prelude::*, 
    scene::SceneInstanceReady
};
use qrz::Convert;

use crate::{
    client::components::*,
    common::{
        components::{ *, 
            entity_type::{ *,
                actor::*,
            },
            heading::*, 
            keybits::*, 
            offset::*, 
        },
        message::{ Event, * }, 
        resources::map::Map,
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
        if let Do { event: Event::Spawn { ent, typ: EntityType::Actor(desc), qrz } } = message {
            commands.entity(ent).insert((
                Loc::new(qrz),
                EntityType::Actor(desc),
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(get_asset(EntityType::Actor(desc))))),
                Transform { 
                    translation: map.convert(qrz),
                    scale: Vec3::ONE * map.radius(),
                    ..default()},
                AirTime { state: Some(0), step: None },
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