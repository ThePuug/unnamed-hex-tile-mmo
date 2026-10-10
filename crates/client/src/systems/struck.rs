//! What a strike shows on what it struck. A strike of the viewed actor's
//! stands as a note over its target from the moment its threat is queued,
//! in the highway's look (`highway::look`), until it goes as a highway
//! note goes (`combat::Gone`): landed, it pulses in its lane's colour;
//! cleared, it shatters, broken. Whatever
//! lands, on whomever, flashes the body it lands on, and its damage rises
//! from it unless that body is the viewed actor's, whose resolved stack
//! shows it.
//!
//! A threat names its source by the server's id here (`renet::write_do`),
//! a landing by the client's, so the viewed actor is matched by each.

use std::time::Duration;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use common_bevy::components::reaction_queue::{Lane, ReactionQueue};
use common_bevy::components::resources::Health;
use common_bevy::message::{Do, Event as GameEvent};
use common_bevy::moment::Moment;

use crate::components::{FloatingText, Viewed};
use crate::resources::EntityMap;
use crate::plugins::cel::CelMaterial;
use crate::systems::{closeup::CloseupCamera, combat_ui::node_at, highway, threat_icons::DOT_COLOR};

/// How long a struck body's flash takes to fade
const FLASH: f32 = 0.2;
/// The light a flash adds at its peak to what the body's surface gives,
/// as drawn, unweighted by exposure
const FLASH_LIGHT: f32 = 0.6;

/// A note's width over its target, and the gap between two over one
const NOTE: f32 = 26.0;
const GAP: f32 = 4.0;
/// How far over a target's origin its notes stand: above its health bar
const OVER: f32 = 2.1;
/// How far over a target's origin its damage rises from
const RISES_FROM: f32 = 2.5;
/// How long a note takes to arrive, and how much bigger than its width it
/// arrives
const ARRIVAL: f32 = 0.15;
const ARRIVES_AT: f32 = 1.8;
/// A note's label is drawn at one size, its largest, and scaled: text
/// keeps a glyph atlas for every size it is drawn at, never freed
const LABEL_FONT: f32 = NOTE * ARRIVES_AT * 0.4;
/// How long a gone note lingers while it pulses or its shards fly
const GONE: Duration = Duration::from_millis(600);
use crate::systems::combat_ui::OFF_SCREEN;

/// When damage last landed on a body, while its flash lasts
#[derive(Component)]
pub struct Struck(Moment);

/// A mesh of a struck body, wearing its own copy of its material while it
/// flashes: the material it shares, to wear again after
#[derive(Component)]
pub struct Flashing(Handle<CelMaterial>);

/// A strike of the viewed actor's, standing over what it struck: which
/// threat, keyed as the highway keys a note, and when it went, if it has
#[derive(Component)]
pub struct Mark {
    struck: Entity,
    key: (Entity, Moment, Lane),
    lands_at: Moment,
    color: Color,
    born: Moment,
    unsure: Option<Moment>,
    gone: Option<Moment>,
}

#[derive(Component)]
pub struct MarkLabel;

/// A strike of the viewed actor's on another gets its note as its threat
/// is queued.
pub fn on_contact(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    viewed: Query<Entity, With<Viewed>>,
    l2r: Res<EntityMap>,
    time: Res<Time>,
) {
    let viewed = viewed.single().ok();
    let on_server = viewed.and_then(|viewed| l2r.get_by_left(&viewed).copied());
    let now = Moment::ZERO + time.elapsed();
    for message in reader.read() {
        let GameEvent::InsertThreat { ent, threat } = message.event else { continue };
        if on_server != Some(threat.source) || viewed == Some(ent) {
            continue;
        }
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(NOTE),
                    height: Val::Px(NOTE),
                    left: Val::Px(OFF_SCREEN),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Percent(50.)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Mark {
                    struck: ent,
                    key: (threat.source, threat.inserted_at, threat.lane()),
                    lands_at: threat.lands_at(),
                    color: Color::NONE,
                    born: now,
                    unsure: None,
                    gone: None,
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::default(),
                    TextFont { font_size: FontSize::Px(LABEL_FONT), ..default() },
                    UiTransform { scale: Vec2::ONE, ..default() },
                    TextColor(Color::WHITE),
                    MarkLabel,
                ));
            });
    }
}

/// As damage lands, the body it lands on flashes, and on an actor other
/// than the viewed one it rises from it.
pub fn on_landing(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    viewed: Query<Entity, With<Viewed>>,
    bodies: Query<&Transform>,
    time: Res<Time>,
) {
    let viewed = viewed.single().ok();
    let now = Moment::ZERO + time.elapsed();
    for message in reader.read() {
        let GameEvent::ApplyDamage { ent, damage, dot, .. } = message.event else { continue };
        let Ok(mut struck) = commands.get_entity(ent) else { continue };
        struck.try_insert(Struck(now));
        // What lands on the viewed actor shows in its resolved stack
        if viewed == Some(ent) {
            continue;
        }
        // A body stays for its death pose, so the dead one struck has its Transform
        let Ok(body) = bodies.get(ent) else { continue };
        commands.spawn((
            Node { position_type: PositionType::Absolute, ..default() },
            Text::new(format!("{damage:.0}")),
            TextFont { font_size: FontSize::Px(32.0), ..default() },
            TextColor(if dot { DOT_COLOR } else { Color::WHITE }),
            TextLayout::justify(Justify::Center),
            FloatingText {
                spawn_time: Moment::ZERO + time.elapsed(),
                world_position: body.translation + Vec3::Y * RISES_FROM,
                lifetime: 1.5,
                velocity: 1.0,
            },
        ));
    }
}

/// Stands each note over its target, in a row ordered by when each lands,
/// and colours it as the highway would; as its threat goes, pulses it if
/// it landed and shatters it if not (`combat::Gone`), and takes it down
/// once it has.
#[allow(clippy::too_many_arguments)]
pub fn update_marks(
    mut commands: Commands,
    mut marks: Query<(Entity, &mut Mark, &mut Node, &mut BackgroundColor, &mut BorderColor, &Children)>,
    mut labels: Query<(&mut Text, &mut UiTransform, &mut Visibility), With<MarkLabel>>,
    targets: Query<(&ReactionQueue, &Health, &Transform)>,
    camera_query: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<CloseupCamera>)>,
    scale: Res<UiScale>,
    gone: Res<crate::systems::combat::Gone>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let now = Moment::ZERO + time.elapsed();

    let mut rows: HashMap<Entity, Vec<(Moment, Entity)>> = HashMap::default();
    for (note, mark, ..) in &marks {
        if mark.gone.is_none() {
            rows.entry(mark.struck).or_default().push((mark.lands_at, note));
        }
    }
    for row in rows.values_mut() {
        row.sort();
    }

    for (note, mut mark, mut node, mut background, mut border, children) in &mut marks {
        if let Some(gone) = mark.gone {
            if now.since(gone) >= GONE {
                commands.entity(note).despawn();
                continue;
            }
            background.0 = Color::NONE;
            *border = BorderColor::all(Color::NONE);
            for child in children.iter() {
                if let Ok((_, _, mut visibility)) = labels.get_mut(child) {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
            continue;
        }
        let Ok((queue, health, body)) = targets.get(mark.struck) else {
            commands.entity(note).despawn();
            continue;
        };

        match queue.threats.iter().find(|t| (t.source, t.inserted_at, t.lane()) == mark.key) {
            Some(threat) => {
                let (fill, rim, label) = highway::look(threat, health, false);
                background.0 = fill;
                *border = BorderColor::all(rim);
                mark.color = fill;
                for child in children.iter() {
                    if let Ok((mut text, _, _)) = labels.get_mut(child) {
                        if **text != label {
                            **text = label.clone();
                        }
                    }
                }
            }
            None if gone.landed(mark.struck, mark.key.0, mark.key.1) == Some(true) => {
                mark.gone = Some(now);
                highway::pulse(&mut commands, note, Vec2::splat(NOTE / 2.0), NOTE, highway::lane_color(mark.key.2), now);
                continue;
            }
            None if gone.landed(mark.struck, mark.key.0, mark.key.1) == Some(false)
                || now.since(*mark.unsure.get_or_insert(now)) >= highway::UNSURE => {
                mark.gone = Some(now);
                highway::shatter(&mut commands, note, Vec2::splat(NOTE / 2.0), NOTE, mark.color, now);
                continue;
            }
            None => {}
        }

        let arriving = 1.0 - (now.since(mark.born).as_secs_f32() / ARRIVAL).min(1.0);
        let size = NOTE * (1.0 + (ARRIVES_AT - 1.0) * arriving);
        let row = &rows[&mark.struck];
        let slot = row.iter().position(|&(_, e)| e == note).unwrap_or(0) as f32;
        let across = (slot - (row.len() - 1) as f32 / 2.0) * (NOTE + GAP);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        for child in children.iter() {
            if let Ok((_, mut transform, _)) = labels.get_mut(child) {
                transform.scale = Vec2::splat(size / (NOTE * ARRIVES_AT));
            }
        }
        match node_at(camera, camera_transform, &scale, body.translation + Vec3::Y * OVER) {
            Some(at) => {
                node.left = Val::Px(at.x + across - size / 2.0);
                node.top = Val::Px(at.y - size / 2.0);
            }
            None => node.left = Val::Px(OFF_SCREEN),
        }
    }
}

/// Flashes each struck body: every mesh under it wears its own copy of its
/// material, lit by the flash as it fades, and wears the shared one again
/// once it has.
pub fn flash(
    mut commands: Commands,
    bodies: Query<(Entity, &Struck)>,
    children: Query<&Children>,
    meshes: Query<(&MeshMaterial3d<CelMaterial>, Option<&Flashing>)>,
    mut materials: ResMut<Assets<CelMaterial>>,
    time: Res<Time>,
) {
    let now = Moment::ZERO + time.elapsed();
    for (body, &Struck(at)) in &bodies {
        let left = 1.0 - now.since(at).as_secs_f32() / FLASH;
        if left <= 0.0 {
            commands.entity(body).remove::<Struck>();
        }
        let light = FLASH_LIGHT * left.max(0.0).powi(2);
        for mesh in children.iter_descendants(body) {
            let Ok((worn, flashing)) = meshes.get(mesh) else { continue };
            match flashing {
                Some(Flashing(shared)) if left <= 0.0 => {
                    commands.entity(mesh).remove::<Flashing>().insert(MeshMaterial3d(shared.clone()));
                }
                Some(_) => {
                    if let Some(mut own) = materials.get_mut(&worn.0) {
                        own.base.emissive = LinearRgba::new(light, light, light, 0.0);
                    }
                }
                None if left > 0.0 => {
                    let Some(mut own) = materials.get(&worn.0).cloned() else { continue };
                    own.base.emissive = LinearRgba::new(light, light, light, 0.0);
                    let own = materials.add(own);
                    commands.entity(mesh).insert((Flashing(worn.0.clone()), MeshMaterial3d(own)));
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{components::ActorAttributes, message::AbilityType, systems::combat::queue as queue_utils, tuning::Tuning};

    fn strike(source: Entity, at: u64) -> common_bevy::components::reaction_queue::QueuedThreat {
        queue_utils::create_threat(
            &Tuning::DEFAULT,
            source, &ActorAttributes::default(), &ActorAttributes::default(),
            50.0, Some(AbilityType::Frenzy), Moment::from_millis(at * 1_000), 0.0, 0.0,
        )
    }

    fn notes(app: &mut App) -> Vec<(Entity, Moment, bool)> {
        let mut query = app.world_mut().query::<&Mark>();
        query.iter(app.world()).map(|mark| (mark.struck, mark.key.1, mark.gone.is_some())).collect()
    }

    #[test]
    fn the_viewed_actors_strike_stands_over_its_target_and_a_landing_flashes() {
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.init_resource::<EntityMap>();
        app.add_systems(Update, (on_contact, on_landing).chain());

        let player = app.world_mut().spawn((Viewed, Transform::default())).id();
        let foe = app.world_mut().spawn(Transform::default()).id();
        // A threat names its source as the server does
        let [player_there, foe_there, other_there] = [9_000, 9_001, 9_002].map(|id| Entity::from_raw_u32(id).unwrap());
        app.world_mut().resource_mut::<EntityMap>().insert(player, player_there);
        app.world_mut().resource_mut::<EntityMap>().insert(foe, foe_there);
        for (ent, threat) in [(foe, strike(player_there, 1)), (foe, strike(player_there, 2)), (foe, strike(other_there, 3)), (player, strike(foe_there, 4))] {
            app.world_mut().write_message(Do { event: GameEvent::InsertThreat { ent, threat } });
        }
        app.update();
        let mut standing = notes(&mut app);
        standing.sort();
        assert_eq!(standing, vec![(foe, Moment::from_millis(1_000), false), (foe, Moment::from_millis(2_000), false)], "a note for each of its own strikes, none for another's or on itself");
        assert!(app.world().get::<Struck>(foe).is_none(), "nothing flashes before it lands");

        app.world_mut().write_message(Do { event: GameEvent::ApplyDamage { ent: foe, damage: 50.0, source: player, dot: false } });
        app.update();
        let mut risen = app.world_mut().query::<&FloatingText>();
        assert_eq!(risen.iter(app.world()).count(), 1, "and its damage rises");
        assert!(app.world().get::<Struck>(foe).is_some(), "from the body it lands on, which flashes");
    }
}
