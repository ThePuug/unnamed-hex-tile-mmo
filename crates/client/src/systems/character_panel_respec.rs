use bevy::prelude::*;

use crate::systems::character_panel::*;
use common_bevy::{
    components::{Actor, ActorAttributes},
    message::{Do, Event as GameEvent, Try},
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
                AxisAdjustButton::MightAgilityIncrease => {
                    let new_axis = draft.might_agility_axis - 1; // Increase commitment = more negative
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.might_agility_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_agility_axis = new_axis;
                }
                AxisAdjustButton::MightAgilityDecrease => {
                    let new_axis = draft.might_agility_axis + 1; // Decrease commitment = less negative
                    if new_axis.abs() > draft.might_agility_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_agility_axis = new_axis;
                }
                AxisAdjustButton::VitalityDisciplineIncrease => {
                    let new_axis = draft.vitality_discipline_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.vitality_discipline_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_discipline_axis = new_axis;
                }
                AxisAdjustButton::VitalityDisciplineDecrease => {
                    let new_axis = draft.vitality_discipline_axis + 1;
                    if new_axis.abs() > draft.vitality_discipline_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_discipline_axis = new_axis;
                }
                AxisAdjustButton::InstinctResolveIncrease => {
                    let new_axis = draft.instinct_resolve_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.instinct_resolve_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_resolve_axis = new_axis;
                }
                AxisAdjustButton::InstinctResolveDecrease => {
                    let new_axis = draft.instinct_resolve_axis + 1;
                    if new_axis.abs() > draft.instinct_resolve_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_resolve_axis = new_axis;
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
                AxisAdjustButtonRight::MightAgilityDecrease => {
                    let new_axis = draft.might_agility_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.might_agility_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_agility_axis = new_axis;
                }
                AxisAdjustButtonRight::MightAgilityIncrease => {
                    let new_axis = draft.might_agility_axis + 1;
                    if new_axis.abs() > draft.might_agility_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.might_agility_axis = new_axis;
                }
                AxisAdjustButtonRight::VitalityDisciplineDecrease => {
                    let new_axis = draft.vitality_discipline_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.vitality_discipline_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_discipline_axis = new_axis;
                }
                AxisAdjustButtonRight::VitalityDisciplineIncrease => {
                    let new_axis = draft.vitality_discipline_axis + 1;
                    if new_axis.abs() > draft.vitality_discipline_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.vitality_discipline_axis = new_axis;
                }
                AxisAdjustButtonRight::InstinctResolveDecrease => {
                    let new_axis = draft.instinct_resolve_axis - 1;
                    if new_axis < -127 { continue; }
                    if new_axis.abs() > draft.instinct_resolve_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_resolve_axis = new_axis;
                }
                AxisAdjustButtonRight::InstinctResolveIncrease => {
                    let new_axis = draft.instinct_resolve_axis + 1;
                    if new_axis.abs() > draft.instinct_resolve_axis.abs()
                        && draft.total_investment() >= max_level
                    {
                        continue;
                    }
                    draft.instinct_resolve_axis = new_axis;
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
                SpectrumAdjustButton::MightAgilityDecrease => {
                    draft.might_agility_spectrum = (draft.might_agility_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::MightAgilityIncrease => {
                    if draft.total_investment() >= max_level {
                        continue; // No budget
                    }
                    if draft.might_agility_spectrum == i8::MAX {
                        continue; // Prevent i8 overflow
                    }
                    draft.might_agility_spectrum = draft.might_agility_spectrum + 1;
                }
                SpectrumAdjustButton::VitalityDisciplineDecrease => {
                    draft.vitality_discipline_spectrum = (draft.vitality_discipline_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::VitalityDisciplineIncrease => {
                    if draft.total_investment() >= max_level {
                        continue;
                    }
                    if draft.vitality_discipline_spectrum == i8::MAX {
                        continue;
                    }
                    draft.vitality_discipline_spectrum = draft.vitality_discipline_spectrum + 1;
                }
                SpectrumAdjustButton::InstinctResolveDecrease => {
                    draft.instinct_resolve_spectrum = (draft.instinct_resolve_spectrum - 1).max(0);
                }
                SpectrumAdjustButton::InstinctResolveIncrease => {
                    if draft.total_investment() >= max_level {
                        continue;
                    }
                    if draft.instinct_resolve_spectrum == i8::MAX {
                        continue;
                    }
                    draft.instinct_resolve_spectrum = draft.instinct_resolve_spectrum + 1;
                }
            }
        }
    }

    // Auto-clamp shifts after any axis/spectrum change
    if let Some(draft) = state.pending_respec.as_mut() {
        draft.might_agility_shift = clamp_shift(draft.might_agility_shift, draft.might_agility_axis, draft.might_agility_spectrum);
        draft.vitality_discipline_shift = clamp_shift(draft.vitality_discipline_shift, draft.vitality_discipline_axis, draft.vitality_discipline_spectrum);
        draft.instinct_resolve_shift = clamp_shift(draft.instinct_resolve_shift, draft.instinct_resolve_axis, draft.instinct_resolve_spectrum);
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

            // Send respec request (keep pending state until server confirms)
            writer.write(Try { event: GameEvent::RespecAttributes { ent, pairs: draft.pairs() } });
        }
    }
}

/// Handle Do event - apply confirmed respec
pub fn handle_respec_confirmed(
    mut state: ResMut<CharacterPanelState>,
    mut reader: MessageReader<Do>,
    mut player_query: Query<&mut ActorAttributes, With<Actor>>,
) {
    for message in reader.read() {
        if let GameEvent::RespecAttributes { ent, pairs } = &message.event {
            // Apply to player's ActorAttributes
            if let Ok(mut attrs) = player_query.get_mut(*ent) {
                attrs.apply_respec(*pairs);

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
