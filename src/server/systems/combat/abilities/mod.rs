pub mod auto_attack;
pub mod deflect;
pub mod knockback;
pub mod lunge;
pub mod overpower;

use bevy::prelude::*;
use crate::common::{
    message::{Do, Event as GameEvent},
    systems::combat::gcd::GcdType,
};

/// Marker event to indicate an ability should trigger GCD
/// Emitted by ability systems, processed by emit_gcd system
#[derive(Event, Clone, Copy)]
pub struct TriggerGcd {
    pub ent: Entity,
    pub typ: GcdType,
}

/// System that listens to TriggerGcd events and emits GCD Do events
/// This decouples GCD emission from ability implementation
/// Runs after all ability systems complete
pub fn emit_gcd(
    mut reader: EventReader<TriggerGcd>,
    mut writer: EventWriter<Do>,
) {
    for trigger in reader.read() {
        writer.write(Do {
            event: GameEvent::Gcd {
                ent: trigger.ent,
                typ: trigger.typ,
            },
        });
    }
}
