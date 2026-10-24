pub mod action_bar;
pub mod actor;
pub mod actor_dead_visibility;
pub mod animator;
pub mod bag_panel;
pub mod camera;
pub mod character_panel;
pub mod character_panel_respec;
pub mod skill_text;
pub mod skills_panel;
pub mod closeup;
pub mod combat;
pub mod combat_log; // Combat log panel for event history
pub mod combat_ui;
pub mod compass;
pub mod den;
pub mod drop_panel;
pub mod equipment;
pub mod equipment_panel;
pub mod focus;
pub mod gathering;
pub mod ghost;
pub mod help;
pub mod highway;
pub mod hiding;
pub mod input;
pub mod keycap;
pub mod loot_window;
pub mod mount;
pub mod movement;
pub mod renet;
pub mod resolved_threats; // Resolved threats stack below threat queue
pub mod resource_bars;
pub mod struck;
pub mod target_frame;
pub mod target_indicator;
pub mod targeting;
pub mod threat_icons;
pub mod ui;
pub mod world;

/// `from` eased toward `to` by the decay constant `k` over `dt`: the one
/// easing every smoothed value uses. Frame-rate independent, and it never
/// passes `to`, however long a frame is.
pub fn ease(from: f32, to: f32, k: f32, dt: f32) -> f32 {
    from + (to - from) * (1.0 - (-k * dt).exp())
}

/// The decay constant `k`, set to follow a held turn's heading steps at
/// whole pace, scaled to the steps of a turn at `turn_pace`
/// (`Status::turn_pace`): what follows is as far through each step when the
/// next comes, so a slower turn is as smooth. At no pace, `k` itself.
pub fn following_turn(k: f32, turn_pace: f32) -> f32 {
    use common_bevy::systems::movement::{turn_interval_ms, TURN_REPEAT_MS};
    turn_interval_ms(turn_pace).map_or(k, |interval| k * TURN_REPEAT_MS as f32 / interval as f32)
}
