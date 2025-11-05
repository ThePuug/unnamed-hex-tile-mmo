use bevy::prelude::*;

/// AllyTarget component for tracking which ally entity is currently targeted
/// Used for friendly abilities and target frames
/// Updated reactively when heading or location changes
///
/// Parallel to the Target component but for allies instead of hostiles
#[derive(Clone, Component, Copy, Debug)]
pub struct AllyTarget {
    /// The currently selected ally target (updated by targeting system)
    pub entity: Option<Entity>,
    /// The last ally target (sticky for UI - persists even when no current target)
    pub last_target: Option<Entity>,
}

impl AllyTarget {
    pub fn new() -> Self {
        Self {
            entity: None,
            last_target: None,
        }
    }

    /// Set the ally target and update last_target
    pub fn set(&mut self, target: Entity) {
        self.entity = Some(target);
        self.last_target = Some(target);
    }

    /// Clear the current ally target (but keep last_target for sticky UI)
    pub fn clear(&mut self) {
        self.entity = None;
        // Keep last_target for sticky UI behavior
    }

    /// Get the current ally target
    pub fn get(&self) -> Option<Entity> {
        self.entity
    }

    /// Check if there is a current ally target
    pub fn is_some(&self) -> bool {
        self.entity.is_some()
    }

    /// Check if there is no current ally target
    pub fn is_none(&self) -> bool {
        self.entity.is_none()
    }
}

impl Default for AllyTarget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ally_target_is_empty() {
        let target = AllyTarget::new();
        assert_eq!(target.entity, None);
        assert_eq!(target.last_target, None);
        assert!(target.is_none());
        assert!(!target.is_some());
    }

    #[test]
    fn test_set_ally_target_updates_both_fields() {
        let mut target = AllyTarget::new();
        let entity = Entity::from_raw(42);

        target.set(entity);

        assert_eq!(target.entity, Some(entity));
        assert_eq!(target.last_target, Some(entity));
        assert!(target.is_some());
        assert!(!target.is_none());
    }

    #[test]
    fn test_clear_keeps_last_target() {
        let mut target = AllyTarget::new();
        let entity = Entity::from_raw(42);

        target.set(entity);
        target.clear();

        assert_eq!(target.entity, None);
        assert_eq!(target.last_target, Some(entity)); // Sticky behavior
        assert!(target.is_none());
        assert!(!target.is_some());
    }

    #[test]
    fn test_get_returns_current_entity() {
        let mut target = AllyTarget::new();
        assert_eq!(target.get(), None);

        let entity = Entity::from_raw(42);
        target.set(entity);
        assert_eq!(target.get(), Some(entity));

        target.clear();
        assert_eq!(target.get(), None);
        // Note: last_target is still set but get() returns entity field
    }

    #[test]
    fn test_set_overwrites_previous_target() {
        let mut target = AllyTarget::new();
        let entity1 = Entity::from_raw(42);
        let entity2 = Entity::from_raw(99);

        target.set(entity1);
        assert_eq!(target.entity, Some(entity1));
        assert_eq!(target.last_target, Some(entity1));

        target.set(entity2);
        assert_eq!(target.entity, Some(entity2));
        assert_eq!(target.last_target, Some(entity2));
    }

    #[test]
    fn test_default_is_same_as_new() {
        let target1 = AllyTarget::new();
        let target2 = AllyTarget::default();

        assert_eq!(target1.entity, target2.entity);
        assert_eq!(target1.last_target, target2.last_target);
    }
}
