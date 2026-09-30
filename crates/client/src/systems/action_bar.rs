use bevy::prelude::*;

use common_bevy::{
    components::{Actor, behaviour::Side, gcd::Gcd, recovery::{GlobalRecovery, SynergyUnlock}, resources::*, tier_lock::TierLock, Loc, heading::Heading, entity_type::EntityType},
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
    pub ability: AbilityType,
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

/// Marker for ability slot synergy glow overlay
#[derive(Component)]
pub struct SynergyGlow;

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

/// The abilities on the bar of an actor of `typ`, each with the key that
/// fires it: a player's four, an NPC's signature.
pub fn loadout(typ: &EntityType) -> Vec<(Option<KeyCode>, AbilityType)> {
    use common_bevy::components::entity_type::actor::ActorIdentity;
    match typ {
        EntityType::Actor(actor) => match actor.identity {
            ActorIdentity::Player => vec![
                (Some(KeyCode::KeyQ), AbilityType::Lunge),
                (Some(KeyCode::KeyW), AbilityType::Overpower),
                (Some(KeyCode::KeyE), AbilityType::Counter),
                (Some(KeyCode::KeyR), AbilityType::Kick),
            ],
            ActorIdentity::Npc(npc) => common_bevy::spatial_difficulty::EnemyArchetype::of_npc(npc).ability()
                .map(|ability| (None, ability))
                .into_iter()
                .collect(),
        },
        _ => Vec::new(),
    }
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
        for (keybind, ability) in loadout(typ) {
            spawn_slot(parent, keybind, ability);
        }
    });
}

fn spawn_slot(parent: &mut ChildSpawnerCommands, keybind: Option<KeyCode>, ability: AbilityType) {
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
            AbilityType::Lunge => "⚡",       // Gap closer / dash
            AbilityType::Overpower => "💥",  // Heavy strike
            AbilityType::Deflect => "🛡",    // Shield / defense
            AbilityType::AutoAttack => "⚔",  // Auto-attack
            AbilityType::Rattle => "💫",      // Juggernaut rattle
            AbilityType::Disengage => "💨",   // Skirmisher leap away
            AbilityType::Volley => "🏹",      // Kiter volley
            AbilityType::Flank => "🗡",       // Ambusher flank
            AbilityType::Counter => "↩",     // Counter / reflect
            AbilityType::Kick => "🦶",       // Kick / knockback
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

        // Keybind label (top-left corner); an NPC's signature has no key
        if let Some(keybind) = keybind {
            crate::systems::keycap::corner_keycap(parent, &format!("{:?}", keybind).replace("Key", "")).insert(SlotKeybind);
        }

        // Cost badge (bottom-right corner)
        {
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

        // Cooldown overlay: dark rect anchored at bottom, height = lockout %
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

        // Synergy glow overlay (BRIGHT gold glow when synergy unlocked)
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
            SynergyGlow,
        ));
    });
}

/// Update action bar states based on player's resources, recovery, and synergies
/// Updates border colors AND synergy glow visibility
pub fn update(
    mut slot_query: Query<(&AbilitySlot, &mut BorderColor, &Children)>,
    mut glow_query: Query<&mut Visibility, With<SynergyGlow>>,
    mut overlay_query: Query<&mut Node, With<CooldownOverlay>>,
    player_query: Query<(Entity, &Stamina, &Mana, &Loc, &Heading, Option<&TierLock>, Option<&Gcd>, Option<&GlobalRecovery>, Option<&SynergyUnlock>, Has<Actor>), With<crate::components::Viewed>>,
    entity_query: Query<(&EntityType, &Loc, Option<&Side>)>,
    nntree: Res<NNTree>,
    time: Res<Time>,
) {
    // The resources and position of the actor the client sees as
    let Ok((player_ent, stamina, mana, player_loc, player_heading, targeting_state, gcd_opt, recovery_opt, synergy_opt, controlled)) = player_query.single() else {
        return;
    };
    let targeting_state = targeting_state.copied().unwrap_or_default();

    let now = time.elapsed();
    let gcd_active = gcd_opt.map_or(false, |gcd| gcd.is_active(now));

    // Check recovery lockout and synergy state
    let recovery_active = recovery_opt.map_or(false, |r| r.is_active());
    let recovery_remaining = recovery_opt.map(|r| r.remaining).unwrap_or(0.0);
    let recovery_duration = recovery_opt.map(|r| r.duration).unwrap_or(1.0);

    for (slot, mut border_color, children) in &mut slot_query {
        let ability = slot.ability;
        // Range is judged for the client's own character, who picks its
        // targets here; a viewed actor's targets are the server's
        let state = if controlled {
            get_ability_state(
                ability,
                stamina,
                mana,
                gcd_active,
                recovery_active,
                recovery_remaining,
                synergy_opt,
                player_ent,
                *player_loc,
                *player_heading,
                &targeting_state,
                &nntree,
                &entity_query,
            )
        } else if recovery_active {
            if synergy_opt.is_some_and(|s| s.ability == ability) { AbilityState::SynergyUnlocked } else { AbilityState::OnCooldown }
        } else if stamina.step < common_bevy::tuning::tuning().cost(ability) {
            AbilityState::InsufficientResources
        } else {
            AbilityState::Ready
        };

        // Update border color based on state (keep meaningful colors)
        let (border, show_synergy_glow) = match state {
            AbilityState::Ready => (BorderColor::all(Color::srgb(0.3, 0.8, 0.3)), false),           // Green
            AbilityState::OnCooldown => (BorderColor::all(Color::srgb(0.5, 0.5, 0.5)), false),      // Gray
            AbilityState::SynergyUnlocked => {
                (BorderColor::all(Color::srgb(0.3, 0.8, 0.3)), true)  // Green + BRIGHT YELLOW GLOW!
            },
            AbilityState::InsufficientResources => (BorderColor::all(Color::srgb(0.9, 0.1, 0.1)), false), // Red
            AbilityState::OutOfRange => (BorderColor::all(Color::srgb(0.8, 0.5, 0.1)), false),      // Orange
        };
        *border_color = border;

        // Cooldown overlay: height = proportion of lockout remaining
        let overlay_pct = if !recovery_active || recovery_duration <= 0.0 {
            0.0
        } else {
            let synergy_unlock_at = synergy_opt
                .filter(|s| s.ability == ability)
                .map(|s| s.unlock_at);
            let ratio = match synergy_unlock_at {
                Some(unlock_at) if recovery_duration > unlock_at => {
                    (recovery_remaining - unlock_at) / (recovery_duration - unlock_at)
                }
                _ => recovery_remaining / recovery_duration,
            };
            ratio.clamp(0.0, 1.0) * 100.0
        };

        // Update synergy glow and cooldown overlay
        for child in children.iter() {
            if let Ok(mut visibility) = glow_query.get_mut(child) {
                *visibility = if show_synergy_glow {
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
    SynergyUnlocked,  // Ability unlocked early via synergy (gold glow)
    InsufficientResources,
    OutOfRange,
}

/// Determine ability state based on resources, recovery, synergies, and targeting
fn get_ability_state(
    ability: AbilityType,
    stamina: &Stamina,
    _mana: &Mana,
    gcd_active: bool,
    recovery_active: bool,
    _recovery_remaining: f32,
    synergy_opt: Option<&SynergyUnlock>,
    player_ent: Entity,
    player_loc: Loc,
    player_heading: Heading,
    targeting_state: &TierLock,
    nntree: &NNTree,
    entity_query: &Query<(&EntityType, &Loc, Option<&Side>)>,
) -> AbilityState {
    // Check recovery lockout (Universal lockout, can be synergy-unlocked)
    if recovery_active {
        // Check if this ability has a synergy active (show glow immediately)
        if let Some(synergy) = synergy_opt {
            if synergy.ability == ability {
                // Synergy active for this ability! Show gold glow immediately
                return AbilityState::SynergyUnlocked;
            }
        }
        // Still locked (no synergy)
        return AbilityState::OnCooldown;
    }

    if gcd_active {
        return AbilityState::OnCooldown;
    }

    // Check resource costs and range requirements
    match ability {
        AbilityType::Lunge => {
            if stamina.step < common_bevy::tuning::tuning().cost(AbilityType::Lunge) {
                return AbilityState::InsufficientResources;
            }

            let target_opt = select_target(
                player_ent,
                player_loc,
                player_heading,
                targeting_state.get(), // Respect tier lock
                nntree,
                |ent| entity_query.get(ent).ok().map(|(et, _, _)| *et),
                |ent| entity_query.get(ent).ok().and_then(|(_, _, side)| side.copied()),
            );

            if let Some(target_ent) = target_opt {
                if let Ok((_, target_loc, _)) = entity_query.get(target_ent) {
                    let distance = player_loc.flat_distance(target_loc) as u32;
                    if distance > common_bevy::systems::combat::resources::LUNGE_RANGE {
                        return AbilityState::OutOfRange;
                    }
                }
                AbilityState::Ready
            } else {
                AbilityState::OutOfRange
            }
        }
        AbilityType::Overpower => {
            if stamina.step < common_bevy::tuning::tuning().cost(AbilityType::Overpower) {
                return AbilityState::InsufficientResources;
            }

            let target_opt = select_target(
                player_ent,
                player_loc,
                player_heading,
                targeting_state.get(), // Respect tier lock
                nntree,
                |ent| entity_query.get(ent).ok().map(|(et, _, _)| *et),
                |ent| entity_query.get(ent).ok().and_then(|(_, _, side)| side.copied()),
            );

            if let Some(target_ent) = target_opt {
                if let Ok((_, target_loc, _)) = entity_query.get(target_ent) {
                    let distance = player_loc.flat_distance(target_loc) as u32;
                    if distance > 1 {
                        return AbilityState::OutOfRange;
                    }
                }
                AbilityState::Ready
            } else {
                AbilityState::OutOfRange
            }
        }
        AbilityType::Deflect => {
            // Clear all threats: 50 stamina, no target required
            if stamina.step >= common_bevy::tuning::tuning().cost(AbilityType::Deflect) {
                AbilityState::Ready
            } else {
                AbilityState::InsufficientResources
            }
        }
        AbilityType::AutoAttack | AbilityType::Rattle | AbilityType::Disengage | AbilityType::Volley | AbilityType::Flank => {
            // Passive or NPC-only - not on the player's action bar
            AbilityState::Ready
        }
        AbilityType::Counter => {
            // Counter: self-target, no range check
            if stamina.step >= common_bevy::tuning::tuning().cost(AbilityType::Counter) {
                AbilityState::Ready
            } else {
                AbilityState::InsufficientResources
            }
        }
        AbilityType::Kick => {
            // Kick: self-target, no range check
            if stamina.step >= common_bevy::tuning::tuning().cost(AbilityType::Kick) {
                AbilityState::Ready
            } else {
                AbilityState::InsufficientResources
            }
        }
    }
}
