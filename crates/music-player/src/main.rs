//! A player for the tracks, and the way a track is listened to while it
//! is made: a band playing any track in any setting at any seed is
//! composed and rendered as the game plays it (`render::take`) and played
//! once through. What plays next is, in order, the play after the current
//! one where the listener went back, the queue the listener filled — a
//! band, a track, a setting and a seed each, played now or added — and
//! only when that is empty, with autoplay on, a fresh play that keeps what
//! the listener locked of band, style, track, setting and seed; with it
//! off the player stops. A play is rendered whole into
//! memory before it plays, so the time bar can seek anywhere; the next is
//! rendered while the current plays.

// A release build opens no console window beside its own on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod banks;
mod midi;
mod player;
mod sheet;
mod theme;
mod ui;
mod worker;

use eframe::egui;

use crate::player::Player;
use crate::theme::{load_fonts, visuals};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        // One size: every panel is laid out for it.
        viewport: egui::ViewportBuilder::default().with_inner_size([880.0, 740.0]).with_resizable(false).with_maximize_button(false),
        ..Default::default()
    };
    eframe::run_native(
        "Music Player",
        options,
        Box::new(|cc| {
            // Egui follows the system's light or dark theme, each with its own
            // visuals; the window's colours are one dark set.
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx.set_visuals(visuals());
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(Player::new()))
        }),
    )
}
