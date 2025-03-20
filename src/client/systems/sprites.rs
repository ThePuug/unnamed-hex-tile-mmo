use bevy::prelude::*;

use crate::{ // *,
    common::{
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        message::{*, Event},
    },
};

pub fn update_transforms(
    time: Res<Time>,
    mut query: Query<(&Hx, &Offset, &Heading, &KeyBits, &mut Transform)>,
) {
    for (&hx, &offset, &heading, &keybits, mut transform0) in &mut query {
        let target = match (keybits, offset, heading) {
            (keybits, offset, _) if keybits != KeyBits::default() => Vec3::from(hx) + offset.step,
            (_, _, heading) => Vec3::from(hx) + Vec3::from(heading),
        };
        transform0.translation = transform0.translation.lerp(target,1.-0.01f32.powf(time.delta_secs()));
        transform0.rotation = heading.into();
    }
}

pub fn try_gcd(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
) {
    for &message in reader.read() {
        if let Try { event: Event::Gcd { ent, typ } } = message {
            debug!("try gcd {ent} {:?}", typ);
            writer.send(Do { event: Event::Gcd { ent, typ }});
        }
    }
}