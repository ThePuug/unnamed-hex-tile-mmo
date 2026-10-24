//! A player's horse on the client: / asks the server to get on it or off
//! it, and an actor the server says is mounted (`Status::mounted`) is drawn
//! on one. The horse's scene is a child of the actor, the rider's rig hangs
//! from its `socket.saddle` with its rest pelvis on the socket's point,
//! and the animator plays the horse's gaits in the rider's place while the
//! rider sits its `mounted` clip, which lifts the hips so the seat lands on
//! the saddle; a body without one holds its idle.

use std::collections::HashSet;

use bevy::{gltf::GltfNode, prelude::*, world_serialization::WorldAssetRoot};

use common_bevy::{
    components::{behaviour::PlayerControlled, entity_type::EntityType, status::Status, Actor},
    message::{Event, Try},
};

use crate::{
    components::Animates,
    systems::{
        actor::{actor_name, animate},
        animator::{Clip, Clips, Jumping, Lope, Rig},
        equipment::{rig, Worn},
    },
};

/// The key that gets on the horse, or off it.
pub const KEYCODE_MOUNT: KeyCode = KeyCode::Slash;

/// The horse's GLB, the same body under every rider.
const HORSE: &str = "actors/horse-basic.glb";

/// The rider's joint that sits on the saddle.
const SEAT: &str = "pelvis";

/// The horse under a mounted actor: its scene, a child of the actor, and
/// once the rider is seated on it, where.
#[derive(Component)]
pub struct Mount {
    horse: Entity,
    seated: Option<Seated>,
}

/// A rider on its horse: the horse's `AnimationPlayer`, and the rider's rig
/// with the parent and transform it had standing, which it takes back on
/// getting down.
struct Seated {
    player: Entity,
    rider: Entity,
    parent: Entity,
    standing: Transform,
}

impl Mount {
    /// The `AnimationPlayer` the animator drives the actor's gaits on, the
    /// horse's once the rider is seated.
    pub fn driven(&self) -> Option<Entity> {
        self.seated.as_ref().map(|s| s.player)
    }
}

/// Asks the server to get the player on its horse or off it when / is
/// pressed; the menu holds it as it holds every gameplay key.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    player: Query<(Entity, Option<&Status>), (With<Actor>, With<PlayerControlled>)>,
    mut writer: MessageWriter<Try>,
) {
    if menu.open {
        return;
    }
    // A view has no character, and no horse
    let Ok((ent, status)) = player.single() else { return };
    if !keys.pressed(KEYCODE_MOUNT, "Get on your horse, or off it") {
        return;
    }
    let on = !status.is_some_and(|s| s.mounted);
    info!("mount: asking to get {}", if on { "on" } else { "off" });
    writer.write(Try { event: Event::Mount { ent, on } });
}

/// Brings a horse under each actor the server says is mounted, once its own
/// scene is animated, and takes it away from each that got down: the rider
/// stands back where it stood.
pub fn saddle(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    actors: Query<(Entity, Option<&Status>, Option<&Mount>), With<Animates>>,
) {
    for (actor, status, mount) in &actors {
        let mounted = status.is_some_and(|s| s.mounted);
        match mount {
            None if mounted => {
                let horse = commands.spawn((
                    Name::new("mount:horse"),
                    WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(HORSE))),
                    Rig(asset_server.load(HORSE)),
                    Transform::IDENTITY,
                    ChildOf(actor),
                )).id();
                commands.entity(actor).insert(Mount { horse, seated: None });
                info!("mount: a horse under {actor}");
            }
            Some(mount) if !mounted => {
                if let Some(seated) = &mount.seated {
                    commands.entity(seated.rider).insert((ChildOf(seated.parent), seated.standing));
                }
                commands.entity(mount.horse).despawn();
                // The gait and the jump were the horse's
                commands.entity(actor).remove::<(Mount, Jumping, Lope)>();
                info!("mount: {actor} got down");
            }
            _ => {}
        }
    }
}

/// Seats each rider on its horse once both scenes are spawned: the horse's
/// player gets its clips, the rider's rig moves onto the saddle socket with
/// its seat on the socket's point at rest, and the rider sits its
/// `mounted` clip, or its idle where its asset has none.
#[allow(clippy::too_many_arguments)]
pub fn seat(
    mut commands: Commands,
    mut actors: Query<(Entity, &EntityType, &Animates, &Rig, &mut Mount)>,
    horses: Query<&Rig>,
    mut players: Query<&mut AnimationPlayer>,
    mut riders: Query<(&mut AnimationTransitions, &Clips)>,
    worn: Query<Entity, With<Worn>>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let pieces: HashSet<Entity> = worn.iter().collect();
    for (actor, &typ, animates, body_rig, mut mount) in actors.iter_mut().filter(|(.., m)| m.seated.is_none()) {
        let horse = mount.horse;
        let (Ok(horse_rig), Some(body_gltf)) = (horses.get(horse), gltfs.get(&body_rig.0)) else { continue };
        let Some(horse_gltf) = gltfs.get(&horse_rig.0) else { continue };
        let Some(player) = children.iter_descendants(horse).find(|&e| players.contains(e)) else { continue };
        let Some(socket) = children.iter_descendants(horse)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "socket.saddle")) else { continue };
        let body = actor_name(typ);
        let Some(rider) = rig(actor, body, &children, &parents, &names, &pieces) else { continue };
        let (Some(saddle), Some(seat)) = (rest(horse_gltf, &nodes, "horse", "socket.saddle"), rest(body_gltf, &nodes, body, SEAT)) else {
            warn!("{body} has no {SEAT}, or the horse no saddle; it stays afoot");
            continue;
        };
        let (Ok(parent), Ok(&standing)) = (parents.get(rider), transforms.get(rider)) else { continue };

        let Ok(mut horse_player) = players.get_mut(player) else { continue };
        animate(player, &mut horse_player, horse_gltf, &nodes, &mut graphs, &mut commands);
        if let (Ok(mut rider_player), Ok((mut transitions, clips))) = (players.get_mut(animates.0), riders.get_mut(animates.0)) {
            if let Some(sitting) = clips.node(Clip::Mounted).or(clips.node(Clip::Idle)) {
                transitions.play(&mut rider_player, sitting, std::time::Duration::from_millis(300)).set_speed(1.).repeat();
            }
        }

        // The rig upright in the horse's frame, its seat on the saddle's
        // point, expressed under the socket
        let upright = Mat4::from_translation(saddle.w_axis.truncate() - seat.w_axis.truncate()) * standing.to_matrix();
        commands.entity(rider).insert((ChildOf(socket), Transform::from_matrix(saddle.inverse() * upright)));
        commands.entity(actor).remove::<(Jumping, Lope)>();
        mount.seated = Some(Seated { player, rider, parent: parent.parent(), standing });
        info!("mount: {actor} seated");
    }
}

/// The rest transform of the node named `target` in the frame of the scene
/// the node named `root` stands in, the root's own transform included.
fn rest(gltf: &Gltf, nodes: &Assets<GltfNode>, root: &str, target: &str) -> Option<Mat4> {
    fn find(handle: &Handle<GltfNode>, above: Mat4, target: &str, nodes: &Assets<GltfNode>) -> Option<Mat4> {
        let node = nodes.get(handle)?;
        let here = above * node.transform.to_matrix();
        if node.name == target {
            return Some(here);
        }
        node.children.iter().find_map(|child| find(child, here, target, nodes))
    }
    find(gltf.named_nodes.get(root)?, Mat4::IDENTITY, target, nodes)
}
