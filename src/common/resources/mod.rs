pub mod map;

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::common::message::Event;

#[derive(Clone, Default, Resource)]
pub struct InputQueues {
    queues: HashMap<Entity, InputQueue>,
    /// Tracks entities with non-empty queues to avoid iterating over all entities
    non_empty: HashSet<Entity>,
}

impl InputQueues {
    pub fn get(&self, ent: &Entity) -> Option<&InputQueue> {
        self.queues.get(ent)
    }
    
    pub fn get_mut(&mut self, ent: &Entity) -> Option<&mut InputQueue> {
        self.queues.get_mut(ent)
    }
    
    pub fn insert(&mut self, ent: Entity, queue: InputQueue) {
        if !queue.queue.is_empty() {
            self.non_empty.insert(ent);
        }
        self.queues.insert(ent, queue);
    }
    
    pub fn extend_one(&mut self, (ent, queue): (Entity, InputQueue)) {
        self.insert(ent, queue);
    }
    
    pub fn remove(&mut self, ent: &Entity) -> Option<InputQueue> {
        self.non_empty.remove(ent);
        self.queues.remove(ent)
    }
    
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &InputQueue)> {
        self.queues.iter().map(|(&k, v)| (k, v))
    }
    
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut InputQueue)> {
        self.queues.iter_mut().map(|(&k, v)| (k, v))
    }
    
    /// Returns iterator over entities that have non-empty queues
    pub fn non_empty_entities(&self) -> impl Iterator<Item = &Entity> {
        self.non_empty.iter()
    }
    
    /// Mark queue as non-empty when items are added
    pub fn mark_non_empty(&mut self, ent: Entity) {
        if let Some(queue) = self.queues.get(&ent) {
            if !queue.queue.is_empty() {
                self.non_empty.insert(ent);
            }
        }
    }
    
    /// Mark queue as empty when items are removed
    pub fn mark_empty_if_needed(&mut self, ent: Entity) {
        if let Some(queue) = self.queues.get(&ent) {
            if queue.queue.is_empty() {
                self.non_empty.remove(&ent);
            }
        }
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct InputQueue {
    pub queue: VecDeque<Event>, 
}
