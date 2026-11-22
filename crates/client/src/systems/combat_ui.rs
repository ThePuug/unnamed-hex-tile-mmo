use std::{collections::HashSet, time::Duration};

use bevy::prelude::*;
use common_bevy::{
    components::{activation::Activating, ally_target::AllyTarget, target::Target},
    message::{AbilityType, Do, Event as GameEvent, Outcome},
    moment::Moment,
};

use crate::{
    components::{ActivationBar, ActivationFill, ActivationName, Viewed, WorldBar, WorldBarFill},
    systems::{closeup::CloseupCamera, skill_text, struck::{outcome_color, outcome_word}, target_frame::{Lane, LANES}},
};

/// Where a `Node` goes to be drawn at `world`, or None where that is off
/// screen. The camera gives the window's pixels; a node's are the UI's,
/// which `UiScale` scales, so a node placed by the window's lands short of
/// its mark in a window smaller than the monitor.
pub(crate) fn node_at(camera: &Camera, camera_transform: &GlobalTransform, scale: &UiScale, world: Vec3) -> Option<Vec2> {
    camera.world_to_viewport(camera_transform, world).ok().map(|at| at / scale.0)
}

/// System to update floating text (damage numbers)
/// Projects world position to screen space, moves text upward, fades out, and despawns
pub fn update_floating_text(
    mut commands: Commands,
    mut query: Query<(Entity, &mut crate::components::FloatingText, &mut Node, &mut TextColor)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    scale: Res<UiScale>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    for (entity, mut floating_text, mut node, mut text_color) in &mut query {
        let elapsed = (Moment::ZERO + time.elapsed()).since(floating_text.spawn_time).as_secs_f32();

        // Check if lifetime expired
        if elapsed >= floating_text.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Move upward in world space
        let delta = time.delta_secs();
        floating_text.world_position.y += floating_text.velocity * delta;

        // Project world position to screen space
        if let Some(at) = node_at(camera, camera_transform, &scale, floating_text.world_position) {
            node.left = Val::Px(at.x);
            node.top = Val::Px(at.y);
        } else {
            // Position is behind camera or off-screen, hide it
            node.left = Val::Px(OFF_SCREEN);
        }

        // Fade out (alpha based on remaining lifetime)
        let alpha = 1.0 - (elapsed / floating_text.lifetime);
        text_color.0 = text_color.0.with_alpha(alpha);
    }
}

/// Width and height of a bar over an actor, in pixels
const BAR_WIDTH: f32 = 50.0;
const BAR_HEIGHT: f32 = 6.0;
/// How far over an actor's origin its bars stand
const OVER: f32 = 1.5;

/// The decay constant a bar's fill eases toward what it measures by
const FILL_SPEED: f32 = 5.0;

/// Where a node sits while what it follows is off screen
pub const OFF_SCREEN: f32 = -10000.0;

const TRACK: Color = Color::srgb(0.2, 0.2, 0.2);
/// The fill of a skill under way
const ACTIVATION: Color = Color::srgb(0.90, 0.76, 0.20);
/// The room an activation bar's name has, centred on its bar, and the gap
/// between the two
const NAME_WIDTH: f32 = 140.0;
const NAME_GAP: f32 = 2.0;

/// Builds, for each lane, the health bar drawn over its target in the
/// world. Hidden until the lane has a target, and moved onto it each frame.
pub fn setup_health_bars(mut commands: Commands) {
    for lane in LANES {
        let fill = match lane {
            Lane::Hostile => Color::srgb(0.9, 0.1, 0.1),
            Lane::Ally => Color::srgb(0.1, 0.9, 0.1),
        };
        let whole = Node {
            position_type: PositionType::Absolute,
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        };
        commands.spawn((
            Node { left: Val::Px(OFF_SCREEN), top: Val::Auto, ..whole.clone() },
            Visibility::Hidden,
            WorldBar { current_fill: 1.0 },
            lane,
        ))
        .with_children(|parent| {
            parent.spawn((whole.clone(), BackgroundColor(TRACK)));
            parent.spawn((whole.clone(), BackgroundColor(fill), ZIndex(1), WorldBarFill));
            // The target's stance, block and holds, standing on the bar
            crate::systems::status_marks::row(parent, crate::systems::status_marks::Of::Held(lane), Node {
                position_type: PositionType::Absolute,
                width: Val::Px(BAR_WIDTH),
                left: Val::Px(0.0),
                bottom: Val::Px(BAR_HEIGHT + NAME_GAP),
                ..default()
            });
        });
    }
}

/// Moves each lane's bar onto the target the viewed actor holds in it and
/// eases its fill toward that target's health. Hidden with no target.
pub fn update_world_bars(
    mut bars: Query<(&Lane, &mut WorldBar, &Children, &mut Node, &mut Visibility)>,
    mut fills: Query<&mut Node, (With<WorldBarFill>, Without<WorldBar>)>,
    targets: Query<(&common_bevy::components::resources::Health, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    viewed: Query<(&Target, Option<&AllyTarget>), With<Viewed>>,
    scale: Res<UiScale>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok((target, ally)) = viewed.single() else {
        return;
    };

    for (lane, mut bar, children, mut node, mut visibility) in &mut bars {
        let Some((health, transform)) = lane.held(target, ally).and_then(|ent| targets.get(ent).ok()) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;

        let measured = (health.state / health.max).clamp(0.0, 1.0);
        bar.current_fill = crate::systems::ease(bar.current_fill, measured, FILL_SPEED, time.delta_secs());

        // Over the target's head, centred on it
        let world_pos = transform.translation + Vec3::Y * OVER;
        let Some(at) = node_at(camera, camera_transform, &scale, world_pos) else {
            node.left = Val::Px(OFF_SCREEN);
            continue;
        };
        node.left = Val::Px(at.x - BAR_WIDTH / 2.0);
        node.top = Val::Px(at.y);
        for child in children.iter() {
            if let Ok(mut fill) = fills.get_mut(child) {
                fill.width = Val::Px(BAR_WIDTH * bar.current_fill);
            }
        }
    }
}

/// Makes `bar` an activation bar (`ActivationBar`) for `of`, laid out from
/// `at`: a track `track` px wide whose fill is how far through the skill
/// stands, and the skill's name, under the track over an actor and above
/// it for the viewed actor's own. Hidden until a skill is under way; what
/// it shows then is its [`Showing`].
pub fn activation_bar(mut bar: EntityCommands, of: Option<Entity>, track: f32, at: Node) {
    bar.insert((
        Node {
            width: Val::Px(NAME_WIDTH.max(track)),
            flex_direction: if of.is_some() { FlexDirection::Column } else { FlexDirection::ColumnReverse },
            align_items: AlignItems::Center,
            row_gap: Val::Px(NAME_GAP),
            ..at
        },
        Visibility::Hidden,
        ActivationBar(of),
        Showing::default(),
    ))
    .with_children(|parent| {
        parent.spawn((Node { width: Val::Px(track), height: Val::Px(BAR_HEIGHT), ..default() }, BackgroundColor(TRACK)))
            .with_children(|track| {
                track.spawn((Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(ACTIVATION), ActivationFill));
            });
        parent.spawn((
            Text::default(),
            TextFont { font_size: FontSize::Px(11.0), ..default() },
            TextColor(Color::WHITE),
            ActivationName,
        ));
    });
}

/// How long an activation bar holds its verdict, fading over the last half
const VERDICT: Duration = Duration::from_millis(600);
/// How long past its skill's time a bar waits, full, to hear how the skill
/// ended: the word of it comes a trip from the server after the clock does
const UNHEARD: Duration = Duration::from_millis(400);

/// What an activation bar shows, its moments on the client's clock
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub enum Showing {
    #[default]
    Nothing,
    /// A skill under way
    UnderWay(AbilityType),
    /// A skill whose time ran out, its ending awaited until the moment
    Unheard(AbilityType, Moment),
    /// How its skill ended, until the moment
    Verdict(Outcome, Moment),
}

impl Showing {
    /// What the bar shows at `now`: its actor activating `under_way`, and
    /// `ended` telling how a skill of that actor's resolved this frame.
    /// The skill it showed resolving gives its verdict, held for
    /// [`VERDICT`]; one whose time ran out unheard goes after [`UNHEARD`].
    pub fn next(self, under_way: Option<AbilityType>, ended: impl Fn(AbilityType) -> Option<Outcome>, now: Moment) -> Showing {
        let shown = match self {
            Showing::UnderWay(ability) | Showing::Unheard(ability, _) => Some(ability),
            _ => None,
        };
        // Before the skill under way: the word may come a frame before the
        // actor's `Activating` clears
        if let Some(outcome) = shown.and_then(&ended) {
            return Showing::Verdict(outcome, now + VERDICT);
        }
        if let Some(ability) = under_way {
            return Showing::UnderWay(ability);
        }
        match self {
            Showing::UnderWay(ability) => Showing::Unheard(ability, now + UNHEARD),
            Showing::Unheard(_, until) | Showing::Verdict(_, until) if now < until => self,
            _ => Showing::Nothing,
        }
    }
}

/// Shows every skill under way: over each actor activating one, a bar
/// under where its health bar stands, made as the skill begins; for the
/// viewed actor, the one bar above its skill bar. Each names its skill and
/// fills as the server's clock nears the moment it lands, then holds a
/// moment, full, in the colour of how the skill ended with the word for it
/// in the name's place (`struck::outcome_color`, `struck::outcome_word`),
/// and goes.
#[allow(clippy::too_many_arguments)]
pub fn update_activation_bars(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    actors: Query<(Entity, &Activating, &Transform, Has<Viewed>)>,
    mut bars: Query<(Entity, &ActivationBar, &mut Showing, &mut Node, &mut Visibility)>,
    children: Query<&Children>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), (With<ActivationFill>, Without<ActivationBar>)>,
    mut names: Query<(&mut Text, &mut TextColor), With<ActivationName>>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    scale: Res<UiScale>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    let now = server.now(time.elapsed().as_millis());
    let here = Moment::ZERO + time.elapsed();
    let camera = camera_query.single().ok();
    let own = actors.iter().find(|(.., viewed)| *viewed).map(|(actor, ..)| actor);
    let ended: Vec<(Entity, AbilityType, Outcome)> = reader.read()
        .filter_map(|message| match message.event {
            GameEvent::Resolved { ent, ability, outcome, .. } => Some((ent, ability, outcome)),
            _ => None,
        })
        .collect();

    let mut barred: HashSet<Entity> = HashSet::new();
    for (bar, shows, mut showing, mut node, mut visibility) in &mut bars {
        let of = match shows.0 {
            None => own.and_then(|own| actors.get(own).ok()),
            Some(actor) => actors.get(actor).ok().filter(|(.., viewed)| !viewed),
        };
        let activation = of.and_then(|(_, activating, ..)| activating.0);
        let next = match of {
            Some((actor, ..)) => showing.next(
                activation.map(|activation| activation.ability),
                |ability| ended.iter().find(|(ent, of, _)| (*ent, *of) == (actor, ability)).map(|(.., outcome)| *outcome),
                here,
            ),
            None => Showing::Nothing,
        };
        showing.set_if_neq(next);
        // What it reads, how full, and in what colour
        let (label, full, color) = match (next, activation) {
            (Showing::UnderWay(ability), Some(activation)) => (skill_text::name(ability), activation.progress(now), ACTIVATION),
            (Showing::UnderWay(ability), None) | (Showing::Unheard(ability, _), _) => (skill_text::name(ability), 1.0, ACTIVATION),
            (Showing::Verdict(outcome, until), _) => {
                let left = until.since(here).as_secs_f32() / VERDICT.as_secs_f32();
                (outcome_word(outcome).to_string(), 1.0, outcome_color(outcome).with_alpha((left * 2.0).clamp(0.0, 1.0)))
            }
            (Showing::Nothing, _) => {
                match shows.0 {
                    Some(_) => commands.entity(bar).despawn(),
                    None => { visibility.set_if_neq(Visibility::Hidden); }
                }
                continue;
            }
        };
        barred.extend(shows.0);
        if let (Some(_), Some((_, _, transform, _))) = (shows.0, of) {
            let over = transform.translation + Vec3::Y * OVER;
            let at = camera.and_then(|(camera, transform)| node_at(camera, transform, &scale, over));
            match at {
                Some(at) => {
                    node.left = Val::Px(at.x - NAME_WIDTH / 2.0);
                    node.top = Val::Px(at.y + BAR_HEIGHT + NAME_GAP);
                }
                None => node.left = Val::Px(OFF_SCREEN),
            }
        }
        visibility.set_if_neq(Visibility::Inherited);
        let verdict = matches!(next, Showing::Verdict(..));
        for part in children.iter_descendants(bar) {
            if let Ok((mut fill, mut background)) = fills.get_mut(part) {
                fill.width = Val::Percent(full * 100.0);
                background.set_if_neq(BackgroundColor(color));
            }
            if let Ok((mut text, mut text_color)) = names.get_mut(part) {
                if text.0 != label {
                    text.0 = label.clone();
                }
                text_color.set_if_neq(TextColor(if verdict { color } else { Color::WHITE }));
            }
        }
    }

    for (actor, activating, _, viewed) in &actors {
        if activating.0.is_some() && !viewed && !barred.contains(&actor) {
            let at = Node { position_type: PositionType::Absolute, left: Val::Px(OFF_SCREEN), ..default() };
            activation_bar(commands.spawn_empty(), Some(actor), BAR_WIDTH, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use common_bevy::{components::activation::Activation, message::AbilityType};

    fn bars(app: &mut App) -> Vec<Option<Entity>> {
        let mut query = app.world_mut().query::<&ActivationBar>();
        query.iter(app.world()).map(|bar| bar.0).collect()
    }

    #[test]
    fn an_actor_has_its_bar_while_its_skill_is_under_way_and_the_viewed_one_keeps_to_its_own() {
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.init_resource::<UiScale>();
        app.insert_resource(crate::resources::Server::default());
        app.add_systems(Update, update_activation_bars);
        let under_way = Activating(Some(Activation { ability: AbilityType::Overpower, target: None, began: Moment::ZERO, length: Duration::from_secs(2) }));
        let foe = app.world_mut().spawn((under_way, Transform::default())).id();
        let viewed = app.world_mut().spawn((under_way, Transform::default(), Viewed)).id();
        app.world_mut().spawn((Activating(None), Transform::default()));
        let own = app.world_mut().spawn_empty().id();
        activation_bar(app.world_mut().commands().entity(own), None, 200.0, Node::default());
        app.world_mut().flush();

        app.update();
        app.update();
        let mut standing = bars(&mut app);
        standing.sort();
        assert_eq!(standing, vec![None, Some(foe)], "one over the foe, none over the viewed actor or the idle one");
        assert_eq!(*app.world().get::<Visibility>(own).unwrap(), Visibility::Inherited, "the viewed actor's own shows");
        let mut names = app.world_mut().query::<(&Text, &ActivationName)>();
        assert!(names.iter(app.world()).all(|(text, _)| text.0 == "Overpower"), "each naming its skill");

        // Each hears how its skill ended and holds the word for it
        let resolved = |ent, outcome| Do { event: GameEvent::Resolved { ent, ability: AbilityType::Overpower, target: None, damage: 0.0, outcome, dot: false } };
        app.world_mut().write_message(resolved(foe, Outcome::Blocked));
        app.world_mut().write_message(resolved(viewed, Outcome::Interrupted));
        for actor in [foe, viewed] {
            app.world_mut().get_mut::<Activating>(actor).unwrap().0 = None;
        }
        app.update();
        let mut words: Vec<String> = names.iter(app.world()).map(|(text, _)| text.0.clone()).collect();
        words.sort();
        assert_eq!(words, vec!["Blocked", "Interrupted"], "the verdict in the name's place");
        let mut fills = app.world_mut().query::<(&Node, &BackgroundColor, &ActivationFill)>();
        assert!(fills.iter(app.world()).all(|(fill, color, _)| fill.width == Val::Percent(100.0) && color.0 != ACTIVATION), "full, in the outcome's colour");

        app.world_mut().resource_mut::<Time>().advance_by(VERDICT);
        app.update();
        assert_eq!(bars(&mut app), vec![None], "gone once the verdict is told");
        assert_eq!(*app.world().get::<Visibility>(own).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn a_bar_holds_its_verdict_a_moment_and_one_unheard_just_goes() {
        let at = Moment::from_millis(5_000);
        let (skill, other) = (AbilityType::Overpower, AbilityType::AutoAttack);
        let none = |_| None;
        let under_way = Showing::Nothing.next(Some(skill), none, at);
        assert_eq!(under_way, Showing::UnderWay(skill));

        // Only its own skill resolving ends it, and before the actor's
        // activation is seen to clear
        let landed = |ability| (ability == skill).then_some(Outcome::Landed);
        assert_eq!(under_way.next(Some(skill), |ability| (ability == other).then_some(Outcome::Landed), at), under_way);
        let verdict = under_way.next(Some(skill), landed, at);
        assert_eq!(verdict, Showing::Verdict(Outcome::Landed, at + VERDICT));
        assert_eq!(verdict.next(None, none, at + VERDICT / 2), verdict, "held");
        assert_eq!(verdict.next(None, none, at + VERDICT), Showing::Nothing, "then gone");
        assert_eq!(verdict.next(Some(other), none, at), Showing::UnderWay(other), "or given up to the next skill");

        // Its time run out, it waits to hear, takes a late word, and goes
        // with none
        let unheard = under_way.next(None, none, at);
        assert_eq!(unheard, Showing::Unheard(skill, at + UNHEARD));
        assert!(matches!(unheard.next(None, landed, at + UNHEARD / 2), Showing::Verdict(Outcome::Landed, _)));
        assert_eq!(unheard.next(None, none, at + UNHEARD), Showing::Nothing);
    }
}

