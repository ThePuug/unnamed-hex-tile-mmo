pub mod heading;
pub mod keybits;
pub mod offset;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorDescriptor {
    pub origin: Origin,
    pub form: Form,
    pub manifestation: Manifestation,
}

impl ActorDescriptor {
    pub fn new(origin: Origin, form: Form, manifestation: Manifestation) -> Self {
        ActorDescriptor { origin, form, manifestation }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecoratorDescriptor {
    pub index: usize,
    pub is_solid: bool,
}

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EntityType {
    Actor(ActorDescriptor),
    Decorator(DecoratorDescriptor),
}

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)] 
pub struct Actor;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();