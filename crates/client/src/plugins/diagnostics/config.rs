use bevy::prelude::*;

/// How a LoD level gives way to the coarser one across the transition
/// strip at its outer edge. The discriminant is the shaders' `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum LodTransition {
    /// A hard cut at the level's outer edge.
    Cut = 0,
    /// The level thins out over the strip by a screen-space dither, the
    /// coarser plate showing through the dropped pixels.
    Dither = 1,
    /// The level's vertices morph onto the coarser surface over the strip,
    /// so the two coincide at the cut.
    Morph = 2,
}

impl LodTransition {
    pub fn next(self) -> Self {
        match self {
            Self::Cut => Self::Dither,
            Self::Dither => Self::Morph,
            Self::Morph => Self::Cut,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Dither => "Dither",
            Self::Morph => "Morph",
        }
    }
}

#[derive(Resource)]
pub struct DiagnosticsState {
    pub grid_visible: bool,
    pub fixed_lighting_enabled: bool,
    pub metrics_overlay_visible: bool,
    /// Every camera renders without MSAA.
    pub msaa_off: bool,
    /// The sun's shadows are filtered by the hardware's 2×2 tap instead of
    /// the Gaussian.
    pub hard_shadows: bool,
    pub lod_transition: LodTransition,
}

impl Default for DiagnosticsState {
    fn default() -> Self {
        Self {
            grid_visible: false,
            fixed_lighting_enabled: true,
            metrics_overlay_visible: false,
            msaa_off: false,
            hard_shadows: false,
            lod_transition: LodTransition::Morph,
        }
    }
}
