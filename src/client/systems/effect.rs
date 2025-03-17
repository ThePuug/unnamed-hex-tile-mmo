use bevy::prelude::*;
use bevy_hanabi::prelude::*;

use crate::{
    client::resources::*, 
    common::{
    components::{ *, 
        heading::*, 
        hx::*
    },
    message::{ *, Event }, 
    systems::gcd::*,
}};

pub fn setup(
    mut effects: ResMut<Assets<EffectAsset>>,
    mut map: ResMut<EffectMap>,
) {
    let mut gradient = Gradient::new();
    gradient.add_key(0.0, Vec4::new(1., 0., 0., 1.));
    gradient.add_key(1.0, Vec4::ZERO);
  
    let mut module = Module::default();

    let init_pos = SetPositionCircleModifier {
        axis: module.lit(Vec3::Z),
        center: module.lit(Vec3::ZERO),
        radius: module.lit(TILE_SIZE / 2.),
        dimension: ShapeDimension::Volume,
    };
  
    let init_vel = SetVelocitySphereModifier {
        center: module.lit(Vec3::ZERO),
        speed: module.lit(0.),
    };

    let lifetime = module.lit(3.);
    let init_lifetime = SetAttributeModifier::new(bevy_hanabi::Attribute::LIFETIME, lifetime);
  
    let accel = module.lit(Vec3::new(0., 0., 1.));
    let update_accel = AccelModifier::new(accel);

    let effect: Handle<EffectAsset> = effects.add(
        EffectAsset::new(256,Spawner::once(256_f32.into(),true),module)
            .with_name("MyEffect")
            .init(init_pos)
            .init(init_vel)
            .init(init_lifetime)
            .update(update_accel)
            .render(ColorOverLifetimeModifier { gradient })
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
            let pos = (hx + heading.0, Vec3::Z * 0.1).calculate();
            let effect = map.0.get(&typ).unwrap().clone();
            let it = commands.spawn(ParticleEffectBundle {
                effect: ParticleEffect::new(effect),
                transform: Transform::from_translation(pos),
                ..default()
            }).id();
            debug!("spawned gcd effect: {:?} at {:?}", it, pos);
        }
    }
}