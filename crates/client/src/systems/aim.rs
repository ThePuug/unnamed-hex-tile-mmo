//! What the gather key and the interact key act on: one thing, the
//! nearest in the player's reach of everything either key acts on
//! ([`aim`]), marked while it is ([`mark`]). G gathers it where it is a
//! tree, a boulder or reeds, or opens it where it is a pile; T claims it
//! where it is a hearth or a home, looks into it where it is a wagon or a
//! storehouse, and hails it where it is a settler. The other key does
//! nothing: neither reaches past the thing marked for one of its own.

use bevy::{ecs::system::SystemParam, prelude::*};
use common::building::{self, Fixture};
use common_bevy::{
    components::{
        entity_type::{actor::ActorIdentity, EntityType},
        heading::Heading,
        position::Position,
        Actor,
    },
    geometry::{flat_top_tile_center, slot_point},
    resources::map::Map,
};
use qrz::{Convert, Qrz};

use crate::{
    plugins::cover::{outline::MarkedActor, Mark, Marked},
    resources::RenderOrigin,
    systems::settlers::Kept,
};

/// What a key does to the thing aimed at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Acts {
    /// G gathers it, or opens the pile.
    Gather,
    /// T claims it: a hearth to return to or a home to live in, not the
    /// player's already.
    Claim,
    /// T looks into it, to move materials.
    Look(Kept),
    /// T hails it, this settler.
    Hail(Entity),
}

/// The thing a key acts on: the tile and slot the server is asked about,
/// the tile the thing stands on, where it stands there, xz in world units
/// from the centre of the tile the player's position is measured from,
/// and what a key does to it. A home on more tiles than one is asked
/// about on the tile of it in reach, and stands on the tile that holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub tile: Qrz,
    pub slot: usize,
    pub stands: Qrz,
    pub at: Vec2,
    pub acts: Acts,
}

/// The nearest thing a key acts on in reach of a player at `position`
/// facing `heading`, that returns to `returns` and lives at `lives`: cover
/// on the tile it stands on or one of the three of its front half, and a
/// settler within `building::HAIL` tiles any way, each as the server
/// reaches. A hearth or a home the player has already is nothing to act
/// on, and each of `settlers` is where a settler stands. Measured
/// in the frame of `position`'s tile, so it picks the same however far
/// out the world is.
pub fn aim(map: &Map, position: &Position, heading: Heading, returns: Option<Qrz>, lives: Option<Qrz>, settlers: impl IntoIterator<Item = (Entity, Position)>) -> Option<Aim> {
    let here = position.reached(map);
    let from = position.offset.xz();
    let frame = (position.tile.q, position.tile.r);
    let centre = |tile: Qrz| Vec2::from(flat_top_tile_center(tile.q - frame.0, tile.r - frame.1, 1.0));
    let reach: Vec<Qrz> = std::iter::once(here)
        .chain(heading.front_dirs().map(|dir| here + dir))
        .filter_map(|tile| map.get_by_qr(tile.q, tile.r))
        .map(|(tile, _)| tile)
        .collect();
    let mut found: Vec<(f32, Aim)> = Vec::new();
    for &tile in &reach {
        let cover = map.cover_at(tile.q, tile.r);
        for slot in 0..common::TILE_SLOTS as usize {
            let gathered = common::gathering::anchor(cover, slot) == slot && common::gathering::reachable(cover, slot) && !building::used(cover, slot);
            if gathered {
                let (x, z) = slot_point(cover, (tile.q, tile.r), slot, frame);
                let at = Vec2::new(x, z);
                found.push((at.distance_squared(from), Aim { tile, slot, stands: tile, at, acts: Acts::Gather }));
            }
        }
        // A building on more tiles than one is the same building from any of them
        let (q, r) = building::holder(|(q, r)| map.cover_at(q, r), (tile.q, tile.r));
        let Some((stands, _)) = map.get_by_qr(q, r) else { continue };
        let held = map.cover_at(q, r);
        let slot = building::SLOT;
        let acts = match building::fixture(held) {
            Some(Fixture::Wagon) => Acts::Look(Kept::Wagon),
            Some(Fixture::Storehouse) => Acts::Look(Kept::Store),
            _ if building::used(held, slot) && !crate::systems::building::claimed(held, slot, (q, r), returns, lives) => Acts::Claim,
            _ => continue,
        };
        found.push((centre(tile).distance_squared(from), Aim { tile, slot, stands, at: centre(stands), acts }));
    }
    for (who, settler) in settlers {
        let on = settler.reached(map);
        if qrz::hex_distance((here.q, here.r), (on.q, on.r)) > building::HAIL {
            continue;
        }
        let Some((stands, _)) = map.get_by_qr(on.q, on.r) else { continue };
        let between: Vec3 = map.convert(settler.tile - position.tile);
        let at = (between + settler.offset).xz();
        found.push((at.distance_squared(from), Aim { tile: stands, slot: building::SLOT, stands, at, acts: Acts::Hail(who) }));
    }
    found.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, aim)| aim)
}

/// The local player's aim, for every system that acts on it or shows it:
/// one rule, read where the player sees itself (`movement::Predicted`).
#[derive(SystemParam)]
pub struct Aiming<'w, 's> {
    map: Res<'w, Map>,
    predicted: crate::systems::movement::Predicted<'w, 's>,
    player: Query<'w, 's, &'static Heading, (With<Actor>, With<Position>)>,
    others: Query<'w, 's, (Entity, &'static Position, &'static EntityType), Without<Actor>>,
    home: Res<'w, crate::systems::building::HomeHearth>,
}

impl Aiming<'_, '_> {
    /// Where the player stands and faces; none for a view, which has no
    /// character.
    fn stood(&self) -> Option<(Position, Heading)> {
        self.predicted.position().zip(self.player.single().ok().copied())
    }

    /// What the keys act on for a player that lives at `lives`, and
    /// where the player stands, the frame the aim is told in.
    pub fn aim(&self, lives: Option<Qrz>) -> Option<(Position, Aim)> {
        let (position, heading) = self.stood()?;
        let settlers = self.others.iter().filter(|(.., typ)| matches!(typ, EntityType::Actor(actor) if matches!(actor.identity, ActorIdentity::Settler))).map(|(who, at, _)| (who, *at));
        aim(&self.map, &position, heading, self.home.0, lives, settlers).map(|aim| (position, aim))
    }
}

/// Marks what the keys act on to be drawn outlined (`cover::outline`):
/// the home an order carried to it would change, else the tile the open
/// build panel builds on, else the aim, the cover it is or the settler.
#[allow(clippy::too_many_arguments)]
pub fn mark(
    mut marked: ResMut<Marked>,
    mut actor: ResMut<MarkedActor>,
    aiming: Aiming,
    claims: Res<crate::systems::building::Claims>,
    origin: Res<RenderOrigin>,
    build: Res<crate::systems::building::BuildPanel>,
    ghosts: Res<crate::systems::building::Ghosts>,
    asked: Res<crate::systems::settlers::Asked>,
) {
    let map = &*aiming.map;
    let mut hailed = None;
    let mark = aiming.stood().and_then(|(position, heading)| {
        // A tile's centre, in the frame of the player's tile
        let centred = |(q, r): (i32, i32)| {
            let tile = map.get_by_qr(q, r)?.0;
            Some((tile, Vec2::from(flat_top_tile_center(tile.q - position.tile.q, tile.r - position.tile.r, 1.0))))
        };
        let (stands, at) = match asked.order {
            Some(building::Order::Upgrade(q, r) | building::Order::Downgrade(q, r)) => centred(building::holder(|(q, r)| map.cover_at(q, r), (q, r)))?,
            // A fixture offered is shown where it would stand, not on the tile faced
            _ if build.open => {
                let faced = crate::systems::building::faced(map, &position, heading);
                centred(build.ghost().map(|(tile, _)| tile).or_else(|| ghosts.shown(build.choices()).filter(|_| !build.placing())).unwrap_or((faced.q, faced.r)))?
            }
            _ => {
                let (_, aim) = aiming.aim(claims.lives)?;
                if let Acts::Hail(who) = aim.acts {
                    hailed = Some(who);
                    return None;
                }
                (aim.stands, aim.at)
            }
        };
        let rise = common_bevy::systems::movement::surface_y_from(position.tile, at, stands, map);
        Some(Mark { foot: origin.render_tile(map, position.tile) + Vec3::new(at.x, rise, at.y), tile: stands })
    });
    marked.set_if_neq(Marked(mark));
    actor.set_if_neq(MarkedActor(hailed));
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Content;
    use common_bevy::components::entity_type::decorator::Decorator;

    fn map_with(tiles: &[(Qrz, common::Cover)]) -> Map {
        let map = Map::new(qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE));
        for &(qrz, cover) in tiles {
            map.insert(qrz, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
        map
    }

    /// The keys reach the tile underfoot and the three of the front half,
    /// never one behind, and act on the nearest thing there.
    #[test]
    fn the_aim_is_the_nearest_in_reach() {
        let here = Qrz { q: 0, r: 0, z: 0 };
        let heading = Heading::NORTH;
        let ahead = here + heading.hex_dir();
        let behind = here + heading.reversed().hex_dir();
        let trees = common::Cover::NONE.with(0, Content::Pine).with(1, Content::Pine);
        let crag = common::Cover::NONE.with_boulder(0).with_rock(common::Rock::Sandstone);
        let aimed = |map: &Map| aim(map, &Position::at_tile(here), heading, None, None, []);

        let map = map_with(&[(here, common::Cover::NONE), (ahead, crag), (behind, trees)]);
        let found = aimed(&map).unwrap();
        assert_eq!((found.tile, found.slot, found.acts), (ahead, 0, Acts::Gather), "the boulder ahead, not the trees behind");

        let map = map_with(&[(here, trees), (ahead, crag), (behind, trees)]);
        let found = aimed(&map).unwrap();
        assert_eq!(found.tile, here, "a tree underfoot is nearer than the boulder ahead");
        assert!(common::SITE_SLOTS.iter().any(|s| s[0] == found.slot), "a tree is named by its first slot");

        let map = map_with(&[(here, common::Cover::NONE), (behind, trees)]);
        assert_eq!(aimed(&map), None);
    }

    /// One thing has both keys: a settler nearer than the boulder ahead
    /// is what is aimed at, behind the player as beside it, and one
    /// further off than the boulder or out of hail is not.
    #[test]
    fn a_settler_in_reach_is_aimed_at_as_cover_is() {
        let here = Qrz { q: 0, r: 0, z: 0 };
        let heading = Heading::NORTH;
        let ahead = here + heading.hex_dir();
        let behind = here + heading.reversed().hex_dir();
        let crag = common::Cover::NONE.with_boulder(0).with_rock(common::Rock::Sandstone);
        let far = behind + heading.reversed().hex_dir();
        let gone = far + heading.reversed().hex_dir();
        let bare = common::Cover::NONE;
        let who = Entity::PLACEHOLDER;
        let aimed = |tiles: &[(Qrz, common::Cover)], settler: Qrz| aim(&map_with(tiles), &Position::at_tile(here), heading, None, None, [(who, Position::at_tile(settler))]).map(|aim| aim.acts);
        let wooded = [(here, bare), (ahead, crag), (behind, bare), (far, bare), (gone, bare)];
        assert_eq!(aimed(&wooded, here), Some(Acts::Hail(who)), "the settler beside the player, nearer than the boulder");
        assert_eq!(aimed(&wooded, far), Some(Acts::Gather), "the boulder, nearer than the settler two tiles behind");
        let open = [(here, bare), (ahead, bare), (behind, bare), (far, bare), (gone, bare)];
        assert_eq!(aimed(&open, far), Some(Acts::Hail(who)), "a settler is hailed from two tiles, any way the player faces");
        assert_eq!(aimed(&open, gone), None, "and not from three");
    }
}
