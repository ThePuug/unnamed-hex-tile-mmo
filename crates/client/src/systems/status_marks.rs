//! The marks of an actor's status: the stance it holds, the block it has
//! up and each hold on it, in one row wherever that actor's bars are
//! drawn. [`marks`] is the one reading of a `Status`; [`Marks`] keeps it on
//! the actor with each mark's share of its time left, and every row
//! ([`row`]) draws from that.

use bevy::prelude::*;

use common_bevy::{
    components::{ally_target::AllyTarget, status::{Stance, Status, Timed}, target::Target},
    message::AbilityType,
};

use crate::{components::Viewed, systems::{action_bar, target_frame::Lane}};

/// What a mark says of its actor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Stance(Stance),
    /// A block up, by the skill that put it up
    Block(AbilityType),
    Stunned,
    Slowed,
    Rooted,
    Pinned,
}

/// One mark and the seconds it has left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub kind: Kind,
    pub left: f32,
}

/// The marks `status` shows, in the order a row draws them: the stance
/// held, the block up, then each hold.
pub fn marks(status: &Status) -> Vec<Mark> {
    let stance = status.held_stance()
        .and_then(|stance| Some(Mark { kind: Kind::Stance(stance), left: status.stance(stance)?.remaining }));
    let block = status.blocking().map(|block| Mark { kind: Kind::Block(block.ability), left: block.remaining });
    let hold = |kind, slot: Option<Timed>| slot.filter(|timed| timed.remaining > 0.0).map(|timed| Mark { kind, left: timed.remaining });
    stance.into_iter()
        .chain(block)
        .chain(hold(Kind::Stunned, status.held))
        .chain(hold(Kind::Slowed, status.slow))
        .chain(hold(Kind::Rooted, status.root))
        .chain(hold(Kind::Pinned, status.pinned))
        .collect()
}

/// The skill that takes `stance`, whose icon marks it.
fn skill(stance: Stance) -> AbilityType {
    match stance {
        Stance::PerfectStride => AbilityType::PerfectStride,
        Stance::Bloodlust => AbilityType::Bloodlust,
        Stance::Brace => AbilityType::Brace,
        Stance::Surge => AbilityType::Surge,
        Stance::Vigil => AbilityType::Vigil,
        Stance::Lurk => AbilityType::Lurk,
    }
}

/// How a mark is drawn: a skill's icon, or a hold's word.
enum Face {
    Icon(AbilityType),
    Word(&'static str),
}

impl Kind {
    fn face(self) -> Face {
        match self {
            Kind::Stance(stance) => Face::Icon(skill(stance)),
            Kind::Block(ability) => Face::Icon(ability),
            Kind::Stunned => Face::Word("Stunned"),
            Kind::Slowed => Face::Word("Slowed"),
            Kind::Rooted => Face::Word("Rooted"),
            Kind::Pinned => Face::Word("Pinned"),
        }
    }

    /// The chip's border: its sides in px, its corners' rounding and its
    /// colour. Each kind differs from the rest in shape as well as colour:
    /// a stance rounded, a block square, a stun boxed heavy, a slow boxed
    /// thin and rounded, a root barred underneath, a pin barred at both
    /// sides. The rounding stays small: the shade is clipped to the chip's
    /// rectangle and would show past a rounder corner.
    fn border(self) -> (UiRect, f32, Color) {
        match self {
            Kind::Stance(_) => (UiRect::all(Val::Px(2.)), ROUND_PX, Color::srgb(0.90, 0.76, 0.20)),
            Kind::Block(_) => (UiRect::all(Val::Px(2.)), 0., Color::srgb(0.45, 0.62, 0.85)),
            Kind::Stunned => (UiRect::all(Val::Px(2.)), 0., Color::srgb(0.95, 0.65, 0.15)),
            Kind::Slowed => (UiRect::all(Val::Px(1.)), ROUND_PX, Color::srgb(0.55, 0.80, 0.95)),
            Kind::Rooted => (UiRect::bottom(Val::Px(3.)), 0., Color::srgb(0.45, 0.75, 0.35)),
            Kind::Pinned => (UiRect::horizontal(Val::Px(3.)), 0., Color::srgb(0.75, 0.50, 0.90)),
        }
    }
}

/// An actor's marks as they stand, each with the longest it has been seen
/// to have left, which is the whole its drain is measured against: a
/// `Status` carries what remains of an effect and not its length.
#[derive(Component, Debug, Default)]
pub struct Marks(Vec<(Mark, f32)>);

impl Marks {
    /// Takes the marks as `status` has them now. A mark that was standing
    /// keeps its span unless it now has longer left, which is a fresh one
    /// laid over it.
    fn take(&mut self, status: &Status) {
        let before = std::mem::take(&mut self.0);
        self.0 = marks(status).into_iter()
            .map(|mark| {
                let span = before.iter().find(|(was, _)| was.kind == mark.kind).map_or(0.0, |&(_, span)| span);
                (mark, span.max(mark.left))
            })
            .collect();
    }

    /// Each mark with the share of its time it has left, 1 whole to 0 spent
    pub fn shares(&self) -> impl Iterator<Item = (Kind, f32)> + '_ {
        self.0.iter().map(|&(mark, span)| (mark.kind, if span > 0.0 { (mark.left / span).clamp(0.0, 1.0) } else { 0.0 }))
    }
}

/// Keeps every actor's [`Marks`] to its `Status`, read again whenever the
/// status changes: as the server sends it, and each tick an effect counts
/// down.
pub fn mark(mut commands: Commands, mut actors: Query<(Entity, Ref<Status>, Option<&mut Marks>)>) {
    for (ent, status, standing) in &mut actors {
        match standing {
            Some(mut standing) => {
                if status.is_changed() {
                    standing.take(&status);
                }
            }
            None => {
                let mut standing = Marks::default();
                standing.take(&status);
                commands.entity(ent).try_insert(standing);
            }
        }
    }
}

/// Whose marks a row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Of {
    /// The viewed actor's own
    Viewed,
    /// The target the viewed actor holds in a lane now, as the bars over
    /// it in the world follow
    Held(Lane),
    /// The last target of a lane, as its frame shows
    Last(Lane),
}

/// A row of marks, and what it last drew: a chip to each, in order.
#[derive(Component)]
pub struct MarkRow {
    of: Of,
    shown: Vec<(Kind, f32)>,
}

/// The shade over the spent part of a chip of `row`, the `index`th: hung
/// from the top, so the mark drains downward as its time runs.
#[derive(Component)]
pub struct MarkShade {
    row: Entity,
    index: usize,
}

/// Height of a chip, and the side of one that holds an icon, in px
pub const CHIP_PX: f32 = 20.;
const GAP_PX: f32 = 2.;
const ROUND_PX: f32 = 6.;

/// A row of `of`'s marks under `parent`, laid out from `at`: empty, and
/// one chip high, until that actor has a mark.
pub fn row(parent: &mut ChildSpawnerCommands, of: Of, at: Node) {
    parent.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(GAP_PX),
            height: Val::Px(CHIP_PX),
            ..at
        },
        MarkRow { of, shown: Vec::new() },
    ));
}

/// The height of the shade over a chip with `share` of its time left
fn shade(share: f32) -> Val {
    Val::Percent((1.0 - share) * 100.0)
}

fn chip(parent: &mut ChildSpawnerCommands, row: Entity, index: usize, kind: Kind, share: f32, assets: Option<&AssetServer>) {
    let (border, radius, colour) = kind.border();
    let face = kind.face();
    let node = match face {
        Face::Icon(_) => Node { width: Val::Px(CHIP_PX), ..default() },
        Face::Word(_) => Node { padding: UiRect::horizontal(Val::Px(4.)), ..default() },
    };
    parent.spawn((
        Node {
            height: Val::Px(CHIP_PX),
            flex_shrink: 0.,
            border,
            border_radius: BorderRadius::all(Val::Px(radius)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
            ..node
        },
        BorderColor::all(colour),
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
    ))
    .with_children(|parent| {
        match face {
            Face::Icon(ability) => {
                if let Some(assets) = assets {
                    parent.spawn((
                        ImageNode::new(assets.load(action_bar::icon_path(ability))),
                        Node { width: Val::Percent(100.), height: Val::Percent(100.), ..default() },
                    ));
                }
            }
            Face::Word(word) => {
                parent.spawn((
                    Text::new(word),
                    TextFont { font_size: FontSize::Px(10.0), ..default() },
                    TextColor(Color::WHITE),
                ));
            }
        }
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: shade(share),
                top: Val::Px(0.),
                left: Val::Px(0.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            MarkShade { row, index },
        ));
    });
}

/// Draws every row from its actor's [`Marks`]: its chips made anew when
/// the marks standing change, and each chip's shade let down as its time
/// runs. A row whose actor is gone, or has no status, stands empty.
pub fn update_rows(
    mut commands: Commands,
    mut rows: Query<(Entity, &mut MarkRow)>,
    mut shades: Query<(&MarkShade, &mut Node)>,
    viewed: Query<(Entity, &Target, Option<&AllyTarget>), With<Viewed>>,
    standing: Query<&Marks>,
    assets: Option<Res<AssetServer>>,
) {
    let viewed = viewed.single().ok();
    for (ent, mut row) in &mut rows {
        let actor = viewed.and_then(|(own, target, ally)| match row.of {
            Of::Viewed => Some(own),
            Of::Held(lane) => lane.held(target, ally),
            Of::Last(lane) => lane.target(target, ally),
        });
        let now: Vec<(Kind, f32)> = actor.and_then(|actor| standing.get(actor).ok()).map_or(Vec::new(), |marks| marks.shares().collect());
        let same = now.len() == row.shown.len() && now.iter().zip(&row.shown).all(|(now, shown)| now.0 == shown.0);
        if !same {
            commands.entity(ent).despawn_related::<Children>().with_children(|parent| {
                for (index, &(kind, share)) in now.iter().enumerate() {
                    chip(parent, ent, index, kind, share, assets.as_deref());
                }
            });
        }
        if row.shown != now {
            row.shown = now;
        }
    }
    for (shade_of, mut node) in &mut shades {
        let Some(&(_, share)) = rows.get(shade_of.row).ok().and_then(|(_, row)| row.shown.get(shade_of.index)) else { continue };
        let height = shade(share);
        if node.height != height {
            node.height = height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::components::status::Block;

    fn timed(remaining: f32) -> Option<Timed> {
        Some(Timed { pace: 1.0, remaining })
    }

    fn fighting() -> Status {
        Status {
            vigil: timed(8.0),
            block: Some(Block { ability: AbilityType::Parry, attacks: 1, remaining: 2.0, power: 0.0 }),
            held: timed(1.0),
            pinned: timed(3.0),
            ..default()
        }
    }

    #[test]
    fn a_status_is_marked_stance_then_block_then_holds() {
        let kinds: Vec<Kind> = marks(&fighting()).into_iter().map(|mark| mark.kind).collect();
        assert_eq!(kinds, vec![Kind::Stance(Stance::Vigil), Kind::Block(AbilityType::Parry), Kind::Stunned, Kind::Pinned]);
    }

    #[test]
    fn an_idle_status_has_no_mark() {
        assert!(marks(&Status::default()).is_empty());
        assert!(marks(&Status { burden: true, mounted: true, ..default() }).is_empty(), "what has no time to run is no mark");
        let spent = Status { block: Some(Block { ability: AbilityType::Parry, attacks: 0, remaining: 2.0, power: 0.0 }), ..default() };
        assert!(marks(&spent).is_empty(), "nor a block that stops nothing more");
    }

    #[test]
    fn every_stance_is_marked_by_the_skill_that_takes_it() {
        for stance in Stance::ALL {
            assert_eq!(Stance::of(skill(stance)), Some(stance));
        }
    }

    #[test]
    fn a_mark_drains_as_its_time_runs_and_goes_with_its_effect() {
        let mut status = fighting();
        let mut standing = Marks::default();
        standing.take(&status);
        assert!(standing.shares().all(|(_, share)| share == 1.0), "whole as it is first seen");

        let mut last: Vec<f32> = standing.shares().map(|(_, share)| share).collect();
        for _ in 0..3 {
            status.tick(0.25);
            standing.take(&status);
            let now: Vec<f32> = standing.shares().map(|(_, share)| share).collect();
            assert!(now.iter().zip(&last).all(|(now, last)| now < last), "each falls as its time runs");
            last = now;
        }

        status.tick(0.5);
        standing.take(&status);
        assert!(standing.shares().all(|(kind, _)| kind != Kind::Stunned), "a spent hold's mark goes");
        assert_eq!(standing.shares().count(), 3, "and the rest stand");

        status.hold(4.0);
        standing.take(&status);
        let stun = standing.shares().find(|(kind, _)| *kind == Kind::Stunned).unwrap();
        assert_eq!(stun.1, 1.0, "a fresh one starts whole");
    }

    #[test]
    fn a_longer_effect_laid_over_a_mark_fills_it_again() {
        let mut status = Status::default();
        status.root(2.0);
        let mut standing = Marks::default();
        standing.take(&status);
        status.tick(1.0);
        standing.take(&status);
        assert!(standing.shares().next().unwrap().1 < 1.0);
        status.root(5.0);
        standing.take(&status);
        assert_eq!(standing.shares().next().unwrap().1, 1.0);
    }

    #[test]
    fn a_row_holds_a_chip_to_each_mark_of_its_actor() {
        let mut app = App::new();
        app.add_systems(Update, (mark, update_rows).chain());
        let foe = app.world_mut().spawn(fighting()).id();
        app.world_mut().spawn((Status::default(), Viewed, Target { entity: Some(foe), ..default() }));
        let root = app.world_mut().spawn_empty().id();
        app.world_mut().commands().entity(root).with_children(|parent| {
            row(parent, Of::Held(Lane::Hostile), Node::default());
            row(parent, Of::Viewed, Node::default());
            row(parent, Of::Held(Lane::Ally), Node::default());
        });
        app.world_mut().flush();

        let chips = |app: &mut App| {
            let mut rows = app.world_mut().query::<(&MarkRow, Option<&Children>)>();
            let mut counts: Vec<(Of, usize)> = rows.iter(app.world()).map(|(row, children)| (row.of, children.map_or(0, |children| children.len()))).collect();
            counts.sort_by_key(|(of, _)| format!("{of:?}"));
            counts
        };
        app.update();
        app.update();
        assert_eq!(chips(&mut app), vec![(Of::Held(Lane::Ally), 0), (Of::Held(Lane::Hostile), 4), (Of::Viewed, 0)], "the foe's four over the foe, none for an idle actor or an empty lane");

        app.world_mut().get_mut::<Status>(foe).unwrap().tick(1.5);
        app.update();
        app.update();
        assert_eq!(chips(&mut app)[1], (Of::Held(Lane::Hostile), 3), "a mark goes with its effect");
        let mut shades = app.world_mut().query::<(&MarkShade, &Node)>();
        assert!(shades.iter(app.world()).all(|(_, node)| matches!(node.height, Val::Percent(spent) if spent > 0.0)), "and the rest have drained");
    }
}
