//! A rune as drawn: the stone every rune shares, and the rune's own glyph
//! hung from the stone's `socket.glyph` node, the face the stone declares,
//! once both scenes are in. A change to the stone moves every glyph with it.

use bevy::prelude::*;
use common_bevy::components::rune::Rune;

use super::equipment::{hang, joints_of, SocketAnchor};

/// The stone every rune stands as
pub const STONE: &str = "models/runestone.glb";

/// A rune's glyph scene, a child of its stone until it hangs from the
/// stone's socket
#[derive(Component, Default)]
pub struct Glyph {
    bound: bool,
}

/// A rune's glyph under `models/`, named for it: Sharpened Blades as
/// `glyph-sharpened-blades`
pub fn glyph_asset(rune: Rune) -> String {
    let name: String = format!("{rune:?}").chars().enumerate()
        .flat_map(|(i, c)| (i > 0 && c.is_uppercase()).then_some('-').into_iter().chain(c.to_lowercase()))
        .collect();
    format!("models/glyph-{name}.glb")
}

/// Hangs each glyph not yet hung from its stone's socket, once the stone's
/// scene holds the socket and the glyph's names it.
pub fn bind(
    mut commands: Commands,
    mut glyphs: Query<(Entity, &ChildOf, &mut Glyph)>,
    children: Query<&Children>,
    names: Query<&Name>,
    anchors: Query<&SocketAnchor>,
) {
    for (glyph, child_of, mut state) in glyphs.iter_mut().filter(|(.., state)| !state.bound) {
        let joints = joints_of(child_of.parent(), &children, &names);
        if hang(glyph, &joints, &children, &anchors, &mut commands).is_some() {
            state.bound = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glyph_is_named_for_its_rune() {
        assert_eq!(glyph_asset(Rune::Drag), "models/glyph-drag.glb");
        assert_eq!(glyph_asset(Rune::SharpenedBlades), "models/glyph-sharpened-blades.glb");
    }

    #[test]
    fn every_glyph_a_skill_cuts_is_shipped() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/");
        assert!(std::path::Path::new(&format!("{root}{STONE}")).exists(), "{STONE}");
        for rune in Rune::ALL.into_iter().filter(|rune| rune.ability().is_some()) {
            let path = glyph_asset(rune);
            assert!(std::path::Path::new(&format!("{root}{path}")).exists(), "{path}");
        }
    }
}
