//! Dresses actors in what the server says they wear, and keeps the local
//! player's bag.
//!
//! A piece's GLB ships its wearer's cut with a copy of that body's rig and
//! mesh, the copy the build proves it on. The client keeps the piece, points
//! its skin's joints at the actor's own joints of the same names and drops
//! the copies, so the actor's clips move the piece and nothing is fitted at
//! load.

use bevy::{mesh::skinning::SkinnedMesh, prelude::*};
use std::collections::{HashMap, HashSet};

use common_bevy::{
    components::{
        entity_type::EntityType,
        equipment::{Equipment, Inventory, Item},
    },
    message::{Event, *},
};

use crate::systems::actor::actor_name;

/// A worn piece's scene, a child of the actor wearing it.
#[derive(Component)]
pub struct Worn {
    pub item: Item,
    /// Whether the piece's joints point at the actor's yet.
    bound: bool,
}

/// Keeps the local player's bag as the server sends it.
pub fn do_inventory(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
) {
    for message in reader.read() {
        let Do { event: Event::Inventory { ent, items } } = message else { continue };
        if let Ok(mut entity) = commands.get_entity(*ent) {
            entity.insert(Inventory { items: items.clone() });
        }
    }
}

/// Spawns a scene for each item an actor newly wears and despawns the scene
/// of each it took off.
pub fn dress(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    actors: Query<(Entity, &EntityType, &Equipment, Option<&Children>), Changed<Equipment>>,
    worn: Query<(Entity, &Worn)>,
) {
    for (actor, typ, equipment, children) in &actors {
        let wanted: HashSet<Item> = equipment.items().collect();
        let mut present = HashSet::new();
        for (piece, worn) in children.into_iter().flatten().filter_map(|&c| worn.get(c).ok()) {
            if wanted.contains(&worn.item) {
                present.insert(worn.item);
            } else {
                commands.entity(piece).despawn();
            }
        }
        let body = actor_name(*typ);
        for item in wanted.difference(&present) {
            let path = format!("models/{}-{}.glb", item.piece.name(), body);
            commands.spawn((
                Worn { item: *item, bound: false },
                Name::new(format!("worn:{}", item.piece.name())),
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(item.style as usize).from_asset(path))),
                ChildOf(actor),
            ));
        }
    }
}

/// Points each unbound piece's joints at its wearer's, once both scenes are
/// spawned, then drops the piece's copies of the body and its rig.
pub fn bind_worn(
    mut commands: Commands,
    mut pieces: Query<(Entity, &ChildOf, &mut Worn)>,
    actors: Query<&EntityType>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    mut skins: Query<&mut SkinnedMesh>,
) {
    for (piece, child_of, mut worn) in &mut pieces {
        if worn.bound {
            continue;
        }
        let actor = child_of.parent();
        let Ok(typ) = actors.get(actor) else { continue };
        let body = actor_name(*typ);

        // The actor's rig is its scene's root node, a direct child named for
        // the body; the piece's copy of it sits a level deeper, under `piece`.
        let Some(rig) = children.get(actor).ok().and_then(|c| {
            c.iter().find(|&e| names.get(e).is_ok_and(|n| n.as_str() == body))
        }) else { continue };
        let joints: HashMap<&str, Entity> = children
            .iter_descendants(rig)
            .filter_map(|e| names.get(e).ok().map(|n| (n.as_str(), e)))
            .collect();

        let skinned: Vec<Entity> = children
            .iter_descendants(piece)
            .filter(|&e| skins.contains(e))
            .collect();
        if skinned.is_empty() {
            continue;
        }
        let mut rebound = Vec::with_capacity(skinned.len());
        for &e in &skinned {
            let skin = skins.get(e).unwrap();
            let Some(targets) = skin
                .joints
                .iter()
                .map(|&j| names.get(j).ok().and_then(|n| joints.get(n.as_str()).copied()))
                .collect::<Option<Vec<Entity>>>()
            else { continue };
            rebound.push((e, targets));
        }
        if rebound.len() < skinned.len() {
            continue;
        }

        let copies: HashSet<Entity> = skinned
            .iter()
            .flat_map(|&e| skins.get(e).unwrap().joints.clone())
            .collect();
        for (e, targets) in rebound {
            skins.get_mut(e).unwrap().joints = targets;
        }
        // Despawn each copied joint tree at its root; despawning recurses.
        for &joint in &copies {
            let is_root = parents.get(joint).map_or(true, |p| !copies.contains(&p.parent()));
            if is_root {
                commands.entity(joint).try_despawn();
            }
        }
        let body_mesh = format!("{body}-mesh");
        for e in children.iter_descendants(piece) {
            if names.get(e).is_ok_and(|n| n.as_str() == body_mesh) {
                commands.entity(e).try_despawn();
            }
        }
        worn.bound = true;
    }
}
