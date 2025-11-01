use bevy::prelude::*;

use crate::{
    common::{
        components::{Actor, gcd::Gcd, resources::*, Loc, heading::Heading, entity_type::EntityType},
        message::AbilityType,
        plugins::nntree::NNTree,
        systems::targeting::select_target,
    },
};

/// Marker component for the action bar container
#[derive(Component)]
pub struct ActionBarDisplay;

/// Marker component for individual ability slot UI
#[derive(Component)]
pub struct AbilitySlot {
    pub slot_index: usize,
    pub ability: Option<AbilityType>,
    pub keybind: KeyCode,
}

/// Marker for ability slot background
#[derive(Component)]
pub struct SlotBackground;

/// Marker for ability slot icon
#[derive(Component)]
pub struct SlotIcon;

/// Marker for ability slot keybind label
#[derive(Component)]
pub struct SlotKeybind;

/// Marker for ability slot cost badge
#[derive(Component)]
pub struct SlotCost;

/// Marker for ability slot cooldown overlay
#[derive(Component)]
pub struct SlotCooldown;

/// Setup action bar UI below resource bars
/// Creates 4 ability slots (Q, W, E, R)
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, Added<Camera3d>>,
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
        ActionBarDisplay,
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
            // Define ability slots: Q, W, E, R
            let slots = vec![
                (0, KeyCode::KeyQ, Some(AbilityType::BasicAttack)),  // Q = BasicAttack
                (1, KeyCode::KeyW, None),                             // W = Empty
                (2, KeyCode::KeyE, Some(AbilityType::Dodge)),        // E = Dodge
                (3, KeyCode::KeyR, None),                             // R = Empty
            ];

            for (slot_index, keybind, ability) in slots {
                // Spawn ability slot
                parent.spawn((
        Node {
            width: Val::Px(80.),
            height: Val::Px(80.),
            border: UiRect::all(Val::Px(3.)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor(Color::srgb(0.3, 0.8, 0.3)),  // Default: Green (ready)
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        AbilitySlot { slot_index, ability, keybind },
    ))
    .with_children(|parent| {
        // Ability icon (center)
        let icon_text = match ability {
            Some(AbilityType::BasicAttack) => "⚔",
            Some(AbilityType::Dodge) => "🌀",
            None => "🔒",
        };

        parent.spawn((
            Text::new(icon_text),
            TextFont {
                font_size: 32.0,
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
        parent.spawn((
            Text::new(format!("{:?}", keybind).replace("Key", "")),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.8, 0.8, 0.8)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.),
                left: Val::Px(6.),
                ..default()
            },
            SlotKeybind,
        ));

        // Cost badge (bottom-right corner)
        if let Some(ability_type) = ability {
            let cost_text = match ability_type {
                AbilityType::BasicAttack => String::new(),  // Free
                AbilityType::Dodge => "60".to_string(),     // 60 stamina
            };

            if !cost_text.is_empty() {
                parent.spawn((
                    Text::new(cost_text),
                    TextFont {
                        font_size: 12.0,
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
    });  // Close .with_children from line 97 (slot children)
            }  // Close for loop from line 82
        });  // Close .with_children from line 73 (action bar children)
    });  // Close outer .with_children
}

/// Update action bar states based on player's resources and GCD
/// Updates border colors to indicate ability states (ready/cooldown/insufficient resources)
pub fn update(
    mut slot_query: Query<(&AbilitySlot, &mut BorderColor)>,
    player_query: Query<(Entity, &Stamina, &Mana, &Loc, &Heading, Option<&Gcd>), With<Actor>>,
    entity_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    time: Res<Time>,
) {
    // Get player resources and position
    let Ok((player_ent, stamina, mana, player_loc, player_heading, gcd_opt)) = player_query.get_single() else {
        return;  // No player yet
    };

    let now = time.elapsed();
    let gcd_active = gcd_opt.map_or(false, |gcd| gcd.is_active(now));

    for (slot, mut border_color) in &mut slot_query {
        if let Some(ability) = slot.ability {
            // Determine ability state
            let state = get_ability_state(
                ability,
                stamina,
                mana,
                gcd_active,
                player_ent,
                *player_loc,
                *player_heading,
                &nntree,
                &entity_query,
            );

            // Update border color based on state
            *border_color = match state {
                AbilityState::Ready => BorderColor(Color::srgb(0.3, 0.8, 0.3)),           // Green
                AbilityState::OnCooldown => BorderColor(Color::srgb(0.5, 0.5, 0.5)),      // Gray
                AbilityState::InsufficientResources => BorderColor(Color::srgb(0.9, 0.1, 0.1)), // Red
                AbilityState::OutOfRange => BorderColor(Color::srgb(0.8, 0.5, 0.1)),      // Orange
            };
        } else {
            // Empty slot: dark gray
            *border_color = BorderColor(Color::srgb(0.2, 0.2, 0.2));
        }
    }
}

/// Ability states for UI feedback
#[derive(Debug, PartialEq)]
enum AbilityState {
    Ready,
    OnCooldown,
    InsufficientResources,
    OutOfRange,
}

/// Determine ability state based on resources, GCD, and targeting
fn get_ability_state(
    ability: AbilityType,
    stamina: &Stamina,
    _mana: &Mana,
    gcd_active: bool,
    player_ent: Entity,
    player_loc: Loc,
    player_heading: Heading,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc)>,
) -> AbilityState {
    // Check GCD first
    if gcd_active {
        return AbilityState::OnCooldown;
    }

    // Check resource costs and range requirements
    match ability {
        AbilityType::BasicAttack => {
            // Free ability, but requires adjacent target
            // Check if there's a valid target within range (distance 1)
            let target_opt = select_target(
                player_ent,
                player_loc,
                player_heading,
                None,
                nntree,
                |ent| entity_query.get(ent).ok().map(|(et, _)| *et),
            );

            if let Some(target_ent) = target_opt {
                // Check if target is within melee range (adjacent = distance 1)
                if let Ok((_, target_loc)) = entity_query.get(target_ent) {
                    let distance = player_loc.flat_distance(target_loc) as u32;
                    if distance > 1 {
                        return AbilityState::OutOfRange;
                    }
                }
                AbilityState::Ready
            } else {
                // No valid target in facing cone
                AbilityState::OutOfRange
            }
        }
        AbilityType::Dodge => {
            // Costs 60 stamina
            if stamina.step >= 60.0 {
                AbilityState::Ready
            } else {
                AbilityState::InsufficientResources
            }
        }
    }
}
