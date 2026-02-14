pub mod ability_prediction;
pub mod action_bar;
pub mod actor;
pub mod actor_dead_visibility;
pub mod animator;
pub mod attack_telegraph;
pub mod camera;
pub mod character_panel;
pub mod character_panel_respec;
pub mod combat;
pub mod combat_log; // ADR-025: Combat log panel for event history
pub mod combat_ui;
// pub mod combat_vignette; // REPLACED: Moved to post-processing plugin (vignette.rs)
// pub mod effect;
pub mod input;
pub mod prediction; // ADR-019: Movement prediction and VisualPosition interpolation
pub mod renet;
pub mod resolved_threats; // ADR-025: Resolved threats stack below threat queue
pub mod resource_bars;
pub mod target_frame;
pub mod target_indicator;
pub mod targeting;
pub mod threat_icons;
pub mod tier_lock_range_indicator;
pub mod ui;
pub mod world;
