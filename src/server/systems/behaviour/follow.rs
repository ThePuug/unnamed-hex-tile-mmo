use bevy::prelude::*;
use rand::seq::IteratorRandom;

use crate::common::{
    components::{ 
        behaviour::*, 
        entity_type::*, 
        keybits::{KeyBits, *}, 
        * 
    },
    message::{Component, Event, * },
    plugins::nntree::*,
};

pub fn tick(
    mut writer: EventWriter<Do>,
    mut query: Query<(&Loc, &mut Behaviour)>,
    q_other: Query<(&Loc, &EntityType)>,
    nntree: Res<NNTree>,
) {
    for (it,_) in nntree.iter() {
        let ent = Entity::from_bits(it);
        let Ok((&loc, mut behaviour)) = query.get_mut(ent) else { continue; };
        let Behaviour::Wander(_) = *behaviour else { continue; };
        // if wander.qrz != *loc { continue; }

        let others = nntree.within_unsorted::<Hexhattan>(&loc.into(), 20_i16.into());
        let Some(other) = others.iter().filter(|it| it.item != ent.to_bits()).choose(&mut rand::thread_rng()) else { continue; };
        let Ok((&o_loc, &o_typ)) = q_other.get(Entity::from_bits(other.item)) else { continue; };
        let EntityType::Actor(_) = o_typ else { continue; };

        *behaviour = Behaviour::Wander(Wander { qrz: *o_loc });
        writer.write(Do { event: Event::Incremental { ent, component: Component::Behaviour(*behaviour) }});
    }
}

pub fn apply(
    mut writer: EventWriter<Do>,
    query: Query<(Entity, &Loc, &Behaviour)>,
) {
    for (ent, loc, behaviour) in query {
        let Behaviour::Wander(wander) = *behaviour else { continue; };
        if wander.qrz == **loc { continue; }
        let rel = wander.qrz - **loc;
        let rel_s = -rel.q-rel.r;
        let (q_r, r_s, s_q) = (rel.q-rel.r, rel.r-rel_s, rel_s-rel.q);
        let Some(&dir) = [q_r.abs(), r_s.abs(), s_q.abs()].iter().max() else { panic!("no max") };
        let key_bits = KeyBits { key_bits: match dir {
            dir if dir == q_r => KB_HEADING_Q | KB_HEADING_R | KB_HEADING_NEG,
            dir if dir == r_s => KB_HEADING_R,
            dir if dir == s_q => KB_HEADING_Q | KB_HEADING_NEG,
            dir if dir == -q_r => KB_HEADING_Q | KB_HEADING_R,
            dir if dir == -r_s => KB_HEADING_R | KB_HEADING_NEG,
            dir if dir == -s_q => KB_HEADING_Q,
            _ => unreachable!(),
        }, accumulator: 0 };
        if rel.z > 0 { 
            writer.write(Do { event: Event::Input { ent, 
                key_bits: KeyBits { key_bits: KB_JUMP, accumulator: 0}, 
                dt: 0 as u16, seq: 0 }}); }
        writer.write(Do { event: Event::Input { ent, key_bits, dt: 125 as u16, seq: 0 }});
    }
}