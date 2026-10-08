//! Metrics on the wire: what a process publishes about itself, and the
//! multicast group it publishes to, so any number of readers can attach on
//! this machine without the publisher knowing about them. Every packet
//! goes to the loopback interface only.
//!
//! A packet is one topic of one publication: its `group` names the topic,
//! and a reader names a field as `topic/field`. A publisher sends every
//! topic it has each time it publishes, whether or not anyone is listening.

use std::{
    borrow::Cow,
    io,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};

pub const METRICS_MAGIC: [u8; 4] = *b"GMSV";
pub const METRICS_VERSION: u16 = 10;

/// The group the client publishes to.
pub const CLIENT_GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 51, 1), 5101);

/// The group the server publishes to.
pub const SERVER_GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 51, 2), 5100);

/// How a snapshot field combines multiple `record()` calls between flushes.
#[derive(Clone, Copy, Debug)]
pub enum Aggregator {
    /// Most recent value wins. Not reset after flush.
    Last,
    /// Maximum of all recorded values. Reset to 0 after flush.
    Peak,
    /// Sum of all recorded values. Reset to 0 after flush.
    /// Console receives per-snapshot deltas, not cumulative totals.
    Sum,
}

/// Packet cadence — tells the console how to handle the data.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Cadence {
    /// Periodic snapshot (every 2s). Console updates gauge displays.
    Snapshot = 0,
    /// Per-event observation. Console accumulates into p95 windows.
    Event = 1,
}

/// Wire format shared between every publisher and every reader.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsPacket {
    pub group: Cow<'static, str>,
    pub cadence: Cadence,
    pub timestamp_secs: f64,
    pub fields: Vec<(Cow<'static, str>, f32)>,
}

impl MetricsPacket {
    /// The packet as sent: magic, version, then the packet.
    pub fn encode(&self) -> Option<Vec<u8>> {
        let bytes = bincode::serde::encode_to_vec(self, bincode::config::legacy()).ok()?;
        let mut buf = Vec::with_capacity(6 + bytes.len());
        buf.extend_from_slice(&METRICS_MAGIC);
        buf.extend_from_slice(&METRICS_VERSION.to_le_bytes());
        buf.extend_from_slice(&bytes);
        Some(buf)
    }

    /// A packet as received, or none where it carries another magic or
    /// version.
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < 6 || buf[..4] != METRICS_MAGIC || buf[4..6] != METRICS_VERSION.to_le_bytes() {
            return None;
        }
        bincode::serde::decode_from_slice(&buf[6..], bincode::config::legacy()).ok().map(|(packet, _)| packet)
    }
}

/// A socket that sends to `group` on the loopback interface. Non-blocking:
/// a publisher never waits on its readers.
pub fn publisher(group: SocketAddrV4) -> io::Result<Publisher> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_multicast_if_v4(&Ipv4Addr::LOCALHOST)?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_multicast_ttl_v4(1)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into())?;
    socket.set_nonblocking(true)?;
    Ok(Publisher { socket: socket.into(), group })
}

/// A socket joined to `group` on the loopback interface. Several share the
/// group's port, and each receives every packet sent to it.
pub fn subscriber(group: SocketAddrV4) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, group.port()).into())?;
    socket.join_multicast_v4(group.ip(), &Ipv4Addr::LOCALHOST)?;
    Ok(socket.into())
}

pub struct Publisher {
    socket: UdpSocket,
    group: SocketAddrV4,
}

impl Publisher {
    /// Sends `packet` to the group. A packet that cannot be sent is dropped.
    pub fn send(&self, packet: &MetricsPacket) {
        if let Some(buf) = packet.encode() {
            let _ = self.socket.send_to(&buf, self.group);
        }
    }
}

/// What this process holds: in RAM now (`working_set`, which the OS may
/// trim), and promised by the OS (`committed`, which is what runs out when
/// an allocation fails). Both bytes, 0 where they cannot be read.
#[derive(Clone, Copy, Debug, Default)]
pub struct Memory {
    pub working_set: u64,
    pub committed: u64,
}

#[cfg(windows)]
pub fn memory() -> Memory {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
    }

    unsafe {
        let mut pmc = std::mem::zeroed::<ProcessMemoryCounters>();
        pmc.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
            Memory { working_set: pmc.WorkingSetSize as u64, committed: pmc.PagefileUsage as u64 }
        } else {
            Memory::default()
        }
    }
}

#[cfg(not(windows))]
pub fn memory() -> Memory {
    Memory::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Two readers joined to a group each receive the one packet a
    /// publisher sends it. A group of its own, so a client running on
    /// this machine does not answer for the publisher under test.
    #[test]
    fn every_reader_of_a_group_receives_each_packet() {
        let group = SocketAddrV4::new(Ipv4Addr::new(239, 255, 51, 250), 5199);
        let readers = [subscriber(group).expect("a reader"), subscriber(group).expect("a second reader")];
        for reader in &readers {
            reader.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        }
        let sent = MetricsPacket { group: Cow::Borrowed("test"), cadence: Cadence::Snapshot, timestamp_secs: 1.5, fields: vec![(Cow::Borrowed("x"), 2.0)] };
        publisher(group).expect("a publisher").send(&sent);
        for reader in &readers {
            let mut buf = [0u8; 2048];
            let n = reader.recv(&mut buf).expect("the packet arrives");
            let got = MetricsPacket::decode(&buf[..n]).expect("it decodes");
            assert_eq!((got.group.as_ref(), got.timestamp_secs, got.fields[0].1), ("test", 1.5, 2.0));
        }
    }

    #[test]
    fn a_packet_of_another_version_is_refused() {
        let packet = MetricsPacket { group: Cow::Borrowed("t"), cadence: Cadence::Event, timestamp_secs: 0.0, fields: Vec::new() };
        let mut buf = packet.encode().unwrap();
        assert!(MetricsPacket::decode(&buf).is_some());
        buf[4] ^= 1;
        assert!(MetricsPacket::decode(&buf).is_none());
    }
}
