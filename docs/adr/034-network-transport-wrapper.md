# ADR-034: Network Transport Wrapper

## Context

The game uses `bevy_renet` directly. Three problems compound as the server scales:

**1. Keepalive spam.** Renet's `update()` and `send_packets()` run every frame in
Update. At high frame rates, this generates ~50KB/s of keepalive traffic for an idle
connection. The receive and send loops need decoupling — receive must stay per-frame
for responsiveness, but sends should be rate-limited.

**2. Buffer overflow vulnerability.** Reliable channels hold unacknowledged messages
until the remote ACKs them. A client that stops ACKing (malicious or dead) causes the
server's send buffer to grow until it hits `max_memory_usage_bytes` and panics. Renet's
design position is that the application should prevent overfilling. The `can_send_message`
API that would enable this was removed. `channel_available_memory` can detect the
condition, but renet provides no built-in disconnect-on-full behavior.

**3. Tight coupling.** Game systems import `RenetServer`/`RenetClient` directly.
Every network callsite must change to replace the transport layer.

## Decision

Wrap renet in a `NetworkPlugin` that owns flow control, stale client detection, and
buffer sizing. Game systems interact with the wrapper, never with renet directly.

### Flow control: queue-then-drain, not per-message checks

Each client has a per-tick byte budget (`budget_per_tick`). Game systems call
`queue_reliable` / `queue_unreliable` — these are `Vec::push`, zero overhead. The
wrapper's send system drains the queue up to budget once per tick, then calls
`send_packets`. Messages that don't fit this tick carry over.

**Why queue-then-drain, not per-message budget checks:** Checking budget at
`send_message` call sites contaminates game logic with transport concerns and creates
pressure to inline budget decisions across many systems. The queue is the natural
boundary — game systems describe intent, the wrapper enforces policy.

**Why only reliable sends are budget-gated:** Unreliable messages carry no ACK
requirement, so they cannot accumulate in the buffer. They are not a vector for
overflow and do not need budget accounting.

**Anti-starvation:** A message larger than `budget_per_tick` would never send under
a strict budget check. The drain system guarantees at least one message sends per
tick regardless of size, preventing permanent starvation of large payloads (e.g.
large chunk messages near the budget limit).

### Health check: disconnect on low buffer memory

A system at `health_check_interval` (suggest 1s) reads `channel_available_memory`
for each client's reliable channels. If available memory falls below
`health_threshold * max_memory_usage_bytes`, the client is disconnected.

**Why disconnect rather than wait for renet's own timeout:** Renet's netcode timeout
fires on receive silence, not on ACK silence. A malicious client can continue sending
(preventing timeout) while withholding ACKs, growing the buffer indefinitely. The
health check is the only defense against this specific failure mode.

**Why no grace period on connect:** Flow control bounds the fill rate to
`budget_per_tick` per tick regardless of queue depth. During initial chunk burst, the
buffer fills at a controlled rate — the health threshold is not reachable through
normal load. The health check only fires when ACKs are not draining the buffer (the
malicious or dead-connection case). Flow control and the health check are
complementary: one bounds fill rate, the other catches drain failure. They do not
need sequencing.

### Buffer sizing: derived from flow control parameters

```
max_memory = budget_per_tick × send_rate × timeout_seconds × safety_margin
```

With 12.5KB budget, 8Hz send rate, 15s timeout, 2× margin → 3MB. This replaces the
current 50MB value. The derivation is a comment in the source; the number is not
guessed.

### Receive/send decoupling

`transport.update()` runs every frame (Update) for minimum receive latency.
`transport.send_packets()` runs on a timer matching the server's FixedUpdate rate
(8Hz / 125ms). Between sends, messages accumulate in the wrapper's outbound queue.
This eliminates keepalive spam without affecting receive responsiveness.

### Abstraction boundary

The wrapper is the sole owner of `RenetServer`/`RenetClient` and the netcode
transports. No game system imports renet types. The public API mirrors renet's
send/receive surface but adds the queue layer. `ServerEvent::ClientConnected` /
`ClientDisconnected` are re-surfaced by the wrapper so connection lifecycle remains
visible to game systems.

## Alternatives Rejected

**Raise `max_memory_usage_bytes` higher and rely on renet timeout alone.** Does not
address the ACK-withholding attack vector. Renet timeout requires receive silence;
a misbehaving client can keep receiving while refusing to ACK. Dismissed.

**Configure renet without wrapping.** Renet's send API is synchronous — there is no
queue layer to rate-limit sends without intercepting call sites. Per-message budget
checks would spread transport policy into game systems. Tight coupling remains.
Dismissed.

**Per-message budget check in the hot path.** Every `send_message` call would need
budget state access. Adds overhead and couples game logic to transport policy per
callsite. The queue-then-drain approach moves policy to a single send system.
Dismissed.

## Consequences

**Positive:**
- Idle bandwidth drops from ~50KB/s to near-zero (send rate limited to 8Hz)
- Buffer overflow from ACK withholding is bounded and detected
- All renet migration is isolated to one plugin; game systems are transport-agnostic
- `max_memory_usage_bytes` shrinks from 50MB to ~3MB with a documented derivation

**Negative:**
- Reliable messages may be delayed up to one send tick (125ms) if the budget is
  exhausted. This is acceptable — reliable messages carry non-time-critical state
  (chunk data, actor spawns). Time-critical state already uses unreliable channels.
- The wrapper must handle the pre-connection window (startup, reconnect) where renet
  is not yet active. `queue_reliable`/`queue_unreliable` silently discard in this
  window.

## Implementation Phases

1. **Passthrough** — wrapper owns renet internally, game systems migrate to wrapper
   API, behavior unchanged
2. **Flow control** — per-tick budget and outbound queue, verify idle bandwidth
3. **Health check + buffer sizing** — stale client disconnect, reduce `max_memory`
4. **Metrics** — expose bytes_sent/received, budget_remaining, buffer_occupancy to
   the metrics overlay

## Date

2026-03-18
