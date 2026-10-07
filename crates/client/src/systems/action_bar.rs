use bevy::prelude::*;

use common_bevy::{
    components::{Actor, behaviour::Side, recovery::GlobalRecovery, Loc, heading::Heading, entity_type::EntityType},
    message::AbilityType,
    plugins::nntree::NNTree,
    systems::targeting::select_target,
};
use common_bevy::tuning::Tuning;

/// Marker component for the action bar container
#[derive(Component)]
pub struct ActionBarDisplay;

/// Marker component for individual ability slot UI
#[derive(Component)]
pub struct AbilitySlot {
    pub ability: Option<AbilityType>,
}

/// The row the viewed actor's ability slots stand in
#[derive(Component)]
pub struct AbilitySlots;

/// The slot under the compass that dismisses the front threat: no
/// ability's, and never refused, so it is lit while there is a threat to
/// take.
#[derive(Component)]
pub struct DismissSlot;

/// Marker for ability slot icon
#[derive(Component)]
pub struct SlotIcon;

/// Marker for ability slot keybind label
#[derive(Component)]
pub struct SlotKeybind;

/// Marker for the glow on a slot whose ability is offered as a combo
#[derive(Component)]
pub struct ComboGlow;

/// Marker for cooldown overlay (dark rect that shrinks as recovery depletes)
#[derive(Component)]
pub struct CooldownOverlay;

/// Side of an ability slot and its border, in px; the compass beside the
/// bar takes the same.
const SLOT_PX: f32 = 80.;
const SLOT_BORDER_PX: f32 = 3.;

/// Setup action bar UI, hung below the resource bars' line
/// Creates the ability slots, four to a row, and beside them the compass
/// over the dismiss slot
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
    asset_server: Res<AssetServer>,
) {
    let icons: Handle<Font> = asset_server.load(ICON_FONT);
    let camera = query.single().expect("query did not return exactly one result");

    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.),
            height: Val::Vw(crate::systems::resource_bars::LINE_VW),
            bottom: Val::Px(0.),
            left: Val::Px(0.),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            padding: UiRect::top(Val::Px(10.)),
            ..default()
        },
        Pickable::IGNORE,
        ActionBarDisplay,
        crate::components::ViewHud,
    ))
    .with_children(|parent| {
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(10.),
                ..default()
            },
        ))
        .with_children(|parent| {
            // The viewed actor's loadout, filled in by `sync_loadout`
            parent.spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(4, SLOT_PX),
                    column_gap: Val::Px(10.),
                    row_gap: Val::Px(10.),
                    ..default()
                },
                AbilitySlots,
            ));

            parent
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(10.), ..default() })
                .with_children(|column| {
                    crate::systems::ui::spawn_compass(column, SLOT_PX, SLOT_BORDER_PX);
                    slot_frame(column, &icons, crate::systems::input::KEYCODE_DISMISS, DISMISS_ICON).insert(DismissSlot);
                });
        });  // Close .with_children from line 73 (action bar children)
    });  // Close outer .with_children
}

/// The bar's keys, row by row, and the ability a player holds on each: the
/// four strikes on the top row; the reactions, then the moves, below. Every
/// skill of the kit has its key.
pub const KEYS: [KeyCode; 8] = [
    KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR,
    KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::KeyF,
];
pub const PLAYER: [AbilityType; 8] = [
    AbilityType::Frenzy, AbilityType::Feint, AbilityType::Overpower, AbilityType::Punish,
    AbilityType::Parry, AbilityType::Counter, AbilityType::Leap, AbilityType::PerfectStride,
];

/// What `ability` does, as its key's help says.
pub fn tells(ability: AbilityType) -> &'static str {
    match ability {
        AbilityType::AutoAttack => "Auto-attack: strikes the hostile target within reach on its own",
        AbilityType::Frenzy => "Frenzy: a bite on the target in reach; its combo is another bite",
        AbilityType::Feint => "Feint: a light strike on the target in reach",
        AbilityType::Overpower => "Overpower: one heavy blow on the target in reach",
        AbilityType::Punish => "Punish: a strike on the target in reach, harder on one still in recovery",
        AbilityType::Parry => "Parry: clear the span, sending nothing back",
        AbilityType::Counter => "Counter: clear the span, and send a share of each threat back to its source at any range",
        AbilityType::Leap => "Leap: clear of the target in reach, or onto the one out of it",
        AbilityType::PerfectStride => "Perfect Stride: for a few seconds, strike past the forward faces freely and run faster",
    }
}

/// What stands on each of the bar's keys for an actor of `typ`: a player's
/// whole kit; each of an NPC's skills on the key a player holds it on, so a
/// view's bar looks as a player's does.
pub fn loadout(typ: &EntityType) -> [Option<AbilityType>; 8] {
    use common_bevy::components::entity_type::actor::ActorIdentity;
    match typ {
        EntityType::Actor(actor) => match actor.identity {
            ActorIdentity::Player => PLAYER.map(Some),
            ActorIdentity::Npc(npc) => {
                let skills = npc.bar();
                PLAYER.map(|ability| skills.contains(&ability).then_some(ability))
            }
        },
        _ => [None; 8],
    }
}

/// Fills the bar with the loadout of the actor the client sees as, anew
/// whenever that actor changes.
pub fn sync_loadout(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    seer: Query<(Entity, &EntityType), With<crate::components::Viewed>>,
    slots: Query<Entity, With<AbilitySlots>>,
    mut shown: Local<Option<Entity>>,
) {
    let Ok(container) = slots.single() else { return };
    let seer = seer.single().ok();
    if seer.map(|(ent, _)| ent) == *shown {
        return;
    }
    *shown = seer.map(|(ent, _)| ent);
    commands.entity(container).despawn_related::<Children>();
    let Some((_, typ)) = seer else { return };
    let icons: Handle<Font> = asset_server.load(ICON_FONT);
    commands.entity(container).with_children(|parent| {
        for (keybind, ability) in KEYS.into_iter().zip(loadout(typ)) {
            spawn_slot(&icons, parent, keybind, ability);
        }
    });
}

/// The font a slot's icon is drawn in: a Nerd Font, whose private-use
/// glyphs stand for the skills until they have icons of their own.
const ICON_FONT: &str = "fonts/IosevkaNerdFont-Regular.ttf";

/// The dismiss slot's icon, a glyph in `ICON_FONT`: md-skip_next.
const DISMISS_ICON: &str = "\u{F04AD}";

const READY: Color = Color::srgb(0.3, 0.8, 0.3);
const EMPTY: Color = Color::srgb(0.2, 0.2, 0.2);

/// A slot under `parent`: its border and fill, `icon` at its centre in
/// `ICON_FONT`, and the cap of the key that works it in its corner.
/// Returns the slot, for what else it holds.
fn slot_frame<'a>(parent: &'a mut ChildSpawnerCommands, icons: &Handle<Font>, keybind: KeyCode, icon: &str) -> EntityCommands<'a> {
    let mut slot = parent.spawn((
        Node {
            width: Val::Px(SLOT_PX),
            height: Val::Px(SLOT_PX),
            border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(READY),
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
    ));
    slot.with_children(|parent| {
        parent.spawn((
            Text::new(icon),
            TextFont {
                font: icons.clone().into(),
                font_size: FontSize::Px(36.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Relative,
                ..default()
            },
            SlotIcon,
        ));
        crate::systems::keycap::corner_keycap(parent, keybind).insert(SlotKeybind);
    });
    slot
}

fn spawn_slot(icons: &Handle<Font>, parent: &mut ChildSpawnerCommands, keybind: KeyCode, ability: Option<AbilityType>) {
    // Ability icon, named for its glyph in `ICON_FONT`
    let icon = match ability {
        None => "",
        Some(AbilityType::AutoAttack) => "\u{F04E5}",     // md-sword
        Some(AbilityType::Frenzy) => "\u{EEB5}",          // fa-teeth_open
        Some(AbilityType::Feint) => "\u{F0D02}",          // md-drama_masks
        Some(AbilityType::Overpower) => "\u{F08EA}",      // md-hammer
        Some(AbilityType::Punish) => "\u{F09FC}",         // md-knife_military
        Some(AbilityType::Parry) => "\u{F0498}",          // md-shield
        Some(AbilityType::Counter) => "\u{F045A}",        // md-reply
        Some(AbilityType::Leap) => "\u{F0907}",           // md-rabbit
        Some(AbilityType::PerfectStride) => "\u{F046E}",  // md-run_fast
    };
    slot_frame(parent, icons, keybind, icon).insert(AbilitySlot { ability }).with_children(|parent| {
        // Cooldown overlay: dark rect anchored at bottom, height = recovery %
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(0.),
                bottom: Val::Px(0.),
                left: Val::Px(0.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            CooldownOverlay,
        ));

        // Combo glow overlay (bright gold glow while the slot's ability is offered)
        // Positioned absolutely to cover the entire slot, hidden by default
        // INTENTIONALLY VERY BRIGHT for testing - will tone down once confirmed working
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                top: Val::Px(0.),
                left: Val::Px(0.),
                border: UiRect::all(Val::Px(8.)),  // THICK border
                ..default()
            },
            BorderColor::all(Color::srgb(1.0, 1.0, 0.0)),  // BRIGHT YELLOW (impossible to miss)
            BackgroundColor(Color::srgba(1.0, 1.0, 0.0, 0.5)),  // BRIGHT semi-transparent yellow fill
            Visibility::Hidden,  // Hidden by default
            ComboGlow,
        ));
    });
}

/// Update action bar states based on player's recovery, combo and range
/// Updates border colors and the combo glow's visibility
pub fn update(
    tuning: Res<Tuning>,
    mut slot_query: Query<(&AbilitySlot, &mut BorderColor, &Children)>,
    mut glow_query: Query<&mut Visibility, With<ComboGlow>>,
    mut overlay_query: Query<&mut Node, With<CooldownOverlay>>,
    player_query: Query<(Entity, &Loc, &Heading, Option<&GlobalRecovery>, Option<&common_bevy::components::ActorAttributes>, Option<&common_bevy::components::AttackRange>, Has<Actor>), With<crate::components::Viewed>>,
    entity_query: Query<(&EntityType, &Loc, Option<&Side>)>,
    nntree: Res<NNTree>,
) {
    // The position of the actor the client sees as
    let Ok((player_ent, player_loc, player_heading, recovery_opt, attrs, own_reach, controlled)) = player_query.single() else {
        return;
    };

    // The recovery, the combo it offers, and what the gate would let fire
    // early in its chain (`combos::timing`)
    let recovery_active = recovery_opt.map_or(false, |r| r.is_active());
    let own_attrs = attrs.copied().unwrap_or_default();
    let early_reaction = |ability: AbilityType| matches!(
        common_bevy::systems::combat::combos::timing(ability, ability.is_reaction(), recovery_opt, &own_attrs),
        Some(common_bevy::systems::combat::combos::Timing::Early(common_bevy::systems::combat::combos::Early::Preparation))
    );
    let offered = recovery_opt.and_then(|r| r.combo);
    let recovery_remaining = recovery_opt.map(|r| r.remaining).unwrap_or(0.0);
    let recovery_duration = recovery_opt.map(|r| r.duration).unwrap_or(1.0);

    for (slot, mut border_color, children) in &mut slot_query {
        // An empty slot: dark, no glow, no overlay
        let Some(ability) = slot.ability else {
            *border_color = BorderColor::all(EMPTY);
            for child in children.iter() {
                if let Ok(mut visibility) = glow_query.get_mut(child) {
                    *visibility = Visibility::Hidden;
                }
                if let Ok(mut node) = overlay_query.get_mut(child) {
                    node.height = Val::Percent(0.0);
                }
            }
            continue;
        };
        // Range is judged for the client's own character, who picks its
        // targets here; a viewed actor's targets are the server's
        let state = if controlled {
            get_ability_state(
                ability,
                recovery_active,
                offered.is_some_and(|combo| combo.ability == ability),
                early_reaction(ability),
                player_ent,
                *player_loc,
                *player_heading,
                common_bevy::systems::targeting::arc_of(&tuning, attrs),
                own_reach.copied().unwrap_or_default().0,
                &nntree,
                &entity_query,
            )
        } else if recovery_active {
            recovery_state(offered.is_some_and(|combo| combo.ability == ability), early_reaction(ability))
        } else {
            AbilityState::Ready
        };

        // Update border color based on state (keep meaningful colors)
        let (border, show_combo_glow) = match state {
            AbilityState::Ready => (BorderColor::all(Color::srgb(0.3, 0.8, 0.3)), false),           // Green
            AbilityState::OnCooldown => (BorderColor::all(Color::srgb(0.5, 0.5, 0.5)), false),      // Gray
            AbilityState::ComboUnlocked => {
                (BorderColor::all(Color::srgb(0.3, 0.8, 0.3)), true)  // Green + BRIGHT YELLOW GLOW!
            },
            AbilityState::EarlyReaction => (BorderColor::all(Color::srgb(0.2, 0.8, 0.9)), false),   // Cyan
            AbilityState::OutOfRange => (BorderColor::all(Color::srgb(0.8, 0.5, 0.1)), false),      // Orange
        };
        *border_color = border;

        // Cooldown overlay: height = proportion of recovery remaining
        let overlay_pct = if !recovery_active || recovery_duration <= 0.0 || matches!(state, AbilityState::EarlyReaction) {
            0.0
        } else {
            let combo_unlock_at = offered
                .filter(|combo| combo.ability == ability)
                .map(|combo| combo.unlock_at);
            let ratio = match combo_unlock_at {
                Some(unlock_at) if recovery_duration > unlock_at => {
                    (recovery_remaining - unlock_at) / (recovery_duration - unlock_at)
                }
                _ => recovery_remaining / recovery_duration,
            };
            ratio.clamp(0.0, 1.0) * 100.0
        };

        // Update combo glow and cooldown overlay
        for child in children.iter() {
            if let Ok(mut visibility) = glow_query.get_mut(child) {
                *visibility = if show_combo_glow {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
            if let Ok(mut node) = overlay_query.get_mut(child) {
                node.height = Val::Percent(overlay_pct);
            }
        }
    }
}

/// Lights the dismiss slot while the actor the client sees as has a
/// threat queued to take.
pub fn update_dismiss(
    mut slot: Query<&mut BorderColor, With<DismissSlot>>,
    viewed: Query<&common_bevy::components::reaction_queue::ReactionQueue, With<crate::components::Viewed>>,
) {
    let Ok(mut border) = slot.single_mut() else { return };
    let lit = viewed.single().is_ok_and(|queue| !queue.is_empty());
    border.set_if_neq(BorderColor::all(if lit { READY } else { EMPTY }));
}

/// Ability states for UI feedback
#[derive(Debug, PartialEq)]
enum AbilityState {
    Ready,
    OnCooldown,
    ComboUnlocked,  // The ability the recovery offers as its combo (gold glow)
    EarlyReaction,  // A reaction Preparation fires early in the chain (cyan)
    OutOfRange,
}

/// A slot's state in recovery: the offered combo glowing from the moment
/// it is offered, a reaction Preparation fires early usable, and all else
/// waiting
fn recovery_state(offered: bool, early_reaction: bool) -> AbilityState {
    if offered {
        AbilityState::ComboUnlocked
    } else if early_reaction {
        AbilityState::EarlyReaction
    } else {
        AbilityState::OnCooldown
    }
}

/// Determine ability state based on recovery, combo, and targeting
fn get_ability_state(
    ability: AbilityType,
    recovery_active: bool,
    offered: bool,
    early_reaction: bool,
    player_ent: Entity,
    player_loc: Loc,
    player_heading: Heading,
    arc: f32,
    own_reach: i32,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc, Option<&Side>)>,
) -> AbilityState {
    // In recovery the offered combo glows from the start, a reaction
    // Preparation fires early shows it may, and all else waits
    if recovery_active {
        return recovery_state(offered, early_reaction);
    }

    // The actors the player may target: those on a side hostile to its own
    let side_of = |ent: Entity| entity_query.get(ent).ok().and_then(|(_, _, side)| side.copied());
    let own_side = side_of(player_ent);
    let hostile = |ent: Entity| side_of(ent).zip(own_side).is_some_and(|(side, own)| side.is_hostile_to(own));

    // Only a strike is held to a reach: it needs the hostile the player
    // faces within it
    let Some(reach) = ability.reach(own_reach) else {
        return AbilityState::Ready;
    };
    let target = select_target(player_ent, player_loc, player_heading, arc, nntree, hostile);
    match target.and_then(|target| entity_query.get(target).ok()) {
        Some((_, target_loc, _)) if reach.contains(&player_loc.flat_distance(target_loc)) => AbilityState::Ready,
        _ => AbilityState::OutOfRange,
    }
}

#[cfg(test)]
mod loadout_tests {
    use super::*;
    use common_bevy::{components::entity_type::actor::*, archetype::EnemyArchetype};

    fn npc(npc: EnemyArchetype) -> EntityType {
        EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Npc(npc)))
    }

    #[test]
    fn a_player_fills_every_key() {
        let player = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player));
        assert_eq!(loadout(&player), PLAYER.map(Some));
    }

    #[test]
    fn an_npc_skill_stands_on_a_players_key_for_it() {
        for archetype in EnemyArchetype::ALL {
            let bar = loadout(&npc(archetype));
            for skill in archetype.bar() {
                let key = PLAYER.iter().position(|&a| a == skill).expect("every skill has a player's key");
                assert_eq!(bar[key], Some(skill), "{archetype:?}");
            }
            assert_eq!(bar.iter().flatten().count(), archetype.bar().len(), "{archetype:?} shows only its own");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_recovery_the_combo_glows_an_early_reaction_shows_usable_and_the_rest_waits() {
        assert!(matches!(recovery_state(true, false), AbilityState::ComboUnlocked));
        assert!(matches!(recovery_state(false, true), AbilityState::EarlyReaction));
        assert!(matches!(recovery_state(false, false), AbilityState::OnCooldown));
    }
}
