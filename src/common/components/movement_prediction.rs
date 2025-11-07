use bevy::prelude::*;
use qrz::Qrz;
use std::time::Duration;

/// Client-side component for tracking predicted movement (ADR-011)
///
/// When a MovementIntent arrives, the client predicts where the entity will move
/// and stores prediction metadata here for later validation against Loc confirmations.
#[derive(Component, Debug)]
pub struct MovementPrediction {
    /// Where we think entity is going
    pub predicted_dest: Qrz,
    /// When we expect arrival (Time::elapsed() + duration)
    pub predicted_arrival: Duration,
    /// When we started predicting (Time::elapsed())
    pub prediction_start: Duration,
}

impl MovementPrediction {
    pub fn new(predicted_dest: Qrz, predicted_arrival: Duration, prediction_start: Duration) -> Self {
        Self {
            predicted_dest,
            predicted_arrival,
            prediction_start,
        }
    }
}
