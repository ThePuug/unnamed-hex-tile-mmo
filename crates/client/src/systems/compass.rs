//! The compass: a strip across the top of the screen, centred on the
//! bearing the camera looks along, a mark for every heading the player can
//! turn to. One press of a turn key moves it one mark. The six tile-to-tile
//! bearings stand taller, the cardinals carry their letters, a bracket
//! spans the arc the viewed actor strikes within, and a pin stands at each
//! target's bearing and the haven's, held at the edge while it is off the
//! strip. Each den the server has told of stands at its bearing too, and
//! drops off the strip with the rest of what lies behind.

use bevy::prelude::*;
use bevy::math::Rot2;
use bevy::ui::UiTransform;

use common_bevy::{
    components::{ally_target::AllyTarget, heading::*, position::VisualPosition, target::Target},
    haven::HAVEN_LOCATION,
    resources::map::Map,
    systems::targeting::STRIDE_ARC,
};
use qrz::Qrz;

use crate::{
    components::{ViewHud, Viewed},
    resources::RenderOrigin,
    systems::{camera::CameraPose, den::Dens, target_frame::{Lane, LANES}},
};

/// The strip's width and height, in px.
const BAR_PX: f32 = 600.;
const BAR_HEIGHT_PX: f32 = 40.;
/// Degrees of bearing the strip spans, half either side of its centre.
const SPAN_DEGREES: f32 = 180.;
const PX_PER_DEGREE: f32 = BAR_PX / SPAN_DEGREES;
/// Width of the column a bearing's mark and letter stand centred in.
const MARK_PX: f32 = 20.;
const PIN_PX: f32 = 10.;
const DEN_PX: f32 = 8.;
const HAVEN_PX: f32 = 12.;
const BRACKET_PX: f32 = 6.;
/// The line the pins stand centred on, in px down from the strip's top.
const PIN_ROW_PX: f32 = 20.;

const GOLD: Color = Color::srgb(0.85, 0.65, 0.13);

/// What a node on the strip stands for, and so where it goes.
#[derive(Component, Clone, Copy)]
pub enum Mark {
    /// A fixed bearing, in degrees clockwise from north
    Bearing(f32),
    /// The arc the viewed actor strikes within, about its heading
    Facing,
    /// The target the viewed actor holds in a lane
    Pin(Lane),
    /// The den whose pack stands on a tile
    Den(Qrz),
    Haven,
}

impl Mark {
    /// Its width on the strip, in px.
    fn px(self) -> f32 {
        match self {
            Mark::Bearing(_) => MARK_PX,
            Mark::Facing => 2. * STRIDE_ARC * PX_PER_DEGREE,
            Mark::Pin(_) => PIN_PX,
            Mark::Den(_) => DEN_PX,
            Mark::Haven => HAVEN_PX,
        }
    }

    /// Whether it is held at the strip's edge while its bearing is off it.
    fn held(self) -> bool {
        matches!(self, Mark::Pin(_) | Mark::Haven)
    }
}

/// The node the dens' pins stand in, beneath every other pin.
#[derive(Component)]
pub struct DenPins;

/// The bearing in degrees clockwise from north of a ground-plane direction
/// given as (x, z), north at -z.
fn bearing(xz: Vec2) -> f32 {
    xz.x.atan2(-xz.y).to_degrees().rem_euclid(360.)
}

/// The bearing the camera looks along at orbit `yaw`, which runs
/// counter-clockwise from behind a player facing north.
fn view_bearing(yaw: f32) -> f32 {
    (-yaw).to_degrees().rem_euclid(360.)
}

/// Degrees from `view` to `bearing`, the shorter way round: negative left.
fn relative(bearing: f32, view: f32) -> f32 {
    let d = (bearing - view).rem_euclid(360.);
    if d > 180. { d - 360. } else { d }
}

/// Px from the strip's left edge of a bearing `rel` degrees off its centre.
fn px(rel: f32) -> f32 {
    BAR_PX / 2. + rel * PX_PER_DEGREE
}

pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.),
            top: Val::Px(10.),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        ViewHud,
    ))
    .with_children(|parent| {
        parent.spawn((
            Node {
                width: Val::Px(BAR_PX),
                height: Val::Px(BAR_HEIGHT_PX),
                border: UiRect::vertical(Val::Px(1.)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.7)),
            BorderColor::all(GOLD.with_alpha(0.6)),
        ))
        .with_children(|bar| {
            for slot in 0..HEADING_SLOTS {
                spawn_bearing(bar, Heading::from_slot(slot));
            }

            // Filled by `sync_dens`
            bar.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
                DenPins,
            ));

            bar.spawn(pin(Mark::Haven, GOLD, Color::WHITE));

            // The bracket: open at the top, its ends standing up
            bar.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(0.),
                    width: Val::Px(Mark::Facing.px()),
                    height: Val::Px(BRACKET_PX),
                    border: UiRect { left: Val::Px(2.), right: Val::Px(2.), bottom: Val::Px(2.), ..default() },
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BorderColor::all(GOLD),
                Mark::Facing,
            ))
            .with_children(|bracket| {
                // The heading itself
                bracket.spawn((
                    Node { width: Val::Px(2.), height: Val::Percent(100.), ..default() },
                    BackgroundColor(GOLD),
                ));
            });

            for lane in LANES {
                let color = match lane {
                    Lane::Hostile => Color::srgb(0.9, 0.5, 0.0),
                    Lane::Ally => Color::srgb(0.2, 0.8, 0.2),
                };
                bar.spawn((
                    pin(Mark::Pin(lane), color, Color::BLACK),
                    UiTransform { rotation: Rot2::degrees(45.), ..default() },
                ));
            }
        });
    });
}

/// A heading's column: a mark, taller on a tile-to-tile bearing, and under
/// it the letter of a cardinal.
fn spawn_bearing(bar: &mut ChildSpawnerCommands, heading: Heading) {
    let degrees = heading.degrees();
    let hex = heading.slot() % (HEADING_SLOTS / 6) == 0;
    let quarter = HEADING_SLOTS / 4;
    let letter = (heading.slot() % quarter == 0).then(|| ["N", "E", "S", "W"][(heading.slot() / quarter) as usize]);
    bar.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.),
            width: Val::Px(MARK_PX),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        Mark::Bearing(degrees),
    ))
    .with_children(|column| {
        column.spawn((
            Node {
                width: Val::Px(if hex { 2. } else { 1. }),
                height: Val::Px(if hex { 12. } else { 6. }),
                ..default()
            },
            BackgroundColor(if hex { Color::srgb(0.85, 0.85, 0.85) } else { Color::srgb(0.55, 0.55, 0.55) }),
        ));
        if let Some(letter) = letter {
            column.spawn((
                Text::new(letter),
                TextFont { font_size: FontSize::Px(14.), ..default() },
                TextColor(if letter == "N" { Color::srgb(1.0, 0.2, 0.2) } else { Color::srgb(0.9, 0.9, 0.9) }),
            ));
        }
    });
}

/// A square pin for `mark`, its size the mark's, hidden until `update`
/// places it.
fn pin(mark: Mark, fill: Color, border: Color) -> impl Bundle {
    let side = mark.px();
    (
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(PIN_ROW_PX - side / 2.),
            width: Val::Px(side),
            height: Val::Px(side),
            border: UiRect::all(Val::Px(1.)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
        Visibility::Hidden,
        mark,
    )
}

/// Stands a pin for each den the server has told of, anew whenever they
/// change: red while its pack stands, grey once cleared.
pub fn sync_dens(mut commands: Commands, dens: Res<Dens>, pins: Query<Entity, With<DenPins>>) {
    if !dens.is_changed() {
        return;
    }
    let Ok(pins) = pins.single() else { return };
    commands.entity(pins).despawn_related::<Children>();
    commands.entity(pins).with_children(|pins| {
        for (&at, look) in dens.0.iter() {
            let fill = if look.cleared { Color::srgb(0.45, 0.45, 0.45) } else { Color::srgb(0.75, 0.15, 0.1) };
            pins.spawn(pin(Mark::Den(at), fill, Color::BLACK));
        }
    });
}

/// Slides every mark to its bearing off the camera's: a pin to its
/// target's, its den's or the haven's from the viewed actor.
pub fn update(
    camera: Res<CameraPose>,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
    viewed: Query<(&Heading, &VisualPosition, &Target, Option<&AllyTarget>), With<Viewed>>,
    positions: Query<&VisualPosition>,
    mut marks: Query<(&Mark, &mut Node, &mut Visibility)>,
) {
    let view = view_bearing(camera.pose.yaw);
    let seer = viewed.single().ok();
    // Degrees off the strip's centre of a rendered point, from the viewed actor
    let toward = |to: Option<Vec3>| {
        let to = to? - seer?.1.current();
        let xz = Vec2::new(to.x, to.z);
        (xz.length_squared() > f32::EPSILON).then(|| relative(bearing(xz), view))
    };
    for (&mark, mut node, mut visibility) in &mut marks {
        let rel = match mark {
            Mark::Bearing(degrees) => Some(relative(degrees, view)),
            Mark::Facing => seer.map(|(heading, ..)| relative(heading.degrees(), view)),
            Mark::Pin(lane) => toward(
                seer.and_then(|(_, _, target, ally)| positions.get(lane.held(target, ally)?).ok())
                    .map(VisualPosition::current),
            ),
            Mark::Den(tile) => toward(Some(origin.render_tile(&map, tile))),
            Mark::Haven => toward(Some(origin.render_tile(&map, HAVEN_LOCATION))),
        };
        let Some(rel) = rel else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        let half = mark.px() / 2.;
        let x = if mark.held() { px(rel).clamp(half + 2., BAR_PX - half - 2.) } else { px(rel) };
        node.left = Val::Px(x - half);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_reads_its_own_bearing() {
        for slot in 0..HEADING_SLOTS {
            let heading = Heading::from_slot(slot);
            let read = bearing(heading.to_world_dir());
            assert!(relative(read, heading.degrees()).abs() < 1e-3, "slot {slot} read {read}");
        }
    }

    #[test]
    fn the_view_behind_a_heading_is_its_bearing() {
        // The orbit stop behind a heading turns counter-clockwise as the
        // heading turns clockwise.
        for slot in 0..HEADING_SLOTS {
            let heading = Heading::from_slot(slot);
            let yaw = ((HEADING_SLOTS - slot) % HEADING_SLOTS) as f32 * SLOT_DEGREES.to_radians();
            assert!(relative(view_bearing(yaw), heading.degrees()).abs() < 1e-3, "slot {slot}");
        }
    }

    #[test]
    fn relative_takes_the_short_way_round() {
        assert_eq!(relative(10., 350.), 20.);
        assert_eq!(relative(350., 10.), -20.);
        assert_eq!(relative(180., 0.), 180.);
    }
}
