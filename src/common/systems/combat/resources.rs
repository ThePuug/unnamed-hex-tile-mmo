use bevy::prelude::*;
use crate::common::{
    components::{ActorAttributes, Loc, offset::Offset, resources::*},
    message::{Component as MessageComponent, Event, *},
};

/// Calculate maximum stamina from actor attributes
/// Formula: 100 + (might * 0.5) + (vitality * 0.3)
pub fn calculate_max_stamina(attrs: &ActorAttributes) -> f32 {
    let might = attrs.might() as f32;
    let vitality = attrs.vitality() as f32;
    100.0 + (might * 0.5) + (vitality * 0.3)
}

/// Calculate maximum mana from actor attributes
/// Formula: 100 + (focus * 0.5) + (presence * 0.3)
pub fn calculate_max_mana(attrs: &ActorAttributes) -> f32 {
    let focus = attrs.focus() as f32;
    let presence = attrs.presence() as f32;
    100.0 + (focus * 0.5) + (presence * 0.3)
}

/// Calculate stamina regeneration rate
/// Base: 10/sec (may scale with attributes in future)
pub fn calculate_stamina_regen_rate(_attrs: &ActorAttributes) -> f32 {
    10.0
}

/// Calculate mana regeneration rate
/// Base: 8/sec (may scale with attributes in future)
pub fn calculate_mana_regen_rate(_attrs: &ActorAttributes) -> f32 {
    8.0
}

/// Calculate armor (physical damage reduction) from actor attributes
/// Formula: base_armor + (vitality / 200.0)
/// Capped at 75% max
pub fn calculate_armor(attrs: &ActorAttributes, base_armor: f32) -> f32 {
    let vitality = attrs.vitality() as f32;
    let armor = base_armor + (vitality / 200.0);
    armor.min(0.75)
}

/// Calculate resistance (magic damage reduction) from actor attributes
/// Formula: base_resistance + (focus / 200.0)
/// Capped at 75% max
pub fn calculate_resistance(attrs: &ActorAttributes, base_resistance: f32) -> f32 {
    let focus = attrs.focus() as f32;
    let resistance = base_resistance + (focus / 200.0);
    resistance.min(0.75)
}

/// Regenerate stamina and mana for all entities with resources
/// Runs in FixedUpdate schedule (125ms ticks)
/// Does NOT regenerate health (per spec - healing abilities only)
pub fn regenerate_resources(
    mut query: Query<(&mut Stamina, &mut Mana)>,
    time: Res<Time>,
) {
    let current_time = time.elapsed();

    for (mut stamina, mut mana) in &mut query {
        // Calculate time since last update (in seconds)
        let dt_stamina = (current_time - stamina.last_update).as_secs_f32();
        let dt_mana = (current_time - mana.last_update).as_secs_f32();

        // Regenerate stamina
        stamina.state = (stamina.state + stamina.regen_rate * dt_stamina).min(stamina.max);
        stamina.step = stamina.state; // Sync step with state for remote entities
        stamina.last_update = current_time;

        // Regenerate mana
        mana.state = (mana.state + mana.regen_rate * dt_mana).min(mana.max);
        mana.step = mana.state;
        mana.last_update = current_time;
    }
}

/// Check for entities with health <= 0 and emit death events
/// Runs on server only, after damage application systems
/// Emits Try::Death events for the death handler to process
pub fn check_death(
    mut writer: EventWriter<Try>,
    query: Query<(Entity, &Health), Without<RespawnTimer>>,
) {
    for (ent, health) in &query {
        if health.state <= 0.0 {
            writer.write(Try {
                event: Event::Death { ent },
            });
        }
    }
}

/// Process respawn timers and respawn players at origin after 5 seconds
/// Runs on server only
pub fn process_respawn(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Entity, &RespawnTimer, &mut Health, &mut Stamina, &mut Mana, &mut Loc, &mut Offset, &crate::common::components::ActorAttributes)>,
) {
    use qrz::Qrz;
    use bevy::math::Vec3;

    for (ent, timer, mut health, mut stamina, mut mana, mut loc, mut offset, attrs) in &mut query {
        if timer.should_respawn(time.elapsed()) {
            // Teleport to origin
            let spawn_qrz = Qrz { q: 0, r: 0, z: 4 };
            *loc = Loc::new(spawn_qrz);

            // Clear offset to snap to new position (no interpolation)
            offset.state = Vec3::ZERO;
            offset.step = Vec3::ZERO;
            offset.prev_step = Vec3::ZERO;

            // Restore resources to full
            health.state = health.max;
            health.step = health.max;
            stamina.state = stamina.max;
            stamina.step = stamina.max;
            mana.state = mana.max;
            mana.step = mana.max;

            // Remove respawn timer
            commands.entity(ent).remove::<RespawnTimer>();

            // Re-spawn the player on client (was despawned on death)
            // Send Spawn event to re-create client entity
            writer.write(Do {
                event: Event::Spawn {
                    ent,
                    typ: crate::common::components::entity_type::EntityType::Actor(
                        crate::common::components::entity_type::actor::ActorImpl {
                            origin: crate::common::components::entity_type::actor::Origin::Natureborn,
                            approach: crate::common::components::entity_type::actor::Approach::Direct,
                            resilience: crate::common::components::entity_type::actor::Resilience::Vital,
                        }
                    ),
                    qrz: spawn_qrz,
                    attrs: Some(*attrs),
                },
            });

            // Broadcast resource updates (sent after Spawn so client entity exists)
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Health(*health),
                },
            });
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Stamina(*stamina),
                },
            });
            writer.write(Do {
                event: Event::Incremental {
                    ent,
                    component: MessageComponent::Mana(*mana),
                },
            });
        }
    }
}

/// Handle death events for players and NPCs
/// Runs on server only as an observer for Death events
/// For NPCs: emit Despawn event (actual despawn happens in cleanup_despawned in PostUpdate)
/// For players: add respawn timer (5 seconds), will respawn at origin (0,0,4)
pub fn handle_death(
    trigger: Trigger<Try>,
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
    mut query: Query<(Option<&crate::common::components::behaviour::Behaviour>, &mut Health, &mut Stamina, &mut Mana)>,
) {
    let event = &trigger.event().event;
    if let Event::Death { ent } = event {
        // Check if this is a player or NPC and set resources to 0
        let is_player = if let Ok((behaviour, mut health, mut stamina, mut mana)) = query.get_mut(*ent) {
            // Set resources to 0 to prevent "zombie" state
            health.state = 0.0;
            health.step = 0.0;
            stamina.state = 0.0;
            stamina.step = 0.0;
            mana.state = 0.0;
            mana.step = 0.0;

            behaviour
                .map(|b| matches!(b, crate::common::components::behaviour::Behaviour::Controlled))
                .unwrap_or(false)
        } else {
            false
        };

        if is_player {
            // Player death: add respawn timer (5 seconds) and despawn from client view
            commands.entity(*ent).insert(RespawnTimer::new(time.elapsed()));

            // Send Despawn to client so player disappears visually
            writer.write(Do {
                event: Event::Despawn { ent: *ent },
            });
        } else {
            // NPC death: emit Despawn event (actual despawn happens in PostUpdate)
            writer.write(Do {
                event: Event::Despawn { ent: *ent },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes
    fn test_attrs(
        might_grace: (i8, u8, i8),
        vitality_focus: (i8, u8, i8),
        instinct_presence: (i8, u8, i8),
    ) -> ActorAttributes {
        ActorAttributes::new(
            might_grace.0, might_grace.1, might_grace.2,
            vitality_focus.0, vitality_focus.1, vitality_focus.2,
            instinct_presence.0, instinct_presence.1, instinct_presence.2,
        )
    }

    #[test]
    fn test_max_stamina_baseline() {
        // Balanced attributes with zero spectrum (0 might, 0 vitality)
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_max_stamina(&attrs), 100.0);
    }

    #[test]
    fn test_max_stamina_with_might() {
        // Might-heavy build: -100A/50S → 150 might, 0 vitality
        // stamina = 100 + (150 * 0.5) + (0 * 0.3) = 175
        let attrs = test_attrs((-100, 50, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(attrs.might(), 150);
        assert_eq!(attrs.vitality(), 0);
        assert_eq!(calculate_max_stamina(&attrs), 175.0);
    }

    #[test]
    fn test_max_stamina_with_vitality() {
        // Vitality-heavy build: -100A/50S → 0 might, 150 vitality
        // stamina = 100 + (0 * 0.5) + (150 * 0.3) = 145
        let attrs = test_attrs((0, 0, 0), (-100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.might(), 0);
        assert_eq!(attrs.vitality(), 150);
        assert_eq!(calculate_max_stamina(&attrs), 145.0);
    }

    #[test]
    fn test_max_stamina_balanced() {
        // Balanced build: 0A/50S → 50 might, 50 vitality
        // stamina = 100 + (50 * 0.5) + (50 * 0.3) = 140
        let attrs = test_attrs((0, 50, 0), (0, 50, 0), (0, 0, 0));
        assert_eq!(attrs.might(), 50);
        assert_eq!(attrs.vitality(), 50);
        assert_eq!(calculate_max_stamina(&attrs), 140.0);
    }

    #[test]
    fn test_max_mana_baseline() {
        // Balanced attributes with zero spectrum (0 focus, 0 presence)
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_max_mana(&attrs), 100.0);
    }

    #[test]
    fn test_max_mana_with_focus() {
        // Focus-heavy build: 100A/50S → 0 presence, 150 focus
        // mana = 100 + (150 * 0.5) + (0 * 0.3) = 175
        let attrs = test_attrs((0, 0, 0), (100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.focus(), 150);
        assert_eq!(attrs.presence(), 0);
        assert_eq!(calculate_max_mana(&attrs), 175.0);
    }

    #[test]
    fn test_max_mana_with_presence() {
        // Presence-heavy build: 100A/50S → 150 presence, 0 focus
        // mana = 100 + (0 * 0.5) + (150 * 0.3) = 145
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (100, 50, 0));
        assert_eq!(attrs.focus(), 0);
        assert_eq!(attrs.presence(), 150);
        assert_eq!(calculate_max_mana(&attrs), 145.0);
    }

    #[test]
    fn test_max_mana_balanced() {
        // Balanced build: 0A/50S → 50 focus, 50 presence
        // mana = 100 + (50 * 0.5) + (50 * 0.3) = 140
        let attrs = test_attrs((0, 0, 0), (0, 50, 0), (0, 50, 0));
        assert_eq!(attrs.focus(), 50);
        assert_eq!(attrs.presence(), 50);
        assert_eq!(calculate_max_mana(&attrs), 140.0);
    }

    #[test]
    fn test_stamina_regen_rate() {
        // All attributes return base 10/sec for now
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_stamina_regen_rate(&attrs), 10.0);

        let attrs = test_attrs((-100, 50, 0), (100, 50, 0), (0, 0, 0));
        assert_eq!(calculate_stamina_regen_rate(&attrs), 10.0);
    }

    #[test]
    fn test_mana_regen_rate() {
        // All attributes return base 8/sec for now
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_mana_regen_rate(&attrs), 8.0);

        let attrs = test_attrs((0, 0, 0), (100, 50, 0), (100, 50, 0));
        assert_eq!(calculate_mana_regen_rate(&attrs), 8.0);
    }

    #[test]
    fn test_armor_baseline() {
        // 0 vitality = base armor only
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_armor(&attrs, 0.0), 0.0);
        assert_eq!(calculate_armor(&attrs, 0.1), 0.1);
    }

    #[test]
    fn test_armor_with_vitality() {
        // 100 vitality = base + 0.5 (50% reduction)
        let attrs = test_attrs((0, 0, 0), (-100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.vitality(), 150);
        assert_eq!(calculate_armor(&attrs, 0.0), 0.75); // 150/200 = 0.75, but capped
    }

    #[test]
    fn test_armor_cap() {
        // Very high vitality should cap at 75%
        let attrs = test_attrs((0, 0, 0), (-100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.vitality(), 150);
        assert_eq!(calculate_armor(&attrs, 0.5), 0.75); // 0.5 + 0.75 = 1.25, capped at 0.75
    }

    #[test]
    fn test_resistance_baseline() {
        // 0 focus = base resistance only
        let attrs = test_attrs((0, 0, 0), (0, 0, 0), (0, 0, 0));
        assert_eq!(calculate_resistance(&attrs, 0.0), 0.0);
        assert_eq!(calculate_resistance(&attrs, 0.1), 0.1);
    }

    #[test]
    fn test_resistance_with_focus() {
        // 100 focus = base + 0.5 (50% reduction)
        let attrs = test_attrs((0, 0, 0), (100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.focus(), 150);
        assert_eq!(calculate_resistance(&attrs, 0.0), 0.75); // 150/200 = 0.75, capped
    }

    #[test]
    fn test_resistance_cap() {
        // Very high focus should cap at 75%
        let attrs = test_attrs((0, 0, 0), (100, 50, 0), (0, 0, 0));
        assert_eq!(attrs.focus(), 150);
        assert_eq!(calculate_resistance(&attrs, 0.5), 0.75); // 0.5 + 0.75 = 1.25, capped at 0.75
    }

    #[test]
    fn test_extreme_attributes() {
        // Test with extreme values (edge cases)
        let attrs = test_attrs((-100, 100, 0), (-100, 100, 0), (-100, 100, 0));

        // Should handle large values without panic
        let stamina = calculate_max_stamina(&attrs);
        assert!(stamina >= 100.0);

        let mana = calculate_max_mana(&attrs);
        assert!(mana >= 100.0);

        let armor = calculate_armor(&attrs, 0.0);
        assert!(armor <= 0.75);

        let resistance = calculate_resistance(&attrs, 0.0);
        assert!(resistance <= 0.75);
    }
}
