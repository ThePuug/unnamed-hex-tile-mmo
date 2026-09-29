use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;

/// The damage an actor has taken in the last [`Grit::WINDOW`], and what it
/// has yet to take: Vitality's commitment lets no one window take more than
/// its cap of the actor's health (`ActorAttributes::grit_cap`), and defers
/// the rest into the windows after. What it defers lands in full, only
/// later. Every actor carries one; only the server fills it.
#[derive(Clone, Component, Debug, Default)]
pub struct Grit {
    taken: VecDeque<(Duration, f32)>,
    /// Damage still to land
    pub deferred: f32,
    /// Who struck the damage still to land, the last of them
    pub source: Option<Entity>,
}

impl Grit {
    /// The span no more than the cap is taken in
    pub const WINDOW: Duration = Duration::from_secs(1);

    /// What more the window ending `now` has room for under `cap`
    fn room(&mut self, now: Duration, cap: f32) -> f32 {
        while self.taken.front().is_some_and(|(at, _)| now.saturating_sub(*at) >= Self::WINDOW) {
            self.taken.pop_front();
        }
        (cap - self.taken.iter().map(|(_, damage)| damage).sum::<f32>()).max(0.0)
    }

    fn record(&mut self, now: Duration, damage: f32) -> f32 {
        if damage > 0.0 {
            self.taken.push_back((now, damage));
        }
        damage
    }

    /// Takes `damage` struck by `source` at `now` under `cap`: returns what
    /// lands now and defers the rest
    pub fn take(&mut self, now: Duration, damage: f32, cap: f32, source: Entity) -> f32 {
        let now_part = damage.min(self.room(now, cap));
        if damage > now_part {
            self.deferred += damage - now_part;
            self.source = Some(source);
        }
        self.record(now, now_part)
    }

    /// What of the deferred damage lands at `now` under `cap`
    pub fn release(&mut self, now: Duration, cap: f32) -> f32 {
        let part = self.deferred.min(self.room(now, cap));
        self.deferred -= part;
        self.record(now, part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_window_takes_more_than_the_cap_and_the_rest_lands_later() {
        let source = Entity::from_raw_u32(1).unwrap();
        let mut grit = Grit::default();
        let t = |ms| Duration::from_millis(ms);
        assert_eq!(grit.take(t(0), 30.0, 50.0, source), 30.0, "under the cap, it all lands");
        assert_eq!(grit.take(t(100), 40.0, 50.0, source), 20.0, "the window fills to the cap");
        assert_eq!(grit.deferred, 20.0, "the rest waits");
        assert_eq!(grit.release(t(500), 50.0), 0.0, "nothing more while the window is full");
        assert_eq!(grit.release(t(1000), 50.0), 20.0, "it lands as the window clears");
        assert_eq!(grit.deferred, 0.0);
    }

    #[test]
    fn a_cap_of_everything_takes_it_all_at_once() {
        let source = Entity::from_raw_u32(1).unwrap();
        let mut grit = Grit::default();
        assert_eq!(grit.take(Duration::ZERO, 500.0, f32::INFINITY, source), 500.0);
        assert_eq!(grit.deferred, 0.0);
    }
}
