//! Dresses actors in what the server says they wear, and keeps the local
//! player's bag.
//!
//! A skinned piece's GLB ships its wearer's cut with a copy of that body's
//! rig, the joints its skin names; the body itself is not in it. The client
//! keeps the piece, points its skin's joints at the actor's own joints of
//! the same names and drops the copy, so the actor's clips move the piece
//! and nothing is fitted at load.

use bevy::{mesh::skinning::SkinnedMesh, prelude::*};
use std::collections::{HashMap, HashSet};

use common_bevy::{
    components::{
        entity_type::EntityType,
        equipment::{Equipment, Inventory, Item},
    },
    message::{Event, *},
};

use crate::systems::{actor::actor_name, hiding::Redress};

/// A worn piece's scene, a child of the actor wearing it.
#[derive(Component)]
pub struct Worn {
    pub item: Item,
    /// Whether the piece hangs from the actor's rig yet.
    bound: bool,
    /// Nodes moved out of the scene onto the rig's sockets, which go when
    /// the piece does.
    moved: Vec<Entity>,
}

/// The rig socket a node hangs from, named by its extras.
#[derive(Component)]
pub struct SocketAnchor(pub String);

impl Worn {
    pub fn is_bound(&self) -> bool {
        self.bound
    }

    /// The piece's nodes: the scene under `piece` and whatever was moved
    /// onto a socket.
    pub fn nodes(&self, piece: Entity, children: &Query<&Children>) -> Vec<Entity> {
        children
            .iter_descendants(piece)
            .chain(self.moved.iter().flat_map(|&m| std::iter::once(m).chain(children.iter_descendants(m))))
            .collect()
    }
}

/// The actor's own rig: its scene's root node, named for the body. The
/// scene spawner puts an unnamed root above it, and every worn piece's scene
/// carries a copy of the rig under the same name, so the search walks the
/// actor's descendants and skips anything inside a worn piece.
pub fn rig(
    actor: Entity,
    body: &str,
    children: &Query<&Children>,
    parents: &Query<&ChildOf>,
    names: &Query<&Name>,
    worn: &HashSet<Entity>,
) -> Option<Entity> {
    children.iter_descendants(actor).find(|&e| {
        names.get(e).is_ok_and(|n| n.as_str() == body)
            && parents.iter_ancestors(e).take_while(|&a| a != actor).all(|a| !worn.contains(&a))
    })
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
                for &moved in &worn.moved {
                    commands.entity(moved).try_despawn();
                }
                commands.entity(piece).despawn();
            }
        }
        let body = actor_name(*typ);
        for item in wanted.difference(&present) {
            let path = format!("models/{}-{}.glb", item.piece.name(), body);
            commands.spawn((
                Worn { item: *item, bound: false, moved: Vec::new() },
                Name::new(format!("worn:{}", item.piece.name())),
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(item.style as usize).from_asset(path))),
                ChildOf(actor),
            ));
        }
        commands.entity(actor).insert(Redress);
    }
}

/// Hangs each unbound piece from its wearer's rig once both scenes are
/// spawned: a skinned piece's joints are pointed at the actor's, a socket
/// piece's nodes are moved onto the rig's sockets, and the piece's copy of
/// the rig is dropped.
pub fn bind_worn(
    mut commands: Commands,
    mut pieces: Query<(Entity, &ChildOf, &mut Worn)>,
    actors: Query<&EntityType>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    anchors: Query<&SocketAnchor>,
    mut skins: Query<&mut SkinnedMesh>,
) {
    let all: HashSet<Entity> = pieces.iter().map(|(e, ..)| e).collect();
    let unbound: Vec<Entity> = pieces.iter().filter(|(_, _, w)| !w.bound).map(|(e, ..)| e).collect();
    for piece in unbound {
        let Ok((_, child_of, _)) = pieces.get(piece) else { continue };
        let actor = child_of.parent();
        let Ok(typ) = actors.get(actor) else { continue };
        let body = actor_name(*typ);

        let Some(rig) = rig(actor, body, &children, &parents, &names, &all) else { continue };
        let joints: HashMap<&str, Entity> = children
            .iter_descendants(rig)
            .filter_map(|e| names.get(e).ok().map(|n| (n.as_str(), e)))
            .collect();

        // A socket piece hangs its nodes from the rig's sockets, keeping the
        // transforms the build wrote relative to the socket's point.
        let anchored: Vec<(Entity, &SocketAnchor)> = children
            .iter_descendants(piece)
            .filter_map(|e| anchors.get(e).ok().map(|a| (e, a)))
            .collect();
        if !anchored.is_empty() {
            let Some(sockets) = anchored
                .iter()
                .map(|(_, a)| joints.get(format!("socket.{}", a.0).as_str()).copied())
                .collect::<Option<Vec<Entity>>>()
            else { continue };
            let Ok((_, _, mut worn)) = pieces.get_mut(piece) else { continue };
            for (&(node, _), socket) in anchored.iter().zip(sockets) {
                commands.entity(node).insert(ChildOf(socket));
                worn.moved.push(node);
            }
            worn.bound = true;
            commands.entity(actor).insert(Redress);
            continue;
        }

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
        if let Ok((_, _, mut worn)) = pieces.get_mut(piece) {
            worn.bound = true;
        }
        commands.entity(actor).insert(Redress);
    }
}
