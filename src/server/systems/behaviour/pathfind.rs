use bevy::prelude::*;
use pathfinding::prelude::*;
use qrz::Qrz;
use rand::seq::IteratorRandom;

use crate::common::{
    components::{ behaviour::*, entity_type::*, heading::Heading, keybits::KeyBits, offset::*, * },
    message::{Component, Event, * },
    plugins::nntree::*, 
    resources::map::*, 
    systems::physics,
};

pub fn tick(
    mut writer: EventWriter<Do>,
    mut query: Query<(&Loc, &mut Behaviour)>,
    q_other: Query<(&Loc, &EntityType)>,
    nntree: Res<NNTree>,
    map: Res<Map>,
) {
    for (it,_) in nntree.iter() {
        let ent = Entity::from_bits(it);
        let Ok((&loc, mut behaviour)) = query.get_mut(ent) else { continue };
        let Behaviour::Pathfind(Pathfind { dest: dest0, mut path }) = *behaviour else { continue };

        let others = nntree.within_unsorted::<Hexhattan>(&loc.into(), 20_i16.into());
        let Some(other) = others.iter().filter(|it| it.item != ent.to_bits()).choose(&mut rand::thread_rng()) else { continue };
        let Ok((&o_loc, &o_typ)) = q_other.get(Entity::from_bits(other.item)) else { continue };
        let EntityType::Actor(_) = o_typ else { continue };
        
        let Some((dest,_)) = map.find(*o_loc,-2) else { continue };
        let Some((start,_)) = map.find(*loc,-2) else { continue };

        if dest == dest0 { continue; }

        let (full_path, _) = astar(
                &start,
                |&l| map.neighbors(l).into_iter().map(|it| (it.0, match it.1 {
                        EntityType::Decorator(_) => 1_i16,
                        _ => unreachable!()
                })),
                |&l| l.distance(&dest), 
                |&l| l == dest
            ).unwrap_or_default();

        // push the first 20 steps of the path not including current location
        path.clear();
        for &it in full_path.iter().skip(1).take(20).rev() { path.push(it); }

        *behaviour = Behaviour::Pathfind(Pathfind { dest, path });
        writer.write(Do { event: Event::Incremental { ent, component: Component::Behaviour(*behaviour) }});
    }
}

pub fn apply(
    mut writer: EventWriter<Do>,
    mut query: Query<(Entity, &mut Behaviour, &Loc, &mut Heading, &mut Offset, &mut AirTime)>,
    dt: Res<Time>,
    map: Res<Map>,
    nntree: Res<NNTree>,
) {
    for (ent, mut behaviour, &loc, mut heading0, mut offset0, mut airtime0) in &mut query {
        let Behaviour::Pathfind(mut pathfind) = *behaviour else { continue; };
        if pathfind.path.is_empty() { continue; }
        let &qrz = pathfind.path.last().expect("no last in path");
        let here = *loc - Qrz::Z;
        if here == qrz { 
            pathfind.path.pop(); 
            *behaviour = Behaviour::Pathfind(pathfind);
        }

        let Some(&dest) = pathfind.path.last() else { continue };
        let heading = Heading::from(KeyBits::from(Heading::new(dest - here)));
        if heading != *heading0 {
            *heading0 = heading;
            writer.write(Do { event: Event::Incremental { ent, component: Component::Heading(heading) }});
        }
        if loc.z <= dest.z && airtime0.state.is_none() { airtime0.state = Some(125); }
        let (offset, airtime) = physics::apply(Loc::new(dest), dt.delta().as_millis() as i16, loc, offset0.state, airtime0.state, &map, &nntree);
        (offset0.state, airtime0.state) = (offset,airtime);
    }
}
