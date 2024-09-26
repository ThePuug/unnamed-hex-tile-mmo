use crate::{ *,
    common::{
        message::{*, Event},
        components::{
            keybits::*,
            hx::*,
        },
        resources::map::*,
    },
};

pub fn do_input(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    map: Res<Map>,
    time: Res<Time>,
    mut query: Query<(Entity, &Heading, &Hx, &mut Offset, Option<&mut AirTime>, Option<&KeyBits>)>,
) {
    for (ent, &heading, &hx0, mut offset0, air_time, key_bits) in query.iter_mut() {
        let mut offset = *offset0;

        let xy = Vec3::from(hx0).xy();
        let xy_curr = xy + offset0.0.xy();

        let (hx_floor, _) = map.find(hx0 + Hx{ z: 1, ..default() }, -10);
    
        if let Some(mut air_time) = air_time { 
            if air_time.0 > 0. { air_time.0 -= time.delta_seconds(); }
            else {
                let z_fall = time.delta_seconds() * 10.;
                if hx_floor.is_none() 
                    || hx0.z as f32 + offset.0.z - z_fall > hx_floor.unwrap().z as f32 + 1. { 
                    offset.0.z -= z_fall;
                } else {
                    offset.0.z = hx_floor.unwrap().z as f32 + 1. - hx0.z as f32;
                    commands.entity(ent).remove::<AirTime>();
                }
            }
        }

        let xy_target = xy.lerp(Vec3::from(hx0 + heading.0).xy(),
            if key_bits.is_some() && (*(key_bits.unwrap()) & (KB_HEADING_Q | KB_HEADING_R)) { 1.25 }
            else { 0.25 }); 
        
        let xy_dist = xy_curr.distance(xy_target);
        let ratio = 0_f32.max((xy_dist - 100_f32 * time.delta_seconds()) / xy_dist);
        offset.0 = (xy_curr.lerp(xy_target, 1. - ratio) - xy).extend(offset.0.z);

        let hx = Hx::from(xy.extend(hx0.z as f32) + offset.0);
        if hx0 != hx { writer.send(Try { event: Event::Move { ent, hx, heading } }); }
        *offset0 = offset;
    }
}

pub fn do_move(
    mut reader: EventReader<Do>,
    mut query: Query<(&mut Hx, &mut Offset, &mut Heading)>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Move { ent, hx, heading } } => {
                if let Ok((mut hx0, mut offset0, mut heading0)) = query.get_mut(ent) {
                    *offset0 = Offset(Vec3::from(*hx0) + offset0.0 - Vec3::from(hx));
                    if *hx0 != hx { 
                        *hx0 = hx; 
                        *heading0 = heading;
                    }
                    else if *heading0 != heading { *heading0 = heading; }
                }
            }
            _ => {}
        }
    }
}

pub fn update_headings(
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Hx, &Heading), Changed<Heading>>,
) {
    for (_ent, &hx, &heading) in &mut query {
        writer.send(Try { event: Event::Move { ent: _ent, hx, heading } });
        writer.send(Try { event: Event::Discover { hx: hx + heading.0 + Hx { q: 0, r: 0, z: -1 } } });
    }
}
