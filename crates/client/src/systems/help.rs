//! How the client reads a key: every press a player makes is read through
//! [`Keys`], with what it does. While [`KEYCODE_HELP`] is held a press acts
//! on nothing; what each reader would have done with it shows beside the
//! cap that names the key, or at one shared spot where none does. The
//! admin console reads its keys as it always has, and has no help.

use std::collections::HashSet;

use bevy::{diagnostic::FrameCount, ecs::system::SystemParam, prelude::*, window::PrimaryWindow};

use crate::systems::keycap::Names;

/// The key held to ask what the others do.
pub const KEYCODE_HELP: KeyCode = KeyCode::AltRight;

/// Room between a tooltip and the cap it stands over, and the screen's edge.
const GAP_PX: f32 = 6.0;
/// The shared spot's height over the screen's bottom, for keys no cap names:
/// over the resource bars' line and the action bar's.
const SHARED_VW: f32 = 2.0 * crate::systems::resource_bars::LINE_VW + 2.0;
const TIP_WIDTH_PX: f32 = 320.0;
const TIP_FONT: f32 = 14.0;
const TIP_TEXT: Color = Color::srgb(0.95, 0.95, 0.95);
const TIP_FILL: Color = Color::srgba(0.06, 0.06, 0.06, 0.95);
const TIP_EDGE: Color = Color::srgb(0.5, 0.5, 0.5);

/// What was asked of the keys while help is held.
#[derive(Resource, Default)]
pub struct Help {
    /// The key last asked about, the frame it was, and what each reader
    /// would have done with it there
    asked: Option<(KeyCode, u32, Vec<&'static str>)>,
    /// Keys pressed while asking: they act on nothing until released
    spent: HashSet<KeyCode>,
}

/// The keyboard as a player works it. Each read names what the key does:
/// while help is held, a press made then does nothing and says so instead.
#[derive(SystemParam)]
pub struct Keys<'w> {
    input: ResMut<'w, ButtonInput<KeyCode>>,
    help: ResMut<'w, Help>,
    frame: Res<'w, FrameCount>,
}

impl Keys<'_> {
    /// Whether `key` went down this frame, to do what `does` says.
    pub fn pressed(&mut self, key: KeyCode, does: &'static str) -> bool {
        self.read(key, does, false)
    }

    /// [`Self::pressed`], and the press is taken: no reader after this one
    /// sees it.
    pub fn take(&mut self, key: KeyCode, does: &'static str) -> bool {
        self.read(key, does, true)
    }

    /// Whether `key` is held, doing what `does` says while it is. A key
    /// held since before help was asked keeps acting; one pressed while it
    /// was does nothing until it is pressed again.
    pub fn held(&mut self, key: KeyCode, does: &'static str) -> bool {
        if self.input.just_pressed(key) && self.asking() {
            self.ask(key, does);
        }
        !self.help.spent.contains(&key) && self.input.pressed(key)
    }

    fn read(&mut self, key: KeyCode, does: &'static str, take: bool) -> bool {
        if !self.input.just_pressed(key) {
            return false;
        }
        if take {
            self.input.clear_just_pressed(key);
        }
        if self.asking() {
            self.ask(key, does);
            return false;
        }
        true
    }

    fn asking(&self) -> bool {
        self.input.pressed(KEYCODE_HELP)
    }

    /// Notes what `key` does, beside what other readers said of it this
    /// frame.
    fn ask(&mut self, key: KeyCode, does: &'static str) {
        let frame = self.frame.0;
        self.help.spent.insert(key);
        match &mut self.help.asked {
            Some((asked, at, said)) if *asked == key && *at == frame => {
                if !said.contains(&does) {
                    said.push(does);
                }
            }
            asked => *asked = Some((key, frame, vec![does])),
        }
    }
}

/// The tooltip help shows.
#[derive(Component)]
pub struct Tooltip;

#[derive(Component)]
pub struct TooltipText;

pub fn setup(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                max_width: Val::Px(TIP_WIDTH_PX),
                padding: UiRect::axes(Val::Px(8.), Val::Px(5.)),
                border: UiRect::all(Val::Px(1.)),
                border_radius: BorderRadius::all(Val::Px(3.)),
                ..default()
            },
            BackgroundColor(TIP_FILL),
            BorderColor::all(TIP_EDGE),
            GlobalZIndex(i32::MAX),
            Visibility::Hidden,
            Pickable::IGNORE,
            Tooltip,
        ))
        .with_children(|tip| {
            tip.spawn((Text::new(""), TextFont { font_size: FontSize::Px(TIP_FONT), ..default() }, TextColor(TIP_TEXT), TooltipText));
        });
}

/// Shows what the key last asked about does while help is held, over the
/// first cap on screen that names it, else at the shared spot; and frees a
/// spent key once it is let go.
pub fn show(
    mut help: ResMut<Help>,
    input: Res<ButtonInput<KeyCode>>,
    caps: Query<(&Names, &ComputedNode, &UiGlobalTransform, &InheritedVisibility)>,
    mut tip: Query<(&mut Node, &mut Visibility, &ComputedNode), With<Tooltip>>,
    mut text: Query<&mut Text, With<TooltipText>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    help.spent.retain(|key| input.pressed(*key));
    if !input.pressed(KEYCODE_HELP) {
        help.asked = None;
    }
    let (Ok((mut node, mut visibility, size)), Ok(mut text)) = (tip.single_mut(), text.single_mut()) else { return };
    let Some((key, _, said)) = &help.asked else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let Ok(window) = window.single() else { return };
    let screen = Vec2::new(window.width(), window.height());
    let wide = size.size().x * size.inverse_scale_factor();
    let now = said.join("\n");
    if text.0 != now {
        text.0 = now;
    }
    let cap = caps
        .iter()
        .find(|(names, cap, _, shown)| shown.get() && cap.size().x > 0.0 && names.0.contains(key));
    let (left, bottom) = match cap {
        Some((_, cap, transform, _)) => {
            let scale = cap.inverse_scale_factor();
            let centre = transform.affine().translation * scale;
            let top = centre.y - cap.size().y * scale / 2.0;
            (centre.x - wide / 2.0, screen.y - top + GAP_PX)
        }
        None => ((screen.x - wide) / 2.0, screen.x * SHARED_VW / 100.0),
    };
    node.left = Val::Px(left.clamp(GAP_PX, (screen.x - wide - GAP_PX).max(GAP_PX)));
    node.bottom = Val::Px(bottom);
    visibility.set_if_neq(Visibility::Visible);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<Help>().init_resource::<FrameCount>();
        app
    }

    /// A frame in which `keys` go down, the rest held as they were.
    fn press(app: &mut App, keys: &[KeyCode]) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.clear();
        for key in keys {
            input.press(*key);
        }
    }

    /// Whether G, read as a press, and A, read as a held key, act.
    fn read(app: &mut App) -> (bool, bool) {
        app.world_mut()
            .run_system_once(|mut keys: Keys| (keys.pressed(KeyCode::KeyG, "Gather"), keys.held(KeyCode::KeyA, "Move")))
            .unwrap()
    }

    #[test]
    fn a_press_while_help_is_held_does_nothing_and_says_what_it_would_do() {
        let mut app = app();
        press(&mut app, &[KEYCODE_HELP, KeyCode::KeyG]);
        assert_eq!(read(&mut app), (false, false));
        let asked = app.world().resource::<Help>().asked.clone().map(|(key, _, said)| (key, said));
        assert_eq!(asked, Some((KeyCode::KeyG, vec!["Gather"])));
    }

    #[test]
    fn a_key_held_from_before_keeps_acting() {
        let mut app = app();
        press(&mut app, &[KeyCode::KeyA]);
        assert_eq!(read(&mut app), (false, true));
        press(&mut app, &[KEYCODE_HELP]);
        assert_eq!(read(&mut app), (false, true), "help held after it, it still acts");
    }

    #[test]
    fn a_key_pressed_while_asking_waits_for_its_next_press() {
        let mut app = app();
        press(&mut app, &[KEYCODE_HELP, KeyCode::KeyA]);
        assert_eq!(read(&mut app), (false, false));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KEYCODE_HELP);
        press(&mut app, &[]);
        assert_eq!(read(&mut app), (false, false), "help let go, the key still held does nothing");
    }
}
