//! What a skill shows as it resolves (`Event::Resolved`). A blow that
//! landed flashes the body it lands on, and its damage rises from that
//! body; one blocked or missed raises that word from its target in the
//! number's place, a pull that lands raises its word from the one pulled,
//! and a skill interrupted or cancelled raises that from its user. The
//! colour and the word of each outcome are told here, for everything that
//! shows one.

use bevy::prelude::*;
use common_bevy::message::{AbilityType, Do, Event as GameEvent, Outcome};
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

/// The colour of damage over time, apart from every blow's
const DOT_COLOR: Color = Color::srgb(0.65, 0.3, 0.95);

/// The colour an outcome is shown in, wherever one is shown: the word that
/// rises and the activation bar's verdict. Never alone: each has its word
/// ([`outcome_word`]).
pub fn outcome_color(outcome: Outcome) -> Color {
    match outcome {
        Outcome::Landed => Color::WHITE,
        Outcome::Blocked => Color::srgb(0.45, 0.62, 0.85),
        Outcome::Missed => Color::srgb(0.85, 0.85, 0.85),
        Outcome::Interrupted => Color::srgb(1.0, 0.72, 0.15),
        Outcome::Cancelled => Color::srgb(0.55, 0.55, 0.55),
    }
}

/// The word for an outcome, wherever one is told
pub fn outcome_word(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Landed => "Landed",
        Outcome::Blocked => "Blocked",
        Outcome::Missed => "Miss",
        Outcome::Interrupted => "Interrupted",
        Outcome::Cancelled => "Cancelled",
    }
}

/// The colour of the word for why a press was refused, over the one who
/// pressed: no outcome's, since nothing began
pub const REFUSED: Color = Color::srgb(0.95, 0.45, 0.4);

/// The word over one a landing moved and did not hurt
const PULLED: &str = "Pulled";

/// Whether `ability` lands by pulling its target, with no damage of its own
fn pulls(ability: AbilityType) -> bool {
    matches!(ability, AbilityType::Yank | AbilityType::Drag)
}

/// Raises `text` in `color` from over `body`, at `now` on the client's
/// clock: the one way anything floats over an actor.
pub fn float(commands: &mut Commands, text: impl Into<String>, color: Color, body: &Transform, now: Moment) {
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

/// When damage last landed on a body, while its flash lasts
#[derive(Component)]
pub struct Struck(Moment);

/// A mesh of a struck body, wearing its own copy of its material while it
/// flashes: the material it shares, to wear again after
#[derive(Component)]
pub struct Flashing(Handle<CelMaterial>);

/// What rises as `ability` resolves `outcome` for `damage`, and from which
/// of the two it names, its user or its target: None where nothing does.
/// A landing rises as its damage, or as a word where it pulled and did not
/// hurt; a landing that does neither is one its user took on itself, told
/// by what it leaves on them. A rune letting a foe go is no miss: its rope
/// falls away.
fn rises(ability: AbilityType, outcome: Outcome, damage: f32) -> Option<(String, bool)> {
    match outcome {
        Outcome::Landed if damage > 0.0 => Some((format!("{damage:.0}"), true)),
        Outcome::Landed => pulls(ability).then(|| (PULLED.to_string(), true)),
        Outcome::Missed if ability.rune().is_some() => None,
        Outcome::Blocked | Outcome::Missed => Some((outcome_word(outcome).to_string(), true)),
        Outcome::Interrupted | Outcome::Cancelled => Some((outcome_word(outcome).to_string(), false)),
    }
}

/// As a skill resolves, the body its damage lands on flashes, and its
/// damage, or the word for how it ended, rises ([`rises`]) in its
/// outcome's colour.
pub fn on_resolved(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    bodies: Query<&Transform>,
    time: Res<Time>,
) {
    let now = Moment::ZERO + time.elapsed();
    for message in reader.read() {
        let GameEvent::Resolved { ent, ability, target, damage, outcome, dot } = message.event else { continue };
        let Some((text, on_target)) = rises(ability, outcome, damage) else { continue };
        // A strike at no one, or at one gone from here, misses over its user
        let from = if on_target { target.unwrap_or(ent) } else { ent };
        if outcome == Outcome::Landed && damage > 0.0 {
            if let Ok(mut struck) = commands.get_entity(from) {
                struck.try_insert(Struck(now));
            }
        }
        // A body stays for its death pose, so the dead one struck has its Transform
        let Ok(body) = bodies.get(from) else { continue };
        let color = if dot && outcome == Outcome::Landed { DOT_COLOR } else { outcome_color(outcome) };
        float(&mut commands, text, color, body, now);
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
    fn a_pull_that_lands_says_so_over_the_one_pulled_and_a_cancel_over_its_user() {
        let (mut app, striker, foe) = app();
        for pull in [AbilityType::Yank, AbilityType::Drag] {
            app.world_mut().write_message(resolved(striker, pull, Some(foe), 0.0, Outcome::Landed));
        }
        app.world_mut().write_message(resolved(striker, AbilityType::Overpower, Some(foe), 0.0, Outcome::Cancelled));
        app.update();
        let mut risen = risen(&mut app);
        risen.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(risen, vec![("Cancelled".to_string(), 0.0), ("Pulled".to_string(), 5.0), ("Pulled".to_string(), 5.0)]);
        assert!(app.world().get::<Struck>(foe).is_none(), "nothing hurt, nothing flashes");
    }

    #[test]
    fn a_rune_letting_a_foe_go_and_a_skill_taken_on_oneself_raise_nothing() {
        let (mut app, striker, foe) = app();
        app.world_mut().write_message(resolved(striker, AbilityType::Drag, Some(foe), 0.0, Outcome::Missed));
        app.world_mut().write_message(resolved(striker, AbilityType::Vigil, None, 0.0, Outcome::Landed));
        app.update();
        assert!(risen(&mut app).is_empty());
    }

    #[test]
    fn every_outcome_has_a_word_and_a_colour_of_its_own() {
        let all = [Outcome::Landed, Outcome::Blocked, Outcome::Missed, Outcome::Interrupted, Outcome::Cancelled];
        for (i, a) in all.iter().enumerate() {
            assert!(!outcome_word(*a).is_empty());
            for b in &all[i + 1..] {
                assert_ne!(outcome_word(*a), outcome_word(*b));
                assert_ne!(outcome_color(*a).to_srgba(), outcome_color(*b).to_srgba(), "{a:?} from {b:?}");
            }
        }
    }

    #[test]
    fn every_ending_but_a_rune_letting_go_and_a_skill_taken_on_oneself_raises_something() {
        for ability in AbilityType::ALL {
            for outcome in [Outcome::Blocked, Outcome::Interrupted, Outcome::Cancelled] {
                assert!(rises(ability, outcome, 0.0).is_some(), "{ability:?} {outcome:?}");
            }
            assert!(rises(ability, Outcome::Landed, 10.0).is_some());
            assert_eq!(rises(ability, Outcome::Missed, 0.0).is_none(), ability.rune().is_some(), "{ability:?} missing");
        }
    }
}
