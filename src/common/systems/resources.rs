use crate::common::components::ActorAttributes;

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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test attributes
    fn test_attrs(
        might_grace: (i8, u8, i8),
        vitality_focus: (i8, u8, i8),
        instinct_presence: (i8, u8, i8),
    ) -> ActorAttributes {
        ActorAttributes {
            might_grace_axis: might_grace.0,
            might_grace_spectrum: might_grace.1,
            might_grace_shift: might_grace.2,
            vitality_focus_axis: vitality_focus.0,
            vitality_focus_spectrum: vitality_focus.1,
            vitality_focus_shift: vitality_focus.2,
            instinct_presence_axis: instinct_presence.0,
            instinct_presence_spectrum: instinct_presence.1,
            instinct_presence_shift: instinct_presence.2,
        }
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
