use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorImpl {
    pub origin: Origin,
    pub form: Form,
    pub manifestation: Manifestation,
}

impl ActorImpl {
    pub fn new(origin: Origin, form: Form, manifestation: Manifestation) -> Self {
        ActorImpl { origin, form, manifestation }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Origin {
    Fauna,         // Evolved from natural ecosystems and biological life.
    Dimensional,   // Emerged across boundaries of metaphysical realms.
    Mythic,        // Born from legend, archetype, or divine belief.
    Synthetic,     // Crafted by artificial means or intelligent design.
    Starborn,      // Coming from another planet or star system.
    Dreamborn,     // Formed from dreams, thoughts, or subconscious realms.
    Voidborn,      // Manifested from emptiness, negation, or unreality.
    Natureborn,    // Grown from elemental wilderness or living terrain.
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Form {
    Humanoid,      // Bipedal, expressive, capable of tool use or speech.
    Bestial,       // Animalistic, primal, driven by instinct and motion.
    Skittering,    // Rapid, erratic movement with insectile or jointed gait.
    Swarming,      // Operates through coordinated collective behavior.
    Anchored,      // Stationary but extends power through reach or terrain.
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Manifestation {
    Physical,      // Interacts through brute strength and tangible force.
    Elemental,     // Commands fire, ice, lightning, or primal energy.
    Psychic,       // Manipulates minds, emotions, or mental space.
    Arcane,        // Channels symbolic magic through ritual or spellcraft.
    Umbral,        // Distorts perception through obscured or warped presence.
}
