pub mod behaviour;
pub mod entity_type;
pub mod gcd;
pub mod heading;
pub mod keybits;
pub mod offset;
pub mod reaction_queue;
pub mod resources;
pub mod spawner;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }
}

/// Destination for pathfinding - the Qrz location an entity is trying to reach
#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dest(pub Qrz);

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)]
pub struct Actor;

/// Attributes for actor entities that affect gameplay mechanics
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct ActorAttributes {
    // MIGHT ↔ GRACE (Physical Expression)
    // -100 = pure Might, 0 = balanced, +100 = pure Grace
    pub might_grace_axis: i8,
    pub might_grace_spectrum: u8,
    pub might_grace_shift: i8,  // Player's chosen shift from axis (within ±spectrum)

    // VITALITY ↔ FOCUS (Endurance Type)
    // -100 = pure Vitality, 0 = balanced, +100 = pure Focus
    pub vitality_focus_axis: i8,
    pub vitality_focus_spectrum: u8,
    pub vitality_focus_shift: i8,

    // INSTINCT ↔ PRESENCE (Engagement Style)
    // -100 = pure Instinct, 0 = balanced, +100 = pure Presence
    pub instinct_presence_axis: i8,
    pub instinct_presence_spectrum: u8,
    pub instinct_presence_shift: i8,
}

impl ActorAttributes {
    /// Create new ActorAttributes with raw axis, spectrum, and shift values
    pub fn new(
        might_grace_axis: i8,
        might_grace_spectrum: u8,
        might_grace_shift: i8,
        vitality_focus_axis: i8,
        vitality_focus_spectrum: u8,
        vitality_focus_shift: i8,
        instinct_presence_axis: i8,
        instinct_presence_spectrum: u8,
        instinct_presence_shift: i8,
    ) -> Self {
        Self {
            might_grace_axis,
            might_grace_spectrum,
            might_grace_shift,
            vitality_focus_axis,
            vitality_focus_spectrum,
            vitality_focus_shift,
            instinct_presence_axis,
            instinct_presence_spectrum,
            instinct_presence_shift,
        }
    }

    // === Private: Get current position (axis + shift) ===

    fn might_grace(&self) -> i8 {
        (self.might_grace_axis as i16 + self.might_grace_shift as i16).clamp(-100, 100) as i8
    }

    fn vitality_focus(&self) -> i8 {
        (self.vitality_focus_axis as i16 + self.vitality_focus_shift as i16).clamp(-100, 100) as i8
    }

    fn instinct_presence(&self) -> i8 {
        (self.instinct_presence_axis as i16 + self.instinct_presence_shift as i16).clamp(-100, 100) as i8
    }

    // === MIGHT ↔ GRACE ===

    /// Maximum Might reach with full spectrum shift
    pub fn might_reach(&self) -> u8 {
        if self.might_grace_axis <= 0 {
            // On might side or balanced: abs(axis - spectrum * 1.5)
            ((self.might_grace_axis as i16 - (self.might_grace_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On grace side: spectrum * 1.5
            (self.might_grace_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Grace reach with full spectrum shift
    pub fn grace_reach(&self) -> u8 {
        if self.might_grace_axis >= 0 {
            // On grace side or balanced: abs(axis + spectrum * 1.5)
            ((self.might_grace_axis as i16 + (self.might_grace_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On might side: spectrum * 1.5
            (self.might_grace_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Might
    pub fn might(&self) -> u8 {
        if self.might_grace_axis <= 0 {
            // On might side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.might_grace_axis as i16 + (self.might_grace_shift as i16 / 2) - self.might_grace_spectrum as i16).abs()).max(0) as u8
        } else {
            // On grace side: spectrum - shift * 0.5
            (self.might_grace_spectrum as i16 - (self.might_grace_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Grace
    pub fn grace(&self) -> u8 {
        if self.might_grace_axis >= 0 {
            // On grace side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.might_grace_axis as i16 + (self.might_grace_shift as i16 / 2) + self.might_grace_spectrum as i16).abs()).max(0) as u8
        } else {
            // On might side: spectrum + shift * 0.5
            (self.might_grace_spectrum as i16 + (self.might_grace_shift as i16 / 2)).max(0) as u8
        }
    }

    // === VITALITY ↔ FOCUS ===

    /// Maximum Vitality reach with full spectrum shift
    pub fn vitality_reach(&self) -> u8 {
        if self.vitality_focus_axis <= 0 {
            // On vitality side or balanced: abs(axis - spectrum * 1.5)
            ((self.vitality_focus_axis as i16 - (self.vitality_focus_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On focus side: spectrum * 1.5
            (self.vitality_focus_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Focus reach with full spectrum shift
    pub fn focus_reach(&self) -> u8 {
        if self.vitality_focus_axis >= 0 {
            // On focus side or balanced: abs(axis + spectrum * 1.5)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On vitality side: spectrum * 1.5
            (self.vitality_focus_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Vitality
    pub fn vitality(&self) -> u8 {
        if self.vitality_focus_axis <= 0 {
            // On vitality side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_shift as i16 / 2) - self.vitality_focus_spectrum as i16).abs()).max(0) as u8
        } else {
            // On focus side: spectrum - shift * 0.5
            (self.vitality_focus_spectrum as i16 - (self.vitality_focus_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Focus
    pub fn focus(&self) -> u8 {
        if self.vitality_focus_axis >= 0 {
            // On focus side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_shift as i16 / 2) + self.vitality_focus_spectrum as i16).abs()).max(0) as u8
        } else {
            // On vitality side: spectrum + shift * 0.5
            (self.vitality_focus_spectrum as i16 + (self.vitality_focus_shift as i16 / 2)).max(0) as u8
        }
    }

    // === INSTINCT ↔ PRESENCE ===

    /// Maximum Instinct reach with full spectrum shift
    pub fn instinct_reach(&self) -> u8 {
        if self.instinct_presence_axis <= 0 {
            // On instinct side or balanced: abs(axis - spectrum * 1.5)
            ((self.instinct_presence_axis as i16 - (self.instinct_presence_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On presence side: spectrum * 1.5
            (self.instinct_presence_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Presence reach with full spectrum shift
    pub fn presence_reach(&self) -> u8 {
        if self.instinct_presence_axis >= 0 {
            // On presence side or balanced: abs(axis + spectrum * 1.5)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On instinct side: spectrum * 1.5
            (self.instinct_presence_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Instinct
    pub fn instinct(&self) -> u8 {
        if self.instinct_presence_axis <= 0 {
            // On instinct side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_shift as i16 / 2) - self.instinct_presence_spectrum as i16).abs()).max(0) as u8
        } else {
            // On presence side: spectrum - shift * 0.5
            (self.instinct_presence_spectrum as i16 - (self.instinct_presence_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Presence
    pub fn presence(&self) -> u8 {
        if self.instinct_presence_axis >= 0 {
            // On presence side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_shift as i16 / 2) + self.instinct_presence_spectrum as i16).abs()).max(0) as u8
        } else {
            // On instinct side: spectrum + shift * 0.5
            (self.instinct_presence_spectrum as i16 + (self.instinct_presence_shift as i16 / 2)).max(0) as u8
        }
    }

    // === DERIVED ATTRIBUTES ===

    /// Movement speed derived from grace_reach
    /// Higher grace reach = higher max speed potential
    /// 0 grace_reach = baseline (0.005)
    /// 100 grace_reach = +15% faster (0.00575)
    pub fn movement_speed(&self) -> f32 {
        let base = 0.005;
        let grace_reach = self.grace_reach() as f32;
        base * (1.0 + (grace_reach / 100.0) * 0.15)
    }

    /// Maximum health derived from vitality_reach
    /// Higher vitality reach = higher max health potential
    /// 0 vitality_reach = 100 HP
    /// 100 vitality_reach = 2000 HP
    pub fn max_health(&self) -> f32 {
        let base = 100.0;
        let vitality_reach = self.vitality_reach() as f32;
        base + (vitality_reach * 19.0)
    }
}

impl Default for ActorAttributes {
    fn default() -> Self {
        Self {
            might_grace_axis: 0,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            vitality_focus_axis: 0,
            vitality_focus_spectrum: 0,
            vitality_focus_shift: 0,
            instinct_presence_axis: 0,
            instinct_presence_spectrum: 0,
            instinct_presence_shift: 0,
        }
    }
}

#[derive(Clone, Component, Copy, Default)]
pub struct Physics;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();

/// Tracks the last time an auto-attack was performed (ADR-009)
/// Used to enforce 1.5s cooldown between passive auto-attacks
#[derive(Clone, Component, Copy, Debug)]
pub struct LastAutoAttack {
    /// Game time when last auto-attack was performed (server time + offset)
    pub last_attack_time: std::time::Duration,
}

impl Default for LastAutoAttack {
    fn default() -> Self {
        Self {
            last_attack_time: std::time::Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_attributes() {
        // axis=-50, spectrum=25: -50A/25S (on might side)
        // might_reach = abs(-50 - 25*1.5) = abs(-50 - 37) = 87
        // grace_reach = 25 * 1.5 = 37
        let base = ActorAttributes {
            might_grace_axis: -50,
            might_grace_spectrum: 25,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -25, ..base };
        assert_eq!(attrs.might_reach(), 87);  // abs(-50 - 37)
        assert_eq!(attrs.might(), 87);  // abs(-50 + (-25)/2 - 25) = abs(-87) = 87
        assert_eq!(attrs.grace(), 13);  // 25 + (-25)/2 = 25 + (-12) = 13 (integer division)
        assert_eq!(attrs.grace_reach(), 37);  // 25 * 1.5

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 80);  // abs(-50 + (-10)/2 - 25) = abs(-80) = 80
        assert_eq!(attrs.grace(), 20);  // 25 + (-10)/2 = 25 - 5 = 20
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 75);  // abs(-50 + 0 - 25) = abs(-75) = 75
        assert_eq!(attrs.grace(), 25);  // 25 + 0 = 25
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 70);  // abs(-50 + 10/2 - 25) = abs(-70) = 70
        assert_eq!(attrs.grace(), 30);  // 25 + 10/2 = 25 + 5 = 30
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 25, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 63);  // abs(-50 + 25/2 - 25) = abs(-50 + 12 - 25) = abs(-63) = 63
        assert_eq!(attrs.grace(), 37);  // 25 + 25/2 = 25 + 12 = 37
        assert_eq!(attrs.grace_reach(), 37);
    }

    #[test]
    fn test_perfectly_balanced_attributes() {
        // Perfectly balanced: axis=0, spectrum=20: 0A/20S
        // might_reach = abs(0 - 20*1.5) = abs(-30) = 30
        // grace_reach = abs(0 + 20*1.5) = abs(30) = 30
        let base = ActorAttributes {
            might_grace_axis: 0,
            might_grace_spectrum: 20,
            ..Default::default()
        };

        // Shift fully toward might
        let attrs = ActorAttributes { might_grace_shift: -20, ..base };
        assert_eq!(attrs.might_reach(), 30);  // abs(0 - 30) = 30
        assert_eq!(attrs.might(), 30);  // abs(0 + (-20)/2 - 20) = abs(-30) = 30
        assert_eq!(attrs.grace(), 10);  // abs(0 + (-20)/2 + 20) = abs(10) = 10
        assert_eq!(attrs.grace_reach(), 30);  // abs(0 + 30) = 30

        // Shift partially toward might
        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 25);  // abs(0 + (-10)/2 - 20) = abs(-25) = 25
        assert_eq!(attrs.grace(), 15);  // abs(0 + (-10)/2 + 20) = abs(15) = 15
        assert_eq!(attrs.grace_reach(), 30);

        // No shift (perfectly centered)
        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 20);  // abs(0 + 0 - 20) = abs(-20) = 20
        assert_eq!(attrs.grace(), 20);  // abs(0 + 0 + 20) = abs(20) = 20
        assert_eq!(attrs.grace_reach(), 30);

        // Shift partially toward grace
        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 15);  // abs(0 + 10/2 - 20) = abs(-15) = 15
        assert_eq!(attrs.grace(), 25);  // abs(0 + 10/2 + 20) = abs(25) = 25
        assert_eq!(attrs.grace_reach(), 30);

        // Shift fully toward grace
        let attrs = ActorAttributes { might_grace_shift: 20, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 10);  // abs(0 + 20/2 - 20) = abs(-10) = 10
        assert_eq!(attrs.grace(), 30);  // abs(0 + 20/2 + 20) = abs(30) = 30
        assert_eq!(attrs.grace_reach(), 30);
    }

    #[test]
    fn test_narrow_spectrum_attributes() {
        // axis=-80, spectrum=10: -80A/10S (on might side)
        // might_reach = abs(-80 - 10*1.5) = abs(-80 - 15) = 95
        // grace_reach = 10 * 1.5 = 15
        let base = ActorAttributes {
            might_grace_axis: -80,
            might_grace_spectrum: 10,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 95);  // abs(-80 - 15) = 95
        assert_eq!(attrs.might(), 95);  // abs(-80 + (-10)/2 - 10) = abs(-95) = 95
        assert_eq!(attrs.grace(), 5);  // 10 + (-10)/2 = 10 - 5 = 5
        assert_eq!(attrs.grace_reach(), 15);  // 10 * 1.5 = 15

        let attrs = ActorAttributes { might_grace_shift: -5, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 92);  // abs(-80 + (-5)/2 - 10) = abs(-92) = 92
        assert_eq!(attrs.grace(), 8);  // 10 + (-5)/2 = 10 + (-2) = 8 (integer division)
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 90);  // abs(-80 + 0 - 10) = abs(-90) = 90
        assert_eq!(attrs.grace(), 10);  // 10 + 0 = 10
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 5, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 88);  // abs(-80 + 5/2 - 10) = abs(-88) = 88
        assert_eq!(attrs.grace(), 12);  // 10 + 5/2 = 10 + 2 = 12
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 85);  // abs(-80 + 10/2 - 10) = abs(-85) = 85
        assert_eq!(attrs.grace(), 15);  // 10 + 10/2 = 10 + 5 = 15
        assert_eq!(attrs.grace_reach(), 15);
    }

}