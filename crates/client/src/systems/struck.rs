//! What a skill shows as it resolves (`Event::Resolved`). A blow that
//! landed flashes the body it lands on, and its damage rises from that
//! body; one blocked or missed raises that word from its target in the
//! number's place, and a skill interrupted raises it from its user.

use bevy::prelude::*;
use common_bevy::message::{Do, Event as GameEvent, Outcome};
use common_bevy::moment::Moment;

use crate::components::FloatingText;
use crate::plugins::cel::CelMaterial;

/// How long a struck body's flash takes to fade
const FLASH: f32 = 0.2;
/// The light a flash adds at its peak to what the body's surface gives,
/// as drawn, unweighted by exposure
const FLASH_LIGHT: f32 = 0.6;

/// How far over a body's origin what rises from it starts
const RISES_FROM: f32 = 2.5;

/// The colour of damage over time, apart from every blow's, and of a word
/// that stands where a number would
const DOT_COLOR: Color = Color::srgb(0.65, 0.3, 0.95);
const WORD_COLOR: Color = Color::srgb(0.85, 0.85, 0.85);

/// When damage last landed on a body, while its flash lasts
#[derive(Component)]
pub struct Struck(Moment);

/// A mesh of a struck body, wearing its own copy of its material while it
/// flashes: the material it shares, to wear again after
#[derive(Component)]
pub struct Flashing(Handle<CelMaterial>);

/// What rises as a skill resolves `outcome` for `damage`, and from which
/// of the two it names, its user or its target: None where nothing shows.
/// A blow with no damage, a drag or a pull, shows by what it moves; a
/// skill its user cancelled shows nothing; a rune letting a foe go is no
/// miss.
fn rises(outcome: Outcome, damage: f32, of_rune: bool) -> Option<(String, bool)> {
    match outcome {
        Outcome::Landed if damage > 0.0 => Some((format!("{damage:.0}"), true)),
        Outcome::Blocked => Some(("Blocked".to_string(), true)),
        Outcome::Missed if !of_rune => Some(("Miss".to_string(), true)),
        Outcome::Interrupted => Some(("Interrupted".to_string(), false)),
        Outcome::Landed | Outcome::Missed | Outcome::Cancelled => None,
    }
}

/// As a skill resolves, the body its damage lands on flashes, and its
/// damage, or the word for how it ended, rises ([`rises`]).
pub fn on_resolved(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    bodies: Query<&Transform>,
    time: Res<Time>,
) {
    let now = Moment::ZERO + time.elapsed();
    for message in reader.read() {
        let GameEvent::Resolved { ent, ability, target, damage, outcome, dot } = message.event else { continue };
        let Some((text, on_target)) = rises(outcome, damage, ability.rune().is_some()) else { continue };
        // A strike at no one, or at one gone from here, misses over its user
        let from = if on_target { target.unwrap_or(ent) } else { ent };
        if outcome == Outcome::Landed {
            if let Ok(mut struck) = commands.get_entity(from) {
                struck.try_insert(Struck(now));
            }
        }
        // A body stays for its death pose, so the dead one struck has its Transform
        let Ok(body) = bodies.get(from) else { continue };
        let color = match outcome {
            Outcome::Landed if dot => DOT_COLOR,
            Outcome::Landed => Color::WHITE,
            _ => WORD_COLOR,
        };
        commands.spawn((
            Node { position_type: PositionType::Absolute, ..default() },
            Text::new(text),
            TextFont { font_size: FontSize::Px(32.0), ..default() },
            TextColor(color),
            TextLayout::justify(Justify::Center),
            FloatingText {
                spawn_time: now,
                world_position: body.translation + Vec3::Y * RISES_FROM,
                lifetime: 1.5,
                velocity: 1.0,
            },
        ));
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
    use common_bevy::message::AbilityType;

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_message::<Do>();
        app.init_resource::<Time>();
        app.add_systems(Update, on_resolved);
        let striker = app.world_mut().spawn(Transform::from_xyz(0.0, 0.0, 0.0)).id();
        let foe = app.world_mut().spawn(Transform::from_xyz(5.0, 0.0, 0.0)).id();
        (app, striker, foe)
    }

    /// What rose this frame: its text and the x it rose at
    fn risen(app: &mut App) -> Vec<(String, f32)> {
        let mut query = app.world_mut().query::<(&Text, &FloatingText)>();
        query.iter(app.world()).map(|(text, floating)| (text.0.clone(), floating.world_position.x)).collect()
    }

    fn resolved(ent: Entity, ability: AbilityType, target: Option<Entity>, damage: f32, outcome: Outcome) -> Do {
        Do { event: GameEvent::Resolved { ent, ability, target, damage, outcome, dot: false } }
    }

    #[test]
    fn a_blow_that_lands_flashes_the_one_struck_and_its_damage_rises_from_it() {
        let (mut app, striker, foe) = app();
        app.world_mut().write_message(resolved(striker, AbilityType::Frenzy, Some(foe), 50.0, Outcome::Landed));
        app.update();
        assert_eq!(risen(&mut app), vec![("50".to_string(), 5.0)]);
        assert!(app.world().get::<Struck>(foe).is_some(), "the one struck flashes");
        assert!(app.world().get::<Struck>(striker).is_none(), "never its striker");
    }

    #[test]
    fn a_blow_stopped_says_how_in_the_numbers_place_and_flashes_no_one() {
        let (mut app, striker, foe) = app();
        app.world_mut().write_message(resolved(striker, AbilityType::Frenzy, Some(foe), 0.0, Outcome::Blocked));
        app.world_mut().write_message(resolved(striker, AbilityType::Overpower, Some(foe), 0.0, Outcome::Missed));
        app.world_mut().write_message(resolved(striker, AbilityType::Overpower, Some(foe), 0.0, Outcome::Interrupted));
        app.update();
        let mut risen = risen(&mut app);
        risen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(risen.iter().map(|(_, x)| *x).collect::<Vec<_>>(), vec![5.0, 0.0, 5.0], "a block and a miss over the target, an interrupt over its user: {risen:?}");
        assert!(risen.iter().all(|(text, _)| text.parse::<f32>().is_err()), "each a word: {risen:?}");
        assert!(app.world().get::<Struck>(foe).is_none() && app.world().get::<Struck>(striker).is_none());
    }

    #[test]
    fn what_moves_its_target_or_was_let_go_raises_nothing() {
        let (mut app, striker, foe) = app();
        app.world_mut().write_message(resolved(striker, AbilityType::Yank, Some(foe), 0.0, Outcome::Landed));
        app.world_mut().write_message(resolved(striker, AbilityType::Drag, Some(foe), 0.0, Outcome::Missed));
        app.world_mut().write_message(resolved(striker, AbilityType::Overpower, Some(foe), 0.0, Outcome::Cancelled));
        app.update();
        assert!(risen(&mut app).is_empty());
        assert!(app.world().get::<Struck>(foe).is_none());
    }
}
