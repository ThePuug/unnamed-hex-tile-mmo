use bevy::prelude::*;

use crate::{
    client::systems::character_panel::*,
    common::{
        components::{Actor, ActorAttributes},
        message::{Do, Event as GameEvent, Try},
    },
};

/// Handle axis and spectrum button clicks
pub fn handle_attribute_buttons(
    mut state: ResMut<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    axis_buttons_left: Query<(&AxisAdjustButton, &Interaction), (Changed<Interaction>, Without<AxisAdjustButtonRight>)>,
    axis_buttons_right: Query<(&AxisAdjustButtonRight, &Interaction), Changed<Interaction>>,
    spectrum_buttons: Query<(&SpectrumAdjustButton, &Interaction), Changed<Interaction>>,
) {
    let Ok(attrs) = player_query.single() else {
        return;
    };

    let max_level = attrs.total_level();

    // Handle left-side axis buttons
    for (button, interaction) in &axis_buttons_left {
        if *interaction == Interaction::Pressed {
            state.mark_dirty(attrs);
            let draft = state.pending_respec.as_mut().unwrap();

            match button {
                // LEFT SIDE: Increase = more negative, Decrease = less negative
                AxisAdjustButton::MightGraceIncrease => {
                    let new_axis = draft.might_grace_axis - 1; // Increase commitment = more negative
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.might_grace_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_grace_axis = new_axis;
                }
                AxisAdjustButton::MightGraceDecrease => {
                    let new_axis = draft.might_grace_axis + 1; // Decrease commitment = less negative
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.might_grace_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_grace_axis = new_axis;
                }
                AxisAdjustButton::VitalityFocusIncrease => {
                    let new_axis = draft.vitality_focus_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.vitality_focus_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_focus_axis = new_axis;
                }
                AxisAdjustButton::VitalityFocusDecrease => {
                    let new_axis = draft.vitality_focus_axis + 1;
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.vitality_focus_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_focus_axis = new_axis;
                }
                AxisAdjustButton::InstinctPresenceIncrease => {
                    let new_axis = draft.instinct_presence_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.instinct_presence_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_presence_axis = new_axis;
                }
                AxisAdjustButton::InstinctPresenceDecrease => {
                    let new_axis = draft.instinct_presence_axis + 1;
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.instinct_presence_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_presence_axis = new_axis;
                }
            }
        }
    }

    // Handle right-side axis buttons (same logic, different components)
    for (button, interaction) in &axis_buttons_right {
        if *interaction == Interaction::Pressed {
            state.mark_dirty(attrs);
            let draft = state.pending_respec.as_mut().unwrap();

            match button {
                AxisAdjustButtonRight::MightGraceDecrease => {
                    let new_axis = draft.might_grace_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.might_grace_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_grace_axis = new_axis;
                }
                AxisAdjustButtonRight::MightGraceIncrease => {
                    let new_axis = draft.might_grace_axis + 1;
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.might_grace_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_grace_axis = new_axis;
                }
                AxisAdjustButtonRight::VitalityFocusDecrease => {
                    let new_axis = draft.vitality_focus_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.vitality_focus_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_focus_axis = new_axis;
                }
                AxisAdjustButtonRight::VitalityFocusIncrease => {
                    let new_axis = draft.vitality_focus_axis + 1;
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.vitality_focus_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_focus_axis = new_axis;
                }
                AxisAdjustButtonRight::InstinctPresenceDecrease => {
                    let new_axis = draft.instinct_presence_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.instinct_presence_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_presence_axis = new_axis;
                }
                AxisAdjustButtonRight::InstinctPresenceIncrease => {
                    let new_axis = draft.instinct_presence_axis + 1;
                    if new_axis > 127 { continue; }
                    if new_axis.abs() > draft.instinct_presence_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_presence_axis = new_axis;
                }
            }
        }
    }

    // Handle spectrum buttons
    for (button, interaction) in &spectrum_buttons {
        if *interaction == Interaction::Pressed {
            state.mark_dirty(attrs);
            let draft = state.pending_respec.as_mut().unwrap();

            match button {
                SpectrumAdjustButton::MightGraceDecrease => {
                    draft.might_grace_spectrum = (draft.might_grace_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::MightGraceIncrease => {
                    if draft.total_investment() >= max_level {
                        continue; // No budget
                    }
                    if draft.might_grace_spectrum >= 127 {
                        continue; // Prevent i8 overflow
                    }
                    draft.might_grace_spectrum = draft.might_grace_spectrum + 1;
                }
                SpectrumAdjustButton::VitalityFocusDecrease => {
                    draft.vitality_focus_spectrum = (draft.vitality_focus_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::VitalityFocusIncrease => {
                    if draft.total_investment() >= max_level {
                        continue;
                    }
                    if draft.vitality_focus_spectrum >= 127 {
                        continue;
                    }
                    draft.vitality_focus_spectrum = draft.vitality_focus_spectrum + 1;
                }
                SpectrumAdjustButton::InstinctPresenceDecrease => {
                    draft.instinct_presence_spectrum = (draft.instinct_presence_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::InstinctPresenceIncrease => {
                    if draft.total_investment() >= max_level {
                        continue;
                    }
                    if draft.instinct_presence_spectrum >= 127 {
                        continue;
                    }
                    draft.instinct_presence_spectrum = draft.instinct_presence_spectrum + 1;
                }
            }
        }
    }
}

/// Update button colors based on cost/refund
pub fn update_button_colors(
    state: Res<CharacterPanelState>,
    player_query: Query<&ActorAttributes, With<Actor>>,
    mut axis_buttons: Query<(&AxisAdjustButton, &mut BackgroundColor)>,
    mut spectrum_buttons: Query<
        (&SpectrumAdjustButton, &mut BackgroundColor),
        Without<AxisAdjustButton>,
    >,
) {
    let Ok(attrs) = player_query.single() else {
        return;
    };

    let default_draft = DraftAttributes::from_current(attrs);
    let draft = state
        .pending_respec
        .as_ref()
        .unwrap_or(&default_draft);
    let budget_available = draft.total_investment() < attrs.total_level();

    // Color axis buttons
    for (button, mut color) in &mut axis_buttons {
        let (current_axis, would_cost) = match button {
            AxisAdjustButton::MightGraceDecrease => {
                let current = draft.might_grace_axis;
                (current, (current - 1).abs() > current.abs())
            }
            AxisAdjustButton::MightGraceIncrease => {
                let current = draft.might_grace_axis;
                (current, (current + 1).abs() > current.abs())
            }
            AxisAdjustButton::VitalityFocusDecrease => {
                let current = draft.vitality_focus_axis;
                (current, (current - 1).abs() > current.abs())
            }
            AxisAdjustButton::VitalityFocusIncrease => {
                let current = draft.vitality_focus_axis;
                (current, (current + 1).abs() > current.abs())
            }
            AxisAdjustButton::InstinctPresenceDecrease => {
                let current = draft.instinct_presence_axis;
                (current, (current - 1).abs() > current.abs())
            }
            AxisAdjustButton::InstinctPresenceIncrease => {
                let current = draft.instinct_presence_axis;
                (current, (current + 1).abs() > current.abs())
            }
        };

        *color = if would_cost {
            // Moving away from center - costs point
            if budget_available {
                BackgroundColor(Color::srgb(0.8, 0.8, 0.4)) // Yellow (cost)
            } else {
                BackgroundColor(Color::srgb(0.6, 0.3, 0.3)) // Red (no budget)
            }
        } else {
            // Moving toward center - refunds point
            if current_axis.abs() > 0 {
                BackgroundColor(Color::srgb(0.4, 0.8, 0.4)) // Green (refund)
            } else {
                BackgroundColor(Color::srgb(0.4, 0.4, 0.4)) // Gray (at center)
            }
        };
    }

    // Color spectrum buttons
    for (button, mut color) in &mut spectrum_buttons {
        *color = match button {
            SpectrumAdjustButton::MightGraceDecrease
            | SpectrumAdjustButton::VitalityFocusDecrease
            | SpectrumAdjustButton::InstinctPresenceDecrease => {
                let has_spectrum = match button {
                    SpectrumAdjustButton::MightGraceDecrease => draft.might_grace_spectrum > 0,
                    SpectrumAdjustButton::VitalityFocusDecrease => draft.vitality_focus_spectrum > 0,
                    SpectrumAdjustButton::InstinctPresenceDecrease => {
                        draft.instinct_presence_spectrum > 0
                    }
                    _ => false,
                };

                if has_spectrum {
                    BackgroundColor(Color::srgb(0.4, 0.8, 0.4)) // Green (refund)
                } else {
                    BackgroundColor(Color::srgb(0.4, 0.4, 0.4)) // Gray (disabled)
                }
            }
            SpectrumAdjustButton::MightGraceIncrease
            | SpectrumAdjustButton::VitalityFocusIncrease
            | SpectrumAdjustButton::InstinctPresenceIncrease => {
                let at_max = match button {
                    SpectrumAdjustButton::MightGraceIncrease => draft.might_grace_spectrum >= 10,
                    SpectrumAdjustButton::VitalityFocusIncrease => draft.vitality_focus_spectrum >= 10,
                    SpectrumAdjustButton::InstinctPresenceIncrease => {
                        draft.instinct_presence_spectrum >= 10
                    }
                    _ => false,
                };

                if at_max || !budget_available {
                    BackgroundColor(Color::srgb(0.6, 0.3, 0.3)) // Red (no budget/max)
                } else {
                    BackgroundColor(Color::srgb(0.8, 0.8, 0.4)) // Yellow (cost)
                }
            }
        };
    }
}

/// Handle Apply button click - sends Try event
pub fn handle_apply_button(
    state: Res<CharacterPanelState>,
    apply_query: Query<&Interaction, (With<ApplyRespecButton>, Changed<Interaction>)>,
    player_query: Query<(Entity, &ActorAttributes), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    for interaction in &apply_query {
        if *interaction == Interaction::Pressed {
            let Some(draft) = &state.pending_respec else {
                continue;
            };
            let Ok((ent, attrs)) = player_query.single() else {
                continue;
            };

            // Validate budget is fully allocated
            if draft.total_investment() != attrs.total_level() {
                continue; // Not all points allocated - don't apply
            }

            // Validate ranges
            if !draft.is_valid(attrs.total_level()) {
                continue;
            }

            // Send respec request (keep pending state until server confirms)
            writer.write(Try {
                event: GameEvent::RespecAttributes {
                    ent,
                    might_grace_axis: draft.might_grace_axis,
                    might_grace_spectrum: draft.might_grace_spectrum,
                    vitality_focus_axis: draft.vitality_focus_axis,
                    vitality_focus_spectrum: draft.vitality_focus_spectrum,
                    instinct_presence_axis: draft.instinct_presence_axis,
                    instinct_presence_spectrum: draft.instinct_presence_spectrum,
                },
            });
        }
    }
}

/// Handle Do event - apply confirmed respec
pub fn handle_respec_confirmed(
    mut state: ResMut<CharacterPanelState>,
    mut reader: MessageReader<Do>,
    mut player_query: Query<&mut ActorAttributes, With<Actor>>,
) {
    for &Do { event } in reader.read() {
        if let GameEvent::RespecAttributes {
            ent,
            might_grace_axis,
            might_grace_spectrum,
            vitality_focus_axis,
            vitality_focus_spectrum,
            instinct_presence_axis,
            instinct_presence_spectrum,
        } = event
        {
            // Apply to player's ActorAttributes
            if let Ok(mut attrs) = player_query.get_mut(ent) {
                attrs.apply_respec(
                    might_grace_axis,
                    might_grace_spectrum,
                    vitality_focus_axis,
                    vitality_focus_spectrum,
                    instinct_presence_axis,
                    instinct_presence_spectrum,
                );

                // Clear pending state now that server confirmed
                state.pending_respec = None;
            }
        }
    }
}

/// Show/hide Apply button based on pending changes
pub fn toggle_apply_button(
    state: Res<CharacterPanelState>,
    mut button_query: Query<&mut Visibility, With<ApplyRespecButton>>,
) {
    let Ok(mut vis) = button_query.single_mut() else {
        return;
    };

    *vis = if state.has_pending_changes() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
