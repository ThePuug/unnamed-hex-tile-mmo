//! A point in time, never a number: a float is made from the span between
//! two moments and never from one (AGENTS.md anti-pattern 11).

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// A moment on a clock, held to the nanosecond: the game clock's, read
/// from the server's uptime, or a client's own. It adds a span and takes
/// one from another moment, and that is all: it has no float and no count,
/// so a timer cannot round it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Moment(u64);

impl Moment {
    /// When the clock read 0.
    pub const ZERO: Moment = Moment(0);

    /// `ms` milliseconds after the clock read 0: for the clock itself, and
    /// for tests. Everything else reaches a moment by `+ Duration`.
    pub const fn from_millis(ms: u64) -> Self {
        Moment(ms * 1_000_000)
    }

    /// The span from `earlier` to this moment, none where `earlier` is later.
    pub fn since(self, earlier: Moment) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// The span from this moment to `later`, none where it has passed.
    pub fn until(self, later: Moment) -> Duration {
        later.since(self)
    }
}

impl Add<Duration> for Moment {
    type Output = Moment;
    fn add(self, span: Duration) -> Moment {
        Moment(self.0 + span.as_nanos() as u64)
    }
}

impl Sub<Duration> for Moment {
    type Output = Moment;
    fn sub(self, span: Duration) -> Moment {
        Moment(self.0.saturating_sub(span.as_nanos() as u64))
    }
}

impl AddAssign<Duration> for Moment {
    fn add_assign(&mut self, span: Duration) {
        *self = *self + span;
    }
}

impl SubAssign<Duration> for Moment {
    fn sub_assign(&mut self, span: Duration) {
        *self = *self - span;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_is_taken_between_moments_and_never_runs_backwards() {
        let start = Moment::from_millis(1_000);
        let later = start + Duration::from_millis(250);
        assert_eq!(later.since(start), Duration::from_millis(250));
        assert_eq!(start.until(later), Duration::from_millis(250));
        assert_eq!(start.since(later), Duration::ZERO);
        assert_eq!(later - Duration::from_secs(5), Moment::ZERO, "taken past zero, it stops there");
        assert!(start < later);
    }
}
