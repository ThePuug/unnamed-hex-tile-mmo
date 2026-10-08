//! The client's own metrics as the overlay reads them: received from
//! `common::metrics::CLIENT_GROUP`, the same packets any reader on this
//! machine gets (`publish`), each field kept with its last `HISTORY_LEN`
//! values.

use std::{collections::HashMap, io::ErrorKind, net::UdpSocket};

use bevy::prelude::*;
use common::metrics::{self, MetricsPacket};

/// Values kept of each field: a minute at two publications a second.
pub const HISTORY_LEN: usize = 120;

#[derive(Resource)]
pub struct Feed {
    socket: Option<UdpSocket>,
    /// Each field by `topic/field`, oldest value first.
    fields: HashMap<String, Vec<f32>>,
}

impl Default for Feed {
    fn default() -> Self {
        let socket = metrics::subscriber(metrics::CLIENT_GROUP)
            .and_then(|socket| socket.set_nonblocking(true).map(|()| socket))
            .inspect_err(|e| warn!("metrics: the overlay cannot join {}: {e}", metrics::CLIENT_GROUP))
            .ok();
        Self { socket, fields: HashMap::new() }
    }
}

impl Feed {
    /// The field's newest value, 0 where none has arrived.
    pub fn latest(&self, name: &str) -> f64 {
        self.history(name).last().copied().unwrap_or(0.0) as f64
    }

    /// The field's values, oldest first.
    pub fn history(&self, name: &str) -> &[f32] {
        self.fields.get(name).map_or(&[], Vec::as_slice)
    }

    /// The largest of the field's last `n` values.
    pub fn peak(&self, name: &str, n: usize) -> f64 {
        let history = self.history(name);
        history[history.len().saturating_sub(n)..].iter().copied().fold(0.0, f32::max) as f64
    }

    /// Every field whose name starts with `prefix`, with the rest of its
    /// name.
    pub fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        self.fields.keys().filter_map(move |name| name.strip_prefix(prefix).map(|rest| (name.as_str(), rest)))
    }
}

/// Takes in every packet that has arrived.
pub fn receive(mut feed: ResMut<Feed>) {
    let feed = &mut *feed;
    let Some(socket) = &feed.socket else { return };
    let mut buf = [0u8; 65536];
    loop {
        let n = match socket.recv(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return,
            // Windows reports some send failures on the next receive;
            // what is queued is read next frame.
            Err(_) => return,
        };
        let Some(packet) = MetricsPacket::decode(&buf[..n]) else { continue };
        for (name, value) in packet.fields {
            let history = feed.fields.entry(format!("{}/{name}", packet.group)).or_default();
            if history.len() >= HISTORY_LEN {
                history.remove(0);
            }
            history.push(value);
        }
    }
}
