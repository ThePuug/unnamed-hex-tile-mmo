pub mod map;

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use crate::common::message::Event;

#[derive(Clone, Default, Resource)]
pub struct InputQueues {
    queues: HashMap<Entity, InputQueue>,
}

impl InputQueues {
    pub fn get(&self, ent: &Entity) -> Option<&InputQueue> {
        self.queues.get(ent)
    }
    
    pub fn get_mut(&mut self, ent: &Entity) -> Option<&mut InputQueue> {
        self.queues.get_mut(ent)
    }
    
    pub fn insert(&mut self, ent: Entity, queue: InputQueue) {
        self.queues.insert(ent, queue);
    }
    
    pub fn extend_one(&mut self, (ent, queue): (Entity, InputQueue)) {
        self.insert(ent, queue);
    }
    
    pub fn remove(&mut self, ent: &Entity) -> Option<InputQueue> {
        self.queues.remove(ent)
    }
    
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &InputQueue)> {
        self.queues.iter().map(|(&k, v)| (k, v))
    }
    
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut InputQueue)> {
        self.queues.iter_mut().map(|(&k, v)| (k, v))
    }

    /// Returns iterator over all entities with queues
    /// Note: All queues always have at least 1 input (the accumulating one)
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.queues.keys()
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct InputQueue {
    pub queue: VecDeque<Event>, 
}
