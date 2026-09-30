use bevy::prelude::*;

use common_bevy::{
    components::{Actor, behaviour::Side, recovery::GlobalRecovery, resources::*, Loc, heading::Heading, entity_type::EntityType},
    message::AbilityType,
    plugins::nntree::NNTree,
    systems::targeting::select_target,
};

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

/// Marker for ability slot icon
#[derive(Component)]
pub struct SlotIcon;

/// Marker for ability slot keybind label
#[derive(Component)]
pub struct SlotKeybind;

/// Marker for ability slot cost badge
#[derive(Component)]
pub struct SlotCost;

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

/// Setup action bar UI below resource bars
/// Creates 4 ability slots (Q, W, E, R), and the compass beside them
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
            height: Val::Percent(100.),
            bottom: Val::Px(0.),
            left: Val::Px(0.),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            padding: UiRect::bottom(Val::Percent(6.0)),  // Above resource bars
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
                column_gap: Val::Px(10.),
                ..default()
            },
        ))
        .with_children(|parent| {
            // The viewed actor's loadout, filled in by `sync_loadout`
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.),
                    ..default()
                },
                AbilitySlots,
            ));

            crate::systems::ui::spawn_compass(parent, SLOT_PX, SLOT_BORDER_PX);
        });  // Close .with_children from line 73 (action bar children)
    });  // Close outer .with_children
}

/// The bar's keys, left to right, and the ability a player holds on each.
const KEYS: [KeyCode; 4] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR];
const PLAYER: [AbilityType; 4] = [AbilityType::Lunge, AbilityType::Overpower, AbilityType::Counter, AbilityType::Kick];

/// What stands on each of the bar's keys for an actor of `typ`: a player's
/// four; an NPC's signature on the key a player holds it on, else the
/// first, the rest empty, so a view's bar looks as a player's does.
pub fn loadout(typ: &EntityType) -> [Option<AbilityType>; 4] {
    use common_bevy::components::entity_type::actor::ActorIdentity;
    let mut bar = [None; 4];
    match typ {
        EntityType::Actor(actor) => match actor.identity {
            ActorIdentity::Player => bar = PLAYER.map(Some),
            ActorIdentity::Npc(npc) => {
                let signature = npc.profile().ability;
                bar[PLAYER.iter().position(|&a| a == signature).unwrap_or(0)] = Some(signature);
            }
        },
        _ => {}
    }
    bar
}

/// Fills the bar with the loadout of the actor the client sees as, anew
/// whenever that actor changes.
pub fn sync_loadout(
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
    commands.entity(container).with_children(|parent| {
        for (keybind, ability) in KEYS.into_iter().zip(loadout(typ)) {
            spawn_slot(parent, keybind, ability);
        }
    });
}

fn spawn_slot(parent: &mut ChildSpawnerCommands, keybind: KeyCode, ability: Option<AbilityType>) {
    parent.spawn((
        Node {
            width: Val::Px(SLOT_PX),
            height: Val::Px(SLOT_PX),
            border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(Color::srgb(0.3, 0.8, 0.3)),  // Default: Green (ready)
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        AbilitySlot { ability },
    ))
    .with_children(|parent| {
        // Ability icon (center)
        let icon_text = match ability {
            None => "",
            Some(AbilityType::Lunge) => "⚡",       // Gap closer / dash
            Some(AbilityType::Overpower) => "💥",  // Heavy strike
            Some(AbilityType::AutoAttack) => "⚔",  // Auto-attack
            Some(AbilityType::Rattle) => "💫",      // Juggernaut rattle
            Some(AbilityType::Disengage) => "💨",   // Skirmisher leap away
            Some(AbilityType::Volley) => "🏹",      // Kiter volley
            Some(AbilityType::Flank) => "🗡",       // Ambusher flank
            Some(AbilityType::Counter) => "↩",     // Counter / reflect
            Some(AbilityType::Kick) => "🦶",       // Kick / knockback
        };

        parent.spawn((
            Text::new(icon_text),
            TextFont {
                font_size: FontSize::Px(32.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Relative,
                ..default()
            },
            SlotIcon,
        ));

        // Keybind label (top-left corner)
        crate::systems::keycap::corner_keycap(parent, &format!("{:?}", keybind).replace("Key", "")).insert(SlotKeybind);

        // Cost badge (bottom-right corner)
        if let Some(ability) = ability {
            let cost_text = match ability {
                AbilityType::AutoAttack => String::new(),     // Free (passive)
                AbilityType::Rattle | AbilityType::Disengage | AbilityType::Volley | AbilityType::Flank => String::new(), // NPC-only
                _ => format!("{:.0}", common_bevy::tuning::tuning().cost(ability)),
            };

            if !cost_text.is_empty() {
                parent.spawn((
                    Text::new(cost_text),
                    TextFont {
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.8, 0.0)),  // Yellow for stamina cost
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(4.),
                        right: Val::Px(6.),
                        ..default()
                    },
                    SlotCost,
                ));
            }
        }

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

/// Update action bar states based on player's resources, recovery, and combo
/// Updates border colors and the combo glow's visibility
pub fn update(
    mut slot_query: Query<(&AbilitySlot, &mut BorderColor, &Children)>,
    mut glow_query: Query<&mut Visibility, With<ComboGlow>>,
    mut overlay_query: Query<&mut Node, With<CooldownOverlay>>,
    player_query: Query<(Entity, &Stamina, &Mana, &Loc, &Heading, Option<&GlobalRecovery>, Option<&common_bevy::components::ActorAttributes>, Option<&common_bevy::components::AttackRange>, Has<Actor>), With<crate::components::Viewed>>,
    entity_query: Query<(&EntityType, &Loc, Option<&Side>)>,
    nntree: Res<NNTree>,
) {
    // The resources and position of the actor the client sees as
    let Ok((player_ent, stamina, mana, player_loc, player_heading, recovery_opt, attrs, own_reach, controlled)) = player_query.single() else {
        return;
    };

    // The recovery, and the combo it offers
    let recovery_active = recovery_opt.map_or(false, |r| r.is_active());
    let offered = recovery_opt.and_then(|r| r.combo);
    let recovery_remaining = recovery_opt.map(|r| r.remaining).unwrap_or(0.0);
    let recovery_duration = recovery_opt.map(|r| r.duration).unwrap_or(1.0);

    for (slot, mut border_color, children) in &mut slot_query {
        // An empty slot: dark, no glow, no overlay
        let Some(ability) = slot.ability else {
            *border_color = BorderColor::all(Color::srgb(0.2, 0.2, 0.2));
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
                stamina,
                mana,
                recovery_active,
                offered.is_some_and(|combo| combo.ability == ability),
                player_ent,
                *player_loc,
                *player_heading,
                common_bevy::systems::targeting::arc_of(attrs),
                own_reach.copied().unwrap_or_default().0,
                &nntree,
                &entity_query,
            )
        } else if recovery_active {
            if offered.is_some_and(|combo| combo.ability == ability) { AbilityState::ComboUnlocked } else { AbilityState::OnCooldown }
        } else if stamina.state < common_bevy::tuning::tuning().cost(ability) {
            AbilityState::InsufficientResources
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
            AbilityState::InsufficientResources => (BorderColor::all(Color::srgb(0.9, 0.1, 0.1)), false), // Red
            AbilityState::OutOfRange => (BorderColor::all(Color::srgb(0.8, 0.5, 0.1)), false),      // Orange
        };
        *border_color = border;

        // Cooldown overlay: height = proportion of recovery remaining
        let overlay_pct = if !recovery_active || recovery_duration <= 0.0 {
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

/// Ability states for UI feedback
#[derive(Debug, PartialEq)]
enum AbilityState {
    Ready,
    OnCooldown,
    ComboUnlocked,  // The ability the recovery offers as its combo (gold glow)
    InsufficientResources,
    OutOfRange,
}

/// Determine ability state based on resources, recovery, combo, and targeting
fn get_ability_state(
    ability: AbilityType,
    stamina: &Stamina,
    _mana: &Mana,
    recovery_active: bool,
    offered: bool,
    player_ent: Entity,
    player_loc: Loc,
    player_heading: Heading,
    arc: f32,
    own_reach: i32,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc, Option<&Side>)>,
) -> AbilityState {
    // In recovery the offered combo glows from the start, and all else waits
    if recovery_active {
        return if offered { AbilityState::ComboUnlocked } else { AbilityState::OnCooldown };
    }

    // The actors the player may target: those on a side hostile to its own
    let side_of = |ent: Entity| entity_query.get(ent).ok().and_then(|(_, _, side)| side.copied());
    let own_side = side_of(player_ent);
    let hostile = |ent: Entity| side_of(ent).zip(own_side).is_some_and(|(side, own)| side.is_hostile_to(own));

    if stamina.state < common_bevy::tuning::tuning().cost(ability) {
        return AbilityState::InsufficientResources;
    }

    // A reaction takes no target; a strike needs the one the player faces
    // within the ability's own reach
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
    use common_bevy::{components::entity_type::actor::*, spatial_difficulty::EnemyArchetype};

    fn npc(npc: EnemyArchetype) -> EntityType {
        EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Npc(npc)))
    }

    #[test]
    fn a_player_fills_every_key() {
        let player = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player));
        assert_eq!(loadout(&player), PLAYER.map(Some));
    }

    #[test]
    fn an_npc_signature_stands_on_a_players_key_for_it() {
        assert_eq!(loadout(&npc(EnemyArchetype::Defender)), [None, None, Some(AbilityType::Counter), None]);
        assert_eq!(loadout(&npc(EnemyArchetype::Berserker)), [Some(AbilityType::Lunge), None, None, None]);
        assert_eq!(loadout(&npc(EnemyArchetype::Juggernaut)), [Some(AbilityType::Rattle), None, None, None], "one a player lacks goes first");
    }
}
