//! The highway: the local player's reaction queue as a rhythm game's lanes,
//! seen in perspective from above the hit line at the bottom of the screen.
//!
//! Three lanes by kind, left to right: blows, wounds, auto-attacks
//! ([`Lane`]). A note stands at its time left, at one speed, so it reaches
//! the hit line as it lands, and grows as it nears it. Every note shows its
//! damage. A band across the lanes, as deep as the span, marks what a
//! reaction pressed now takes, from the hit line or from the threat
//! Awareness's capstone snaps it to, and every note in it carries a white
//! rim. A cleared note shatters
//! where it stands, broken; a landed one pulses on the line in its lane's
//! colour with a flash down its lane.
//!
//! [`scale`] and [`rise`] are the one projection: the notes here and the
//! lanes the shader draws (`shaders/highway.wgsl`) are placed by it.

use std::time::Duration;

use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use common_bevy::components::reaction_queue::{Lane, QueuedThreat, ReactionQueue};
use common_bevy::components::resources::{CombatState, Health};
use common_bevy::components::ActorAttributes;

use crate::components::{ResolvedThreatsContainer, ViewHud, Viewed};
use crate::systems::threat_icons::{estimate, severity, severity_rgb, DOT_COLOR};
use common_bevy::tuning::Tuning;

/// Half the highway's width at the hit line, in pixels
const HALF_WIDTH: f32 = 70.0;
/// How far the far end stands above the hit line, in pixels
const RISE: f32 = 190.0;
/// How far the hit line stands above the highway's foot, room for a note
/// landing on it
const BASE: f32 = 24.0;
/// Perspective: a note at the far end is `1 / (1 + DEPTH)` its size at the line
const DEPTH: f32 = 2.0;
/// A note's width at the hit line
const NOTE: f32 = 32.0;
/// Clear of the resource bars below it
const ABOVE_BARS: f32 = 30.0;
/// Where across the screen the highway's middle stands, as a share of its
/// width: where the close camera, over the right shoulder, stands the
/// player, beside what it watches rather than over it.
const ACROSS: f32 = 28.0;

/// The time the highway spans, far end to hit line: half as long again as
/// the base reaction window, so every note moves before it lands. A note
/// with longer to wait holds at the far end.
const SPAN: Duration = Duration::from_millis(4500);

/// How long a note whose threat has gone with nothing known of how waits
/// before it shatters: one that left the view with its actor, never told.
pub(crate) const UNSURE: f32 = 0.15;

/// The shader's parameters; every length is the node's, in pixels.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct HighwayUniform {
    pub depth: f32,
    pub span: f32,
    pub base: f32,
    pub unused: f32,
    /// The band a reaction takes from, as seconds from landing: from x,
    /// where it starts (the hit line, or a snapped threat), to y, the span
    /// past that. No band where y is not past x.
    pub band: Vec4,
    /// Each lane's landing flash, left to right in x, y and z, fading from 1
    pub flash: Vec4,
    /// Each lane's colour, linear, left to right: [`lane_color`]
    pub blow: Vec4,
    pub wound: Vec4,
    pub pressure: Vec4,
}

#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub struct HighwayMaterial {
    #[uniform(0)]
    pub highway: HighwayUniform,
}

impl UiMaterial for HighwayMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/highway.wgsl".into()
    }
}

#[derive(Component)]
pub struct Highway;

/// A threat's note: which threat, where and how it was last drawn, so it
/// can go there when the threat does, and when its threat went with no
/// damage seen, if it has.
#[derive(Component)]
pub struct Note {
    key: (Entity, Duration, Lane),
    centre: Vec2,
    size: f32,
    color: Color,
    unsure: Option<f32>,
}

#[derive(Component)]
pub struct NoteLabel;

/// A landed note, spreading from where it landed as it fades
#[derive(Component)]
pub struct Pulse {
    at: Vec2,
    born: f32,
    size: f32,
    color: Color,
}

/// A fragment of a shattered note, flying out from where it broke
#[derive(Component)]
pub struct Shard {
    from: Vec2,
    velocity: Vec2,
    born: f32,
    size: f32,
    color: Color,
}

/// On-screen scale at depth `d`, 0 at the hit line to 1 at the far end
pub fn scale(d: f32) -> f32 {
    1.0 / (1.0 + DEPTH * d)
}

/// A note's label is drawn once at the font size of its largest note and
/// scaled from there: Bevy keeps a glyph atlas for every font size it is
/// asked for and frees none, so a label whose font size moved with its
/// note made an atlas each frame it moved.
const LABEL_FONT: f32 = NOTE * 0.36;

/// The scale of a label on a note `size` across, which never shows
/// smaller than an 8 px font.
fn label_scale(size: f32) -> Vec2 {
    Vec2::splat((size * 0.36).max(8.0) / LABEL_FONT)
}

/// Height above the hit line at depth `d`
pub fn rise(d: f32) -> f32 {
    RISE * (1.0 - scale(d)) / (1.0 - scale(1.0))
}

/// Depth of a note `remaining` from landing
pub fn depth(remaining: Duration) -> f32 {
    (remaining.as_secs_f32() / SPAN.as_secs_f32()).min(1.0)
}

/// How much of a note shows at depth `d`: the far part fades, where it
/// crosses the player
pub fn fade(d: f32) -> f32 {
    let t = ((d - 0.35) / 0.65).clamp(0.0, 1.0);
    1.0 - 0.65 * t * t * (3.0 - 2.0 * t)
}

/// The colour of a lane, which says its threats' kind: the lane is tinted
/// with it and a landing flashes and shatters in it, while a note's own
/// colour says how hard it hits.
pub fn lane_color(lane: Lane) -> Color {
    match lane {
        Lane::Blow => Color::srgb(0.95, 0.4, 0.15),
        Lane::Wound => DOT_COLOR,
        Lane::AutoAttack => Color::srgb(0.8, 0.75, 0.6),
    }
}

fn lane_index(lane: Lane) -> f32 {
    match lane {
        Lane::Blow => 0.0,
        Lane::Wound => 1.0,
        Lane::AutoAttack => 2.0,
    }
}

/// Centre of a note in `lane` at depth `d`: across from the highway's left
/// edge, up from its foot
fn centre(lane: Lane, d: f32) -> Vec2 {
    let across = (lane_index(lane) - 1.0) * (2.0 / 3.0) * HALF_WIDTH * scale(d);
    Vec2::new(HALF_WIDTH + across, BASE + rise(d))
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    color.with_alpha(color.alpha() * alpha)
}

pub fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<HighwayMaterial>>,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");
    let material = materials.add(HighwayMaterial {
        highway: HighwayUniform {
            depth: DEPTH,
            span: SPAN.as_secs_f32(),
            base: BASE,
            blow: lane_color(Lane::Blow).to_linear().to_vec4(),
            wound: lane_color(Lane::Wound).to_linear().to_vec4(),
            pressure: lane_color(Lane::AutoAttack).to_linear().to_vec4(),
            ..default()
        },
    });

    // On the resource bars' line at any size, left of them where the close
    // camera stands the player
    commands
        .spawn((
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::FlexEnd,
                padding: UiRect { left: Val::Percent(ACROSS), bottom: Val::Vw(crate::systems::resource_bars::LINE_VW), ..default() },
                ..default()
            },
            Pickable::IGNORE,
            ViewHud,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(2.0 * HALF_WIDTH),
                        height: Val::Px(RISE + BASE),
                        margin: UiRect { left: Val::Px(-HALF_WIDTH), bottom: Val::Px(ABOVE_BARS), ..default() },
                        ..default()
                    },
                    MaterialNode(material),
                    Visibility::Hidden,
                    Highway,
                ))
                .with_children(|parent| {
                    // What lands stacks beside the hit line, newest nearest it
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(2.0 * HALF_WIDTH + 12.0),
                            bottom: Val::Px(BASE - 15.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        ResolvedThreatsContainer,
                    ));
                });
        });
}

/// Place a note for every threat in the viewed actor's queue; pulse the
/// note of a threat gone from it that landed, and shatter one cleared.
pub fn update(
    tuning: Res<Tuning>,
    mut commands: Commands,
    player_query: Query<(Entity, &ReactionQueue, &ActorAttributes, &Health, Option<&CombatState>), With<Viewed>>,
    mut highway_query: Query<(Entity, &mut Visibility, &MaterialNode<HighwayMaterial>), With<Highway>>,
    mut materials: ResMut<Assets<HighwayMaterial>>,
    mut note_query: Query<(Entity, &mut Note, &mut Node, &mut BackgroundColor, &mut BorderColor, &Children), Without<Highway>>,
    mut label_query: Query<(&mut Text, &mut UiTransform, &mut TextColor), With<NoteLabel>>,
    gone: Res<crate::systems::combat::Gone>,
    time: Res<Time>,
    server: Res<crate::resources::Server>,
) {
    let Ok((highway, mut visibility, material)) = highway_query.single_mut() else {
        return;
    };
    let Some((viewed, queue, attrs, health, combat)) = player_query.iter().next() else {
        visibility.set_if_neq(Visibility::Hidden);
        for (entity, ..) in &note_query {
            commands.entity(entity).despawn();
        }
        return;
    };

    let now_ms = server.current_time(time.elapsed().as_millis());
    let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
    let shown = !queue.is_empty() || combat.is_some_and(|c| c.in_combat);
    visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });

    let key = |t: &QueuedThreat| (t.source, t.inserted_at, t.lane());
    let mut drawn = Vec::with_capacity(queue.threats.len());

    // The band a reaction pressed now takes from: at the hit line, or from
    // the threat Awareness's capstone snaps it to, drawn where it starts
    let span = attrs.span(&tuning);
    let band_at = queue.band(now, attrs.awareness_snap(&tuning));
    let from = band_at.saturating_sub(now).as_secs_f32();
    let band = Vec4::new(from, from + span.as_secs_f32(), 0.0, 0.0);
    if materials.get(&material.0).is_some_and(|lit| lit.highway.band != band) {
        if let Some(mut lit) = materials.get_mut(&material.0) {
            lit.highway.band = band;
        }
    }

    for (entity, mut note, mut node, mut background, mut border, children) in &mut note_query {
        let Some(threat) = queue.threats.iter().find(|t| key(t) == note.key) else {
            let at = time.elapsed_secs();
            let landed = gone.landed(viewed, note.key.0, note.key.1);
            if landed == Some(true) {
                let lane = note.key.2;
                if let Some(mut lit) = materials.get_mut(&material.0) {
                    let flash = &mut lit.highway.flash;
                    match lane {
                        Lane::Blow => flash.x = 1.0,
                        Lane::Wound => flash.y = 1.0,
                        Lane::AutoAttack => flash.z = 1.0,
                    }
                }
                pulse(&mut commands, highway, note.centre, note.size, lane_color(lane), at);
            } else if landed == Some(false) || at - *note.unsure.get_or_insert(at) >= UNSURE {
                shatter(&mut commands, highway, note.centre, note.size, note.color, at);
            } else {
                continue;
            }
            commands.entity(entity).despawn();
            continue;
        };
        drawn.push(note.key);

        let d = depth(threat.lands_at().saturating_sub(now));
        let taken = threat.in_band(band_at, span);
        let (fill, rim, label) = look(threat, attrs, health, taken);
        let alpha = fade(d);
        let size = NOTE * scale(d);
        let at = centre(threat.lane(), d);

        node.width = Val::Px(size);
        node.height = Val::Px(size);
        node.left = Val::Px(at.x - size / 2.0);
        node.bottom = Val::Px(at.y - size / 2.0);
        node.border = UiRect::all(Val::Px(if taken { 3.0 } else { 2.0 }));
        background.0 = with_alpha(fill, alpha);
        *border = BorderColor::all(with_alpha(rim, alpha));
        note.centre = at;
        note.size = size;
        note.color = fill;

        for child in children.iter() {
            if let Ok((mut text, mut transform, mut color)) = label_query.get_mut(child) {
                if **text != label {
                    **text = label.clone();
                }
                transform.scale = label_scale(size);
                color.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
            }
        }
    }

    for threat in queue.threats.iter().filter(|t| !drawn.contains(&key(t))) {
        let d = depth(threat.lands_at().saturating_sub(now));
        let size = NOTE * scale(d);
        let at = centre(threat.lane(), d);
        let (fill, rim, label) = look(threat, attrs, health, false);
        commands.entity(highway).with_children(|parent| {
            parent
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(size),
                        height: Val::Px(size),
                        left: Val::Px(at.x - size / 2.0),
                        bottom: Val::Px(at.y - size / 2.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Percent(50.)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(with_alpha(fill, fade(d))),
                    BorderColor::all(with_alpha(rim, fade(d))),
                    Note { key: key(threat), centre: at, size, color: fill, unsure: None },
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new(label),
                        TextFont { font_size: FontSize::Px(LABEL_FONT), ..default() },
                        UiTransform { scale: label_scale(size), ..default() },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, fade(d))),
                        NoteLabel,
                    ));
                });
        });
    }
}

/// A note's fill, rim and label: its colour says how hard it hits, its lane
/// what kind it is, and its label its damage. One a reaction would take is
/// `taken`, and rimmed white.
pub(crate) fn look(threat: &QueuedThreat, attrs: &ActorAttributes, health: &Health, taken: bool) -> (Color, Color, String) {
    let rim = if taken { Color::WHITE } else { Color::srgba(0.1, 0.08, 0.06, 0.9) };
    let (r, g, b) = severity_rgb(severity(threat, attrs, health));
    (Color::srgb(r, g, b), rim, format!("{:.0}", estimate(threat, attrs)))
}

/// How long a landed note's pulse spreads, and how wide it ends, as a
/// share of the note's width
const PULSE: f32 = 0.35;
const PULSE_SPREAD: f32 = 2.2;
/// How many shards a cleared note breaks into, and how long they fly
const SHARDS: usize = 7;
pub(crate) const SHARDS_FLY: f32 = 0.45;

/// A landed note, of width `size`, pulses: a disc in `color` spreading
/// from `at` in `parent`'s box as it fades.
pub(crate) fn pulse(commands: &mut Commands, parent: Entity, at: Vec2, size: f32, color: Color, now: f32) {
    commands.entity(parent).with_children(|children| {
        children.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(size),
                height: Val::Px(size),
                left: Val::Px(at.x - size / 2.0),
                bottom: Val::Px(at.y - size / 2.0),
                border_radius: BorderRadius::all(Val::Percent(50.)),
                ..default()
            },
            BackgroundColor(color),
            Pulse { at, born: now, size, color },
        ));
    });
}

/// A cleared note, of width `size`, breaks: shards in its `color`, children
/// of `parent`, flying out from `from` in its box.
pub(crate) fn shatter(commands: &mut Commands, parent: Entity, from: Vec2, size: f32, color: Color, now: f32) {
    commands.entity(parent).with_children(|children| {
        for i in 0..SHARDS {
            let angle = std::f32::consts::TAU * (i as f32 + 0.5) / SHARDS as f32;
            let velocity = Vec2::new(angle.cos(), angle.sin()) * (110.0 + 40.0 * (i % 3) as f32);
            let shard = size * 0.22;
            children.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(shard),
                    height: Val::Px(shard),
                    left: Val::Px(from.x - shard / 2.0),
                    bottom: Val::Px(from.y - shard / 2.0),
                    ..default()
                },
                BackgroundColor(color),
                Shard { from, velocity, born: now, size: shard, color },
            ));
        }
    });
}

/// Spread each landed note's pulse and fade it
pub fn update_pulses(
    mut commands: Commands,
    mut pulse_query: Query<(Entity, &Pulse, &mut Node, &mut BackgroundColor)>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for (entity, pulse, mut node, mut background) in &mut pulse_query {
        let k = (now - pulse.born) / PULSE;
        if k >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let spread = 1.0 - (1.0 - k).powi(2);
        let size = pulse.size * (1.0 + (PULSE_SPREAD - 1.0) * spread);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        node.left = Val::Px(pulse.at.x - size / 2.0);
        node.bottom = Val::Px(pulse.at.y - size / 2.0);
        background.0 = with_alpha(pulse.color, 0.9 * (1.0 - k));
    }
}

/// Fly shards out and fade them, and fade each lane's landing flash
pub fn update_shards(
    mut commands: Commands,
    mut shard_query: Query<(Entity, &Shard, &mut Node, &mut BackgroundColor)>,
    highway_query: Query<&MaterialNode<HighwayMaterial>, With<Highway>>,
    mut materials: ResMut<Assets<HighwayMaterial>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for (entity, shard, mut node, mut background) in &mut shard_query {
        let t = now - shard.born;
        if t >= SHARDS_FLY {
            commands.entity(entity).despawn();
            continue;
        }
        let k = t / SHARDS_FLY;
        let at = shard.from + shard.velocity * t * (1.0 - 0.5 * k);
        let size = shard.size * (1.0 - 0.6 * k);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        node.left = Val::Px(at.x - size / 2.0);
        node.bottom = Val::Px(at.y - size / 2.0);
        background.0 = with_alpha(shard.color, 1.0 - k);
    }

    let Ok(material) = highway_query.single() else {
        return;
    };
    let decay = (-6.0 * time.delta_secs()).exp();
    if let Some(mut lit) = materials.get_mut(&material.0) {
        let flash = &mut lit.highway.flash;
        if flash.max_element() > 0.001 {
            *flash *= decay;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_lands_on_the_hit_line_at_full_size() {
        assert_eq!(rise(depth(Duration::ZERO)), 0.0);
        assert_eq!(scale(0.0), 1.0);
    }

    #[test]
    fn a_note_rises_and_shrinks_with_time_left() {
        let steps: Vec<f32> = (0..=18).map(|i| i as f32 * 0.25).collect();
        for pair in steps.windows(2) {
            let (near, far) = (depth(Duration::from_secs_f32(pair[0])), depth(Duration::from_secs_f32(pair[1])));
            assert!(rise(far) > rise(near));
            assert!(scale(far) < scale(near));
        }
    }

    #[test]
    fn the_far_end_is_the_top() {
        assert!((rise(1.0) - RISE).abs() < 1e-3);
        assert_eq!(depth(SPAN * 2), 1.0, "a longer wait holds at the far end");
    }

    #[test]
    fn lanes_run_left_to_right_in_reaction_order() {
        let x = |lane| centre(lane, 0.5).x;
        assert!(x(Lane::Blow) < x(Lane::Wound) && x(Lane::Wound) < x(Lane::AutoAttack));
    }

    #[test]
    fn lanes_converge_toward_the_far_end() {
        let gap = |d| centre(Lane::AutoAttack, d).x - centre(Lane::Blow, d).x;
        assert!(gap(1.0) < gap(0.0));
    }

    #[test]
    fn the_far_part_fades() {
        assert_eq!(fade(0.0), 1.0);
        assert!(fade(1.0) < fade(0.5) && fade(0.5) <= fade(0.2));
    }
}
