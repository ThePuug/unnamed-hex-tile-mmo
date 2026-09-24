//! Dresses actors in what the server says they wear, puts the tool of a
//! gather's work in the hand of an actor at it, and keeps the local
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
        equipment::{Equipment, Item},
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

/// A tool's scene in the hand of an actor at a gather's work, a child of
/// the actor, its nodes moved onto the rig's socket once bound.
#[derive(Component)]
pub struct Held {
    tool: &'static str,
    bound: bool,
    moved: Vec<Entity>,
}

/// The tool in hand for what an actor is doing at a gather: the stem of
/// its asset, for work; none stooped to a pile.
fn tool_of(activity: common::gathering::Activity) -> Option<&'static str> {
    match activity {
        common::gathering::Activity::Work(common::gathering::Work::Chop) => Some("wood-axe"),
        common::gathering::Activity::Work(common::gathering::Work::Mine) => Some("pick"),
        common::gathering::Activity::Pickup => None,
    }
}

/// Moves each node under `piece` that names a socket onto the rig's socket
/// of that name among `joints`, keeping the transform the build wrote
/// relative to the socket's point. Returns the nodes moved, or None where a
/// socket is not on the rig yet or the piece names none.
fn hang(
    piece: Entity,
    joints: &HashMap<&str, Entity>,
    children: &Query<&Children>,
    anchors: &Query<&SocketAnchor>,
    commands: &mut Commands,
) -> Option<Vec<Entity>> {
    let anchored: Vec<(Entity, &SocketAnchor)> =
        children.iter_descendants(piece).filter_map(|e| anchors.get(e).ok().map(|a| (e, a))).collect();
    if anchored.is_empty() {
        return None;
    }
    let sockets = anchored
        .iter()
        .map(|(_, a)| joints.get(format!("socket.{}", a.0).as_str()).copied())
        .collect::<Option<Vec<Entity>>>()?;
    for (&(node, _), socket) in anchored.iter().zip(sockets) {
        commands.entity(node).insert(ChildOf(socket));
    }
    Some(anchored.into_iter().map(|(node, _)| node).collect())
}

/// The joints of an actor's own rig, by name.
fn joints_of<'a>(rig: Entity, children: &Query<&Children>, names: &'a Query<&Name>) -> HashMap<&'a str, Entity> {
    children.iter_descendants(rig).filter_map(|e| names.get(e).ok().map(|n| (n.as_str(), e))).collect()
}

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
        let Do { event: Event::Inventory { ent, bag } } = message else { continue };
        if let Ok(mut entity) = commands.get_entity(*ent) {
            entity.insert(bag.clone());
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
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(item.style as usize).from_asset(path))),
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
        let joints = joints_of(rig, &children, &names);

        // A socket piece hangs its nodes from the rig's sockets.
        if children.iter_descendants(piece).any(|e| anchors.contains(e)) {
            let Some(moved) = hang(piece, &joints, &children, &anchors, &mut commands) else { continue };
            let Ok((_, _, mut worn)) = pieces.get_mut(piece) else { continue };
            worn.moved.extend(moved);
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

/// Puts the tool of its work in the hand of each actor at a gather's work,
/// and takes it away when the work stops or turns to another.
pub fn hold(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    actors: Query<(Entity, &EntityType, Option<&crate::systems::gathering::Gathering>)>,
    held: Query<(Entity, &ChildOf, &Held)>,
) {
    let mut holding = HashSet::new();
    for (piece, child_of, h) in &held {
        let actor = child_of.parent();
        let wanted = actors.get(actor).ok().and_then(|(_, _, gathering)| gathering).and_then(|g| tool_of(g.0));
        if wanted == Some(h.tool) {
            holding.insert(actor);
            continue;
        }
        for &moved in &h.moved {
            commands.entity(moved).try_despawn();
        }
        commands.entity(piece).despawn();
    }
    for (actor, typ, gathering) in &actors {
        let Some(tool) = gathering.filter(|_| !holding.contains(&actor)).and_then(|g| tool_of(g.0)) else { continue };
        let path = format!("models/{tool}-{}.glb", actor_name(*typ));
        commands.spawn((
            Held { tool, bound: false, moved: Vec::new() },
            Name::new(format!("held:{tool}")),
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))),
            ChildOf(actor),
        ));
    }
}

/// Hangs each unbound tool from its holder's rig once both scenes are
/// spawned: its node moves onto the socket it names.
pub fn bind_held(
    mut commands: Commands,
    mut held: Query<(Entity, &ChildOf, &mut Held)>,
    worn: Query<Entity, With<Worn>>,
    actors: Query<&EntityType>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    anchors: Query<&SocketAnchor>,
) {
    let pieces: HashSet<Entity> = worn.iter().collect();
    for (piece, child_of, mut h) in held.iter_mut().filter(|(_, _, h)| !h.bound) {
        let actor = child_of.parent();
        let Ok(typ) = actors.get(actor) else { continue };
        let Some(rig) = rig(actor, actor_name(*typ), &children, &parents, &names, &pieces) else { continue };
        let joints = joints_of(rig, &children, &names);
        let Some(moved) = hang(piece, &joints, &children, &anchors, &mut commands) else { continue };
        h.moved = moved;
        h.bound = true;
    }
}
