use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorImpl {
    pub origin: Origin,
    pub approach: Approach,
    pub resilience: Resilience,
}

impl ActorImpl {
    pub fn new(origin: Origin, approach: Approach, resilience: Resilience) -> Self {
        ActorImpl { origin, approach, resilience }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Origin {
    Natureborn, // natural, earthly
    Synthetic, // artificial, constructed
    Dreamborn, // imagined, surreal
    Voidborn, // forgotten, insubstantial
    Mythic, // legendary, fabled
    Dimensional, // alien, otherworldly
    Indiscernible, // mysterious, unknowable
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Approach {
    Direct, // simple, straightforward, honest
    Distant, // attacks from safety, indirect, aloof
    Ambushing, // cunning, stealthy, untrustworthy
    Patient, // calculating, immobile, consistent
    Binding, // controlling, dominant, restrictive
    Evasive, // reactive, slippery, indecisive
    Overwhelming, // relentless, unstoppable, inescapable
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Resilience {
    Vital, // tough, consistent
    Mental, // focused, willfull
    Hardened, // durable, sturdy
    Shielded, // protected, guarded
    Blessed, // favored, lucky
    Primal, // raw, feral
    Eternal, // immortal, unchangable
}
