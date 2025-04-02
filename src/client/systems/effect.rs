use std::f32::consts::PI;

use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use crate::{
    client::resources::*, 
    common::{
    components::{ 
        heading::*, 
        hx::*,
    },
    message::{
        Do, 
        Event
    }, 
    systems::gcd::*,
}};

pub fn setup(
    mut effects: ResMut<Assets<EffectAsset>>,
    mut map: ResMut<EffectMap>,
) {

    let writer = ExprWriter::new();

    let init_age = SetAttributeModifier::new(
        Attribute::AGE, 
        writer.lit(0.).expr());

    let init_lif = SetAttributeModifier::new(
        Attribute::LIFETIME, 
        writer.lit(0.2).expr());

    let init_pos = SetAttributeModifier::new(
        Attribute::POSITION, 
        writer.lit(Vec3::ZERO).expr());

    let update_pos = SetAttributeModifier::new(
        Attribute::POSITION,
        writer.attr(Attribute::AGE)
            .div(writer.attr(Attribute::LIFETIME))
            .mul(writer.lit(PI)) // 2*PI*180/360
            .cos()
            .mul(writer.lit(TILE_SIZE))
            .vec3(writer.attr(Attribute::AGE)
                    .div(writer.attr(Attribute::LIFETIME))
                    .mul(writer.lit(PI)) // 2*PI*180/360
                    .sin()
                    .mul(writer.lit(TILE_SIZE)),
                writer.lit(0.))
            .expr()
    );

    let init_vel = SetAttributeModifier::new(
        Attribute::VELOCITY, 
        writer.lit(Vec3::ZERO).expr());

    let effect = effects.add(
        EffectAsset::new(64, Spawner::once(1_f32.into(), true), writer.finish())
            .with_name("attack")
            .with_simulation_space(SimulationSpace::Local)
            .with_trails(20, 1. / 100., 0.2, 0)
            .init(init_pos)
            .init(init_age)
            .init(init_lif)
            .init(init_vel)
            .update(update_pos)
    );
  
    map.0.insert(GcdType::Attack, effect);
}

pub fn render_do_gcd(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    query: Query<(&Hx, &Heading)>,
    map: Res<EffectMap>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Gcd { ent, typ, .. } } = message {
            let (&hx, &heading) = query.get(ent).unwrap();
            let pos = Vec3::from(hx + *heading);
            let effect = map.0.get(&typ).unwrap().clone();

            let it = commands.spawn(ParticleEffectBundle {
                effect: ParticleEffect::new(effect),
                transform: Transform {
                    rotation: Quat::from(heading) * Quat::from_rotation_z(-PI/2.),
                    translation: hx.into(),
                    scale: Vec3::ONE, 
                },
                ..default()
            }).id();
            debug!("spawned gcd effect: {:?} at {:?}", it, pos);
        }
    }
}