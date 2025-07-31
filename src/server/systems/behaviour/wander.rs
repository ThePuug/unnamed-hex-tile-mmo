use bevy::prelude::*;
use rand::seq::SliceRandom;

use crate::common::{
    components::{ behaviour::*, entity_type::*, * },
    message::{Component, Event, * },
    plugins::nntree::*,
};

pub fn tick(
    mut writer: EventWriter<Do>,
    mut query: Query<(&Loc, &mut Behaviour)>,
    q_other: Query<(&Loc, &EntityType)>,
    // dt: Res<Time>,
    nntree: Res<NNTree>,
) {
    for (it,_) in nntree.iter() {
        let ent = Entity::from_bits(it);
        let Ok((&loc, mut behaviour)) = query.get_mut(ent) else { continue; };
        let Behaviour::Wander(wander) = *behaviour else { continue; };
        if wander.qrz != *loc { continue; }

        let others = nntree.within_unsorted::<Hexhattan>(&loc.into(), 20_i16.into());
        let Some(other) = others.choose(&mut rand::thread_rng()) else { continue; };
        let Ok((&o_loc, &o_typ)) = q_other.get(Entity::from_bits(other.item)) else { continue; };
        let EntityType::Actor(_) = o_typ else { continue; };

        *behaviour = Behaviour::Wander(Wander { qrz: *o_loc });
        writer.write(Do { event: Event::Incremental { ent, component: Component::Behaviour(*behaviour) }});
    }
}
