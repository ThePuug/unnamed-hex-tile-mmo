//! A lurker is drawn as a ghost: each mesh under it wears its own copy of
//! its material, see-through, and wears the shared one again once the lurk
//! ends. The client holds a lurker only where its player may see it: its
//! own, an ally's, a foe's within its reach. The server keeps every other
//! from it.

use bevy::prelude::*;
use common_bevy::components::{status::Status, Actor};

use crate::plugins::cel::CelMaterial;
use crate::systems::struck::Flashing;

/// How much of a lurker's body shows
const GHOST_ALPHA: f32 = 0.35;

/// A body drawn as a ghost, until each of its meshes wears its own
/// material again
#[derive(Component)]
pub struct Ghost;

/// A mesh of a ghost, wearing its own see-through copy of its material:
/// the material it shares, to wear again after
#[derive(Component)]
pub struct Ghosted(Handle<CelMaterial>);

/// Draws each lurking body as a ghost, its meshes as they come, and each
/// ghost no longer lurking as it was. A flash wears its own copy of what
/// a mesh wore as it began, and puts that back as it fades: a mesh
/// flashing is left until it has.
pub fn ghost(
    mut commands: Commands,
    bodies: Query<(Entity, Option<&Status>, Has<Ghost>), With<Actor>>,
    children: Query<&Children>,
    meshes: Query<(&MeshMaterial3d<CelMaterial>, Option<&Ghosted>, Has<Flashing>)>,
    mut materials: ResMut<Assets<CelMaterial>>,
) {
    for (body, status, ghost) in &bodies {
        let lurking = status.is_some_and(Status::is_lurking);
        if !lurking && !ghost {
            continue;
        }
        let mut settled = true;
        for mesh in children.iter_descendants(body) {
            let Ok((worn, ghosted, flashing)) = meshes.get(mesh) else { continue };
            if lurking == ghosted.is_some() {
                continue;
            }
            if flashing {
                settled = false;
                continue;
            }
            match ghosted {
                Some(Ghosted(shared)) => {
                    commands.entity(mesh).remove::<Ghosted>().insert(MeshMaterial3d(shared.clone()));
                }
                None => {
                    let Some(mut own) = materials.get(&worn.0).cloned() else { continue };
                    own.base.base_color.set_alpha(GHOST_ALPHA);
                    own.base.alpha_mode = AlphaMode::Blend;
                    let own = materials.add(own);
                    commands.entity(mesh).insert((Ghosted(worn.0.clone()), MeshMaterial3d(own)));
                }
            }
        }
        if lurking && !ghost {
            commands.entity(body).insert(Ghost);
        } else if !lurking && settled {
            commands.entity(body).remove::<Ghost>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::components::status::Timed;

    #[test]
    fn a_lurker_is_drawn_see_through_until_its_lurk_ends() {
        let mut world = World::new();
        world.init_resource::<Assets<CelMaterial>>();
        let shared = world.resource_mut::<Assets<CelMaterial>>().add(CelMaterial::default());
        let lurking = Status { lurk: Some(Timed { pace: 1.0, remaining: 5.0 }), ..default() };
        let body = world.spawn((Actor, lurking)).id();
        let mesh = world.spawn((MeshMaterial3d(shared.clone()), ChildOf(body))).id();
        let standing = world.spawn(Actor).id();
        let plain = world.spawn((MeshMaterial3d(shared.clone()), ChildOf(standing))).id();

        world.run_system_once(ghost).unwrap();
        let worn = world.get::<MeshMaterial3d<CelMaterial>>(mesh).unwrap().0.clone();
        let look = world.resource::<Assets<CelMaterial>>().get(&worn).unwrap().base.clone();
        assert!(worn != shared && look.base_color.alpha() < 1.0 && look.alpha_mode == AlphaMode::Blend, "the lurker shows through");
        assert_eq!(world.get::<MeshMaterial3d<CelMaterial>>(plain).unwrap().0, shared, "one not lurking is drawn as it was");

        world.get_mut::<Status>(body).unwrap().lurk = None;
        world.run_system_once(ghost).unwrap();
        assert_eq!(world.get::<MeshMaterial3d<CelMaterial>>(mesh).unwrap().0, shared, "its lurk over, it wears its own again");
        assert!(world.get::<Ghost>(body).is_none());
    }
}
