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
        // Queue invariant: all queues must have at least 1 input when inserted
        assert!(
            !queue.queue.is_empty(),
            "Queue invariant violation: attempted to insert empty queue for entity {ent}"
        );
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
    
    /// Returns iterator over all entities with queues
    /// Note: All queues always have at least 1 input (the accumulating one)
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.queues.keys()
    }

    #[cfg(test)]
    pub fn insert_for_test(&mut self, ent: Entity, queue: InputQueue) {
        // Test helper: bypasses invariant check to allow testing panic conditions
        self.queues.insert(ent, queue);
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct InputQueue {
    pub queue: VecDeque<Event>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::components::keybits::KeyBits;

    #[test]
    #[should_panic(expected = "Queue invariant violation: attempted to insert empty queue")]
    fn test_insert_empty_queue_panics() {
        let mut buffers = InputQueues::default();
        let entity = Entity::from_raw_u32(1).unwrap();
        let empty_queue = InputQueue::default(); // Empty queue

        // This should panic due to invariant violation
        buffers.insert(entity, empty_queue);
    }

    #[test]
    #[should_panic(expected = "Queue invariant violation: attempted to insert empty queue")]
    fn test_extend_one_empty_queue_panics() {
        let mut buffers = InputQueues::default();
        let entity = Entity::from_raw_u32(1).unwrap();
        let empty_queue = InputQueue::default(); // Empty queue

        // This should panic due to invariant violation
        buffers.extend_one((entity, empty_queue));
    }

    #[test]
    fn test_insert_non_empty_queue_succeeds() {
        let mut buffers = InputQueues::default();
        let entity = Entity::from_raw_u32(1).unwrap();
        let mut queue = InputQueue::default();
        queue.queue.push_back(Event::Input {
            ent: entity,
            key_bits: KeyBits::default(),
            dt: 0,
            seq: 0,
        });

        // This should NOT panic
        buffers.insert(entity, queue);

        // Verify the queue was inserted
        assert!(buffers.get(&entity).is_some());
        assert_eq!(buffers.get(&entity).unwrap().queue.len(), 1);
    }

    #[test]
    fn test_extend_one_non_empty_queue_succeeds() {
        let mut buffers = InputQueues::default();
        let entity = Entity::from_raw_u32(1).unwrap();
        let mut queue = InputQueue::default();
        queue.queue.push_back(Event::Input {
            ent: entity,
            key_bits: KeyBits::default(),
            dt: 0,
            seq: 0,
        });

        // This should NOT panic
        buffers.extend_one((entity, queue));

        // Verify the queue was inserted
        assert!(buffers.get(&entity).is_some());
        assert_eq!(buffers.get(&entity).unwrap().queue.len(), 1);
    }
}
