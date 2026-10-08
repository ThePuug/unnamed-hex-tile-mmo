//! What one source has published, as the console keeps it: each field by
//! `topic/field` with its last `HISTORY_LEN` values, oldest first.

use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

use common::metrics::MetricsPacket;

/// Values kept of each field: four minutes of the server's publications,
/// one of the client's.
pub const HISTORY_LEN: usize = 120;

/// How long a source may go unheard and still count as connected.
const LIVE_SECS: f64 = 5.0;

#[derive(Default)]
pub struct Feed {
    fields: HashMap<String, VecDeque<f64>>,
    /// Every field in the order first heard. A publisher sends its fields
    /// in a fixed order, which a page can read back: the server's world
    /// layers come in the order the stack evaluates them.
    order: Vec<String>,
    heard: Option<Instant>,
    /// Each topic's last two publications, by the publisher's clock.
    clocks: HashMap<String, (f64, f64)>,
}

impl Feed {
    pub fn hear(&mut self, packet: MetricsPacket) {
        self.heard = Some(Instant::now());
        let clock = self.clocks.entry(packet.group.to_string()).or_default();
        *clock = (clock.1, packet.timestamp_secs);
        for (field, value) in packet.fields {
            let name = format!("{}/{field}", packet.group);
            let history = match self.fields.get_mut(&name) {
                Some(history) => history,
                None => {
                    self.order.push(name.clone());
                    self.fields.entry(name).or_default()
                }
            };
            if history.len() >= HISTORY_LEN {
                history.pop_front();
            }
            history.push_back(value as f64);
        }
    }

    /// The field's newest value, 0 where none has arrived.
    pub fn latest(&self, name: &str) -> f64 {
        self.fields.get(name).and_then(|h| h.back()).copied().unwrap_or(0.0)
    }

    /// The field's values, oldest first.
    pub fn history(&self, name: &str) -> Vec<f32> {
        self.fields.get(name).map_or_else(Vec::new, |h| h.iter().map(|&v| v as f32).collect())
    }

    fn last(&self, name: &str, n: usize) -> impl Iterator<Item = f64> + '_ {
        let history = self.fields.get(name);
        let skip = history.map_or(0, |h| h.len().saturating_sub(n));
        history.into_iter().flat_map(move |h| h.iter().skip(skip).copied())
    }

    /// The largest of the field's last `n` values.
    pub fn peak(&self, name: &str, n: usize) -> f64 {
        self.last(name, n).fold(0.0, f64::max)
    }

    /// The sum of the field's last `n` values.
    pub fn sum(&self, name: &str, n: usize) -> f64 {
        self.last(name, n).sum()
    }

    /// How many of the field's last `n` values are over `limit`.
    pub fn over(&self, name: &str, n: usize, limit: f64) -> usize {
        self.last(name, n).filter(|&v| v > limit).count()
    }

    /// Every field whose name starts with `prefix`, with the rest of its
    /// name, in the order first heard.
    pub fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        self.order.iter().filter_map(move |name| name.strip_prefix(prefix).map(|rest| (name.as_str(), rest)))
    }

    /// The publisher's clock at `topic`'s last publication, and the time
    /// since the one before it.
    pub fn clock(&self, topic: &str) -> (f64, f64) {
        self.clocks.get(topic).map_or((0.0, 0.0), |&(before, last)| (last, last - before))
    }

    /// When the source was last heard.
    pub fn heard(&self) -> Option<Instant> {
        self.heard
    }

    pub fn is_live(&self) -> bool {
        self.heard.is_some_and(|t| t.elapsed().as_secs_f64() < LIVE_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::metrics::Cadence;

    fn packet(group: &'static str, at: f64, fields: &[(&'static str, f32)]) -> MetricsPacket {
        MetricsPacket { group: group.into(), cadence: Cadence::Snapshot, timestamp_secs: at, fields: fields.iter().map(|&(n, v)| (n.into(), v)).collect() }
    }

    #[test]
    fn a_field_keeps_its_last_values_in_the_order_heard() {
        let mut feed = Feed::default();
        for i in 0..HISTORY_LEN + 5 {
            feed.hear(packet("server", i as f64 * 2.0, &[("b", i as f32), ("a", 1.0)]));
        }
        assert_eq!(feed.history("server/b").len(), HISTORY_LEN);
        assert_eq!(feed.latest("server/b"), (HISTORY_LEN + 4) as f64);
        assert_eq!(feed.peak("server/b", 3), (HISTORY_LEN + 4) as f64);
        assert_eq!(feed.sum("server/a", 10), 10.0);
        assert_eq!(feed.under("server/").map(|(_, rest)| rest).collect::<Vec<_>>(), vec!["b", "a"]);
        assert_eq!(feed.clock("server"), ((HISTORY_LEN + 4) as f64 * 2.0, 2.0));
    }
}
