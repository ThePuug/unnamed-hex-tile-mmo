# System Map

Every registered Bevy system across server and client, grouped by domain. This is the navigational chart for understanding what runs, when, and why.

**Counts:** Server 64 systems · Client 113 systems · 177 total

---

## How to Read This Document

**Schedule** — when the system runs:
- `FixedUpdate` (125ms) — deterministic game state. Server authority. Client prediction.
- `Update` (every frame) — rendering, input, network I/O, UI. Variable rate.
- `PreUpdate` / `PostUpdate` — bookend phases for network receive/send.
- `FixedFirst` / `FixedLast` — measurement brackets around the fixed tick.
- `Startup` — runs once.

**Path classification:**
- **HOT** — runs every tick/frame unconditionally, touches shared mutable state. Budget matters.
- **WARM** — runs every tick/frame but read-only or low-cost. Worth watching at scale.
- **COLD** — timer-gated, event-driven, or startup-only. Not a budget concern unless pathological.
- **ASYNC** — dispatches or polls work on `AsyncComputeTaskPool`. Main-thread cost is polling only.

**Ordering notation:** `A → B` means A runs before B (`.after()`/`.before()`). `[chain]` means systems run in strict sequence.

**Scaling notation** — how cost grows with load:
- **O(1)** — constant cost regardless of entity/player count
- **O(N)** — linear in entity count (NPCs, players, chunks)
- **O(N log E)** — linear in active entities, log in total entity count (spatial queries)
- **O(N×M)** — quadratic in player density (broadcast patterns, AOI)
- Systems without annotation are O(1) or bounded by event count.

---

## Schedule Overviews

These are the maps you look at when deciding where a new system goes.

### Server Schedule

```
FixedFirst
│  tick_timer_start                                    [Metrics]
│
FixedUpdate
│  ┌─ regenerate_resources ─── update_combat_state     [Combat State]
│  │   ─── global_recovery_system
│  │   ─── synergy_cleanup_system
│  │   ─── process_expired_threats
│  │
│  ├─ controlled::apply ─── controlled::tick           [Physics]
│  │
│  ├─ tick_stagger                                     [NPC Behaviour]
│  │   ├─ assign_hexes
│  │   ├─ chase ····························O(N log E)
│  │   ├─ kite
│  │   ├─ process_knockback ·········(after tick_stagger)
│  │   └─ enforce_stagger ···········(after chase, kite, process_knockback)
│  │
│
FixedPostUpdate
│  broadcast_player_movement_intent                    [Physics]
│  broadcast_heading_changes                           [Physics]
│
FixedLast
│  tick_timer_end                                      [Metrics]
│
PreUpdate
│  write_try                                           [Network I/O]
│
Update  ⚠ UNCAPPED — server uses MinimalPlugins, no frame rate limit
│  ┌─ [Combat Resolution] validate_prerequisites,
│  │   7 ability handlers, process_dismiss, check_death
│  │
│  ├─ [Targeting & AI] update_targets ·····O(N log E)
│  │   process_passive_auto_attack (500ms),
│  │   npc_ability_usage (500ms)
│  │
│  ├─ [Input] try_input, send_input, try_set_tier_lock,
│  │   try_respec_attributes, sync_queue_window_size
│  │
│  ├─ [World Events] try_incremental, do_incremental,
│  │   do_spawn, try_spawn
│  │
│  ├─ [World Streaming] do_spawn_discover,
│  │   try_discover_chunk, poll_chunk_tasks,
│  │   dispatch_summary_tasks, poll_summary_tasks
│  │
│  ├─ [Engagement] activate_spawners, process_respawn,
│  │   update_engagement_proximity (1s),
│  │   cleanup_engagements (5s)
│  │
│  ├─ [Spatial Index] nntree::update
│  │
│  ├─ [Actor] do_incremental, update
│  │
│  └─ [Metrics] track_frame_time, flush systems
│
PostUpdate
│  update_area_of_interest ············O(N×M)          [Network I/O]
│  → send_do ··························O(events×M)     [Network I/O]
│  → cleanup_despawned                                 [Network I/O]
```

### Client Schedule

```
PreUpdate
│  input::update_keybits                               [Input]
│  renet::write_do ·············⚠ UNBOUNDED            [Network I/O]
│
FixedUpdate
│  controlled::apply ─── controlled::tick              [Physics]
│  → input::do_input ···········(after tick)           [Input]
│  regenerate_resources                                [Combat State]
│
FixedPostUpdate
│  predict_local_player                                [Prediction]
│
Update
│  ┌─ [Actor] do_spawn, apply_movement_intent,
│  │   try_gcd, advance_interpolation → update,
│  │   dead_visibility, cleanup_dead, animator
│  │
│  ├─ [Combat Events] handle_insert_threat,
│  │   handle_apply_damage, handle_clear_queue,
│  │   handle_ability_failed, player_auto_attack (500ms),
│  │   apply_gcd
│  │
│  ├─ [Combat State] handle_ability_used,
│  │   global_recovery, synergy_cleanup,
│  │   sync_queue_window_size
│  │
│  ├─ [Attack Telegraphs] on_insert_threat,
│  │   on_apply_damage → on_clear_queue,
│  │   update_telegraphs
│  │
│  ├─ [World State] do_init, try_incremental,
│  │   do_incremental ···(after apply_movement_intent),
│  │   update
│  │
│  ├─ [World Streaming] do_spawn → dispatch_summary
│  │   → poll_summary_meshes, evict_data
│  │
│  ├─ [Targeting] update_targets, update_ally_targets
│  │
│  ├─ [Camera] camera::update
│  │
│  ├─ [HUD] ui, compass, resource_bars, action_bar
│  │
│  ├─ [Target Frame] 6 systems
│  │
│  ├─ [Combat Feedback] 15 systems
│  │
│  ├─ [Character Panel] 9 systems ·····⚠ no run_if guard
│  │
│  ├─ [Dev Console] 4 systems [chain] ·⚠ no run_if guard
│  │
│  ├─ [Diagnostics] 5 systems ········⚠ no run_if guard
│  │
│  ├─ [Audio] (placeholder — not yet implemented)
│  │
│  ├─ [Spatial Index] nntree::update
│  │
│  └─ [Vignette] update_vignette_intensity
│
PostUpdate
│  renet::send_try                                     [Network I/O]
```

---

## Shared State — The Contention Map

These resources are touched by multiple domains. Every lock acquisition, every mutable borrow, every query filter here is a potential bottleneck.

| Resource | Type | Domains Reading | Domains Writing | Concern |
|----------|------|-----------------|-----------------|---------|
| **Map** | `Arc<RwLock<qrz::Map>>` | Physics, Combat, Behaviour, AOI, World Streaming, Targeting, Metrics | World Events (insert) | **Global RwLock.** Every physics tick reads 3+ times per entity. Chunk writes hold write lock. |
| **NNTree** | `RTree` (rstar) | AOI, Targeting, Combat, Behaviour, Engagement | Component hooks (on Loc change) | Rebuilt on every `Changed<Loc>`. Query cost scales with entity density. |
| **InputQueues** | `HashMap<Entity, InputQueue>` | Physics, Input, Prediction | Input, Network I/O | INV-002: must always have ≥1 entry. Shared between input capture and physics. |
| **Lobby** | `BiMap<ClientId, Entity>` | Network I/O, Input, AOI | Network I/O (connect/disconnect) | Server only. Lookup on every message receive. |
| **MetricSnapshot** | `Mutex<Vec<SnapshotField>>` | — | All instrumented systems | **See warning below.** |

### MetricSnapshot Mutex — Scaling Time Bomb

Every instrumented system calls `snapshot.record()`, which acquires `std::sync::Mutex<Vec<SnapshotField>>`. Today with ~10 callers per tick, contention is noise. But this is the kind of thing that silently degrades as instrumentation grows:

- Adding per-system tracing (recommendation #3 below) increases write pressure on this mutex proportionally.
- At 64+ systems recording metrics, the mutex becomes a serialization point in the Update schedule — systems that could otherwise parallelize will contend on this lock.
- **Tracy integration sidesteps this entirely.** Tracy spans go to the Tracy collector via lock-free ring buffers, not through the MetricSnapshot path. The two instrumentation strategies have fundamentally different contention profiles.

**Implication:** Use Tracy (`--features trace`) for per-system profiling. Keep MetricSnapshot for the UDP metrics console (coarse server gauges only). Don't route per-system timing through MetricSnapshot — that's the path to making the measurement system the bottleneck.

### Implicit Synchronization: Commands

`Commands` isn't a lock, but it's a synchronization barrier. Every system that spawns or despawns entities (do_spawn, activate_spawners, process_respawn, check_death, cleanup_dead_entities) writes to Bevy's command buffer, which flushes at `apply_deferred` sync points between system groups. These flushes are the invisible walls in your schedule — systems on opposite sides of a flush cannot parallelize across it. If you ever hit unexplained stalls between Update systems with no obvious data dependency, command buffer flushes are the ghost you're looking for.

### Key Components (cross-domain)

| Component | Written By | Read By |
|-----------|-----------|---------|
| **Loc** | Input (controlled::apply), Behaviour (chase/kite), Stagger (knockback) | AOI, Targeting, NNTree, all Combat, Engagement, Prediction, World Streaming |
| **Position** | controlled::apply | Prediction |
| **VisualPosition** | Prediction, MovementIntent | Actor Rendering, Animator |
| **Health/Stamina/Mana** | Combat Resolution (damage), Regen | Combat, Targeting, AOI, UI |
| **ReactionQueue** | Combat (insert threat), Reaction Queue (expire/dismiss) | Combat, NPC AI, Client Combat UI |
| **Target** | Targeting | All Combat, Behaviour, NPC AI |
| **LoadedBy** | AOI | Network I/O (send_do) |
| **KeyBits** | Input | Physics, Prediction |

---

# SERVER

## S1. Network I/O

Receives client messages, dispatches server broadcasts, manages connections.

| System | Schedule | Path | Scaling | Notes |
|--------|----------|------|---------|-------|
| `renet::write_try` | PreUpdate | HOT | O(M) | Deserialize all client messages into Try events. M = connected players. |
| `aoi::update_area_of_interest` | PostUpdate | HOT | **O(N×M)** | For each moved entity (N), scan observers (M) to update LoadedBy. Quadratic in player density. |
| `renet::send_do` | PostUpdate | HOT | **O(events×M)** | Clone encoded bytes per observer. Encode-once pattern (2026-04-08). |
| `renet::cleanup_despawned` | PostUpdate | WARM | O(D) | D = despawned entities this frame. |
| `renet::do_manage_connections` | Observer | COLD | O(1) | On connect/disconnect: spawn/despawn player entity |

**Ordering:** `update_area_of_interest` → `send_do` → `cleanup_despawned`

**Scaling note:** `send_do` and `update_area_of_interest` are the two systems most sensitive to player density. At 10 players with 20 state changes/tick: 200 byte-copies. At 50 players: 1,000. At 200 players in mutual visibility: 4,000. These are the systems that will blindside you going from 10 to 50 players — the cost is quadratic in the thing you're trying to grow.

---

## S2. Physics & Movement

Deterministic movement processing. Shared between client and server via `ControlledPlugin`.

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `controlled::apply` | FixedUpdate | HOT | `common-bevy/systems/behaviour/controlled.rs` |
| `controlled::tick` | FixedUpdate | HOT | `common-bevy/systems/behaviour/controlled.rs` |
| `input::broadcast_player_movement_intent` | FixedPostUpdate | HOT | `server/systems/input.rs` |
| `actor::broadcast_heading_changes` | FixedPostUpdate | HOT | `server/systems/actor.rs` |

**Data flow:** `KeyBits` → `controlled::tick` (accumulates dt) → `controlled::apply` (writes `Position.offset` via `movement::calculate_movement()`) → broadcast intent/heading changes after physics settles.

**Reads:** Map (elevation, blocking), KeyBits, Position, InputQueues
**Writes:** Position.offset, Loc (on tile crossing), Heading

**Scaling:** O(N) in total entities (players + NPCs). `calculate_movement()` calls `get_by_qr()` 3+ times per entity per tick. 200 entities = 600+ Map reads per tick. Each read acquires the RwLock read guard. Currently no idle filtering — stationary entities still pay full cost. Linear, but with a high constant factor per entity.

---

## S3. NPC Behaviour

Server-only AI decision-making. Registered via `BehaviourPlugin`.

| System | Schedule | Path | Ordering |
|--------|----------|------|----------|
| `stagger::tick_stagger` | FixedUpdate | HOT | — |
| `hex_assignment::assign_hexes` | FixedUpdate | HOT | — |
| `chase::chase` | FixedUpdate | HOT | — |
| `kite::kite` | FixedUpdate | HOT | — |
| `stagger::process_knockback` | FixedUpdate | HOT | after `tick_stagger` |
| `stagger::enforce_stagger` | FixedUpdate | HOT | after `chase`, `kite`, `process_knockback` |

**Data flow:** Target → chase/kite decision → Loc/Heading writes → enforce_stagger freezes staggered entities

**Reads:** Target, Loc, Heading, HexAssignment, Engagement, Map, NNTree, Stagger, Knockback
**Writes:** Loc, Heading, Position.offset (reset by enforce_stagger), MovementIntentState

**Scaling:** O(N log E) where N = active NPCs, E = total entities in NNTree. `chase` does up to 6 NNTree queries per NPC per tick (neighbor evaluation). Each RTree query is O(log E). With 200 NPCs and 250 total entities: 1,200 × log(250) ≈ 9,600 comparisons per tick. All 200 NPCs are evaluated even if ~180 are idle — no `Changed` filter. Per-NPC Vec allocations for spatial query results. The `enforce_stagger` after-constraint ensures staggered NPCs don't move even if chase/kite wrote to them.

**Recommendation:** Filter chase/kite queries with `With<EngagementMember>` (or equivalent) to skip disengaged NPCs. Going from O(200) to O(20) active NPCs changes the character of the system entirely — probably the single biggest free win on the server side once NPC counts grow.

---

## S4. Combat State

Tick-driven resource regeneration and combat state bookkeeping. Shared via `common-bevy`.

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `regenerate_resources` | FixedUpdate | HOT | `common-bevy/systems/combat/resources.rs` |
| `update_combat_state` | FixedUpdate | HOT | `common-bevy/systems/combat/state.rs` |
| `global_recovery_system` | FixedUpdate | HOT | `common-bevy/systems/combat/recovery.rs` |
| `synergy_cleanup_system` | FixedUpdate | WARM | `common-bevy/systems/combat/synergies.rs` |
| `process_expired_threats` | FixedUpdate | HOT | `server/systems/reaction_queue.rs` |

**Data flow:** Resources regen → combat state checks (5s out-of-combat exit) → recovery lockout ticks down → expired synergies cleaned → expired threats trigger ResolveThreat

**Reads:** Health, Stamina, Mana, CombatState, GlobalRecovery, ReactionQueue, ActorAttributes, NNTree (combat state checks hostiles within 20 hexes)
**Writes:** Health, Stamina, Mana, CombatState, GlobalRecovery, ReactionQueue

---

## S5. Combat Resolution

Event-driven damage pipeline. Mix of Update systems and Observers.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `combat::validate_ability_prerequisites` | Update | WARM | Check stamina, mana, GCD, recovery before ability fires |
| `abilities::handle_auto_attack` | Update | WARM | |
| `abilities::handle_overpower` | Update | WARM | |
| `abilities::handle_lunge` | Update | WARM | |
| `abilities::handle_counter` | Update | WARM | ADR-014 |
| `abilities::handle_kick` | Update | WARM | Knockback |
| `abilities::handle_deflect` | Update | WARM | |
| `abilities::handle_volley` | Update | WARM | Ranged multi-target |
| `reaction_queue::process_dismiss` | Update | WARM | ADR-022: pop front threat |
| `combat::check_death` | Update | WARM | Emit Despawn, add RespawnTimer |
| `combat::process_deal_damage` | **Observer** | WARM | Phase 1: crit roll, evasion, insert threat into queue |
| `combat::resolve_threat` | **Observer** | WARM | Phase 2: armor/resistance, apply damage |

**Data flow:** UseAbility event → validate_prerequisites → ability handler → DealDamage trigger → process_deal_damage (observer) → InsertThreat → [threat window ~1.5s] → process_expired_threats (S4) → ResolveThreat trigger → resolve_threat (observer) → ApplyDamage

**Budget note:** Event-driven, so cost scales with combat activity, not entity count. Observers run synchronously when triggered. The 7 ability handlers all run every frame but early-exit via EventReader — effectively free when no abilities are being used.

---

## S6. Targeting & NPC AI

Target acquisition and NPC ability usage.

| System | Schedule | Path | Run Condition |
|--------|----------|------|---------------|
| `targeting::update_targets` | Update | HOT | every frame |
| `combat::process_passive_auto_attack` | Update | COLD | `on_timer(500ms)` |
| `npc_ability_usage::npc_ability_usage` | Update | COLD | `on_timer(500ms)` |

**Reads:** Loc, Heading, NNTree, Target, ActorAttributes, Health, Stamina, Mana, Behaviour, AttackRange
**Writes:** Target, UseAbility events

**Scaling:** O(N log E) — `update_targets` queries NNTree for each entity with a Target component.

**Frequency note:** `update_targets` runs every frame. Throttling to 10Hz is safe on the **client** (tab-target selection doesn't need 60Hz). On the **server**, throttling is dangerous: ability handlers in Update read the cached `Target` component. If a player fires an ability at frame N and targeting last ran 100ms ago, the target state could be stale enough to cause a validation mismatch (target moved out of range, or died). Either keep server targeting at tick rate, or refactor ability validation to resolve targeting inline rather than reading a cached component. The timer-gated NPC systems are fine at 2Hz.

---

## S7. Input Processing

Client input relay and queue management.

| System | Schedule | Path |
|--------|----------|------|
| `input::try_input` | Update | HOT |
| `input::send_input` | Update | HOT |
| `input::try_set_tier_lock` | Update | WARM |
| `input::try_respec_attributes` | Update | COLD |
| `queue::sync_queue_window_size` | Update | WARM |

**Data flow:** `write_try` (S1) deserializes → `try_input` relays Try::Input → Do::Input → `send_input` drains queues to clients

**Reads:** InputQueues, Lobby, Map, Target, TierLock, ActorAttributes
**Writes:** InputQueues, Do events, TierLock

---

## S8. World Streaming

Async chunk generation and summary computation. Registered via `WorldStreamingPlugin`.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `world::setup` | Startup | COLD | Initialize RunTime |
| `actor::do_spawn_discover` | Update | WARM | Discover initial chunks on player spawn |
| `actor::try_discover_chunk` | Update | ASYNC | Dispatch async chunk generation tasks |
| `actor::poll_chunk_tasks` | Update | ASYNC | Poll in-flight chunk tasks, send results |
| `summary::dispatch_summary_tasks` | Update | ASYNC | Dispatch async summary computation |
| `summary::poll_summary_tasks` | Update | ASYNC | Poll summary tasks, send results |

**Resources:** `WorldDiscoveryCache`, `ChunkTaskQueue`, `SummaryTaskQueue`, `SummaryCache`

**Reads:** Loc (player positions for discovery radius), Map, Terrain, EventRegistry
**Writes:** Map (chunk inserts — **acquires write lock**), Do events (ChunkData, SummaryBatch, EvictChunks)

**Scaling:** O(M) in moving players for chunk discovery. O(1) per poll (bounded by async task completion rate). But `poll_chunk_tasks` inserts 272 tiles per chunk into Map, acquiring the **write lock**. During chunk streaming (player moving through new territory), this blocks all physics reads. Multiple players moving simultaneously multiply the write-lock contention. This is the primary Map contention scenario.

---

## S9. World Events

Spawn/despawn entity processing and incremental state updates.

| System | Schedule | Path |
|--------|----------|------|
| `world::try_spawn` | Update | WARM |
| `world::do_spawn` | Update | WARM |
| `world::try_incremental` | Update | HOT |
| `world::do_incremental` | Update | HOT |

**Data flow:** Try::Spawn → world::try_spawn (chunk discovery trigger) → Do::Spawn. Try::Incremental → passthrough → Do::Incremental (component updates: Loc, Heading, Health, etc.)

---

## S10. Engagement Management

NPC spawner activation and engagement lifecycle.

| System | Schedule | Path | Run Condition |
|--------|----------|------|---------------|
| `engagement_spawner::activate_spawners` | Update | WARM | every frame |
| `combat::resources::process_respawn` | Update | WARM | every frame |
| `engagement_cleanup::update_engagement_proximity` | Update | COLD | `on_timer(1s)` |
| `engagement_cleanup::cleanup_engagements` | Update | COLD | `on_timer(5s)` |

**Reads:** Loc (Changed, PlayerControlled), EventRegistry (spawner index), ActiveSpawners, Engagement, EngagementMember, LastPlayerProximity
**Writes:** ActiveSpawners, Commands (spawn/despawn NPCs), Health (respawn restore)

**Budget note:** `activate_spawners` checks `Changed<Loc>` on player entities — bounded by player count. Timer-gated cleanup is fine.

---

## S11. Spatial Index

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `nntree::update` | Update | HOT | `common-bevy/plugins/nntree.rs` |

**Data flow:** `Changed<Loc>` → remove old entry → insert new entry in RTree

**Budget note:** Also maintained via component hooks (`on_add`, `on_remove`). The Update system handles `Changed<Loc>` for moves. Cost = O(changed entities × log N) for RTree operations. Well-filtered.

---

## S12. Metrics

Server performance telemetry. Registered via `MetricsPlugin`.

| System | Schedule | Path | Run Condition |
|--------|----------|------|---------------|
| `tick_timer_start` | FixedFirst | WARM | — |
| `tick_timer_end` | FixedLast | WARM | — |
| `track_frame_time` | Update | WARM | — |
| `maybe_flush_snapshot` | Update | COLD | internal 2s check |
| `maybe_flush_timings` | Update | COLD | internal 2s check |
| `refresh_metric_gauges` | Update | COLD | `run_if(flush_due)` |
| `drain_event_metrics` | Update | COLD | `run_if(flush_due)` |

**Budget note:** `MetricSnapshot` uses a `Mutex<Vec<SnapshotField>>` — every `record()` call acquires this lock. Low contention today (few callers), but will scale linearly with instrumented systems. The tick timer pair (FixedFirst/FixedLast) correctly brackets the entire fixed tick for measurement.

---

---

# CLIENT

## C1. Network I/O

Receive server messages, send client requests, latency measurement.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `renet::write_do` | PreUpdate | HOT | Deserialize all server messages, map entities, forward to game systems |
| `renet::send_try` | PostUpdate | HOT | Send queued Try events to server |
| `renet::handle_pong` | Update | COLD | Measure network latency |
| `renet::periodic_ping` | Update | COLD | Send ping every 5s |

**Budget note:** `write_do` processes all buffered messages in a single frame — **unbounded**. During initial chunk load + entity spawns, this can spike. Flagged in audit (Tier 1 item 3, deferred).

---

## C2. Input

Key capture and input queue management.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `input::update_keybits` | PreUpdate | HOT | Capture keyboard → KeyBits. Admin: `run_if(not_in_flyover)` |
| `input::do_input` | FixedUpdate | HOT | `.after(controlled::tick)` — dequeue confirmed inputs |

**Data flow:** Keyboard → `update_keybits` → KeyBits → `controlled::tick` (accumulates dt) → `do_input` (dequeues by server seq)

---

## C3. Physics & Prediction

Client-side movement prediction. Shared plugin + client-specific prediction.

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `controlled::apply` | FixedUpdate | HOT | `common-bevy` (ControlledPlugin) |
| `controlled::tick` | FixedUpdate | HOT | `common-bevy` (ControlledPlugin) |
| `prediction::predict_local_player` | FixedPostUpdate | HOT | `client/systems/prediction.rs` |
| `prediction::advance_interpolation` | Update | HOT | `.before(actor::update)` |

**Data flow:** `controlled::apply` writes confirmed `Position.offset` → `predict_local_player` replays InputQueue forward → writes `VisualPosition` → `advance_interpolation` smooths toward target each frame

**Reads:** InputQueue, Position, Map, KeyBits, Heading
**Writes:** Position.offset (apply), VisualPosition, Heading (prediction)

---

## C4. Actor Rendering

Entity lifecycle and visual transform updates.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `actor::do_spawn` | Update | WARM | Spawn entities from Do::Spawn events |
| `actor::apply_movement_intent` | Update | HOT | ADR-011: Apply MovementIntent for remote entities |
| `actor::try_gcd` | Update | WARM | |
| `actor::update` | Update | HOT | Write Transform from VisualPosition + Loc |
| `actor_dead_visibility::update_dead_visibility` | Update | WARM | |
| `actor_dead_visibility::cleanup_dead_entities` | Update | WARM | |
| `animator::update` | Update | HOT | Drive AnimationPlayer from VisualPosition state |

**Ordering:** `advance_interpolation` (C3) → `actor::update`

**Reads:** Loc, Heading, VisualPosition, AirTime, Map
**Writes:** Transform, AnimationPlayer, VisualPosition (movement intent)

---

## C5. Combat Events

Process server combat messages on the client.

| System | Schedule | Path |
|--------|----------|------|
| `combat::handle_insert_threat` | Update | WARM |
| `combat::handle_apply_damage` | Update | WARM |
| `combat::handle_clear_queue` | Update | WARM |
| `combat::handle_ability_failed` | Update | WARM |
| `combat::player_auto_attack` | Update | COLD (`on_timer(500ms)`) |
| `combat::apply_gcd` | Update | WARM |

**Data flow:** Do events → update local ReactionQueue, Health, Stamina, Mana, spawn floating damage text

---

## C6. Combat State (Client)

Client-side mirrors of server combat state systems.

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `regenerate_resources` | FixedUpdate | HOT | `common-bevy` |
| `ability_prediction::handle_ability_used` | Update | WARM | `client/systems/ability_prediction.rs` |
| `global_recovery_system` | Update | WARM | `common-bevy` |
| `synergy_cleanup_system` | Update | WARM | `common-bevy` |
| `sync_queue_window_size` | Update | WARM | `common-bevy` |

---

## C7. Targeting

| System | Schedule | Path |
|--------|----------|------|
| `targeting::update_targets` | Update | HOT |
| `targeting::update_ally_targets` | Update | HOT |

**Budget note:** Both run every frame, querying all entities. Same concern as server S6 — 10Hz would suffice.

---

## C8. World Streaming

Client-side chunk and summary mesh management. Registered via `WorldStreamingPlugin`.

| System | Schedule | Path | Ordering |
|--------|----------|------|----------|
| `world::do_spawn` | Update | WARM | — |
| `world::dispatch_summary_tasks` | Update | ASYNC | after `do_spawn` |
| `world::poll_summary_meshes` | Update | ASYNC | after `dispatch_summary_tasks` |
| `world::evict_data` | Update | WARM | Admin: `run_if(not_in_flyover)` |

**Ordering:** `do_spawn` → `dispatch_summary_tasks` → `poll_summary_meshes`

**Reads:** Map, SummaryCache, LoadedChunks, Loc
**Writes:** Map (tile inserts), LoadedChunks, SummaryMeshes, mesh assets

---

## C9. World State

Time sync, lighting, and incremental state relay.

| System | Schedule | Path |
|--------|----------|------|
| `world::do_init` | Update | COLD |
| `world::update` | Update | WARM |
| `world::try_incremental` | Update | HOT |
| `world::do_incremental` | Update | HOT |

**Ordering:** `do_incremental` runs `.after(actor::apply_movement_intent)` — ensures MovementPrediction exists when Loc updates arrive.

---

## C10. Camera

| System | Schedule | Path | Run Condition |
|--------|----------|------|---------------|
| `camera::update` | Update | HOT | Admin: `run_if(not_in_flyover)` |
| `admin::flyover_camera_update` | Update | COLD | `run_if(flyover_active)` |

---

## C11. Attack Telegraphs

Visual feedback for incoming threats.

| System | Schedule | Path | Ordering |
|--------|----------|------|----------|
| `attack_telegraph::on_insert_threat` | Update | WARM | — |
| `attack_telegraph::on_apply_damage` | Update | WARM | `.before(on_clear_queue)` |
| `attack_telegraph::on_clear_queue` | Update | WARM | — |
| `attack_telegraph::update_telegraphs` | Update | WARM | — |

**Ordering:** `on_apply_damage` → `on_clear_queue` (must spawn damage line before clearing threat ball)

---

## C12. HUD

Core UI elements. Registered via `UiPlugin`.

| System | Schedule | Path |
|--------|----------|------|
| `ui::update` | Update | WARM |
| `ui::update_compass` | Update | WARM |
| `resource_bars::update` | Update | WARM |
| `action_bar::update` | Update | WARM |

**Budget note:** Text caching implemented (2026-04-09). Resource bars only `format!()` on value change. UI time text caches minute-tick.

---

## C13. Target Frame

Target and ally UI panels.

| System | Schedule | Path |
|--------|----------|------|
| `target_frame::update` | Update | WARM |
| `target_frame::update_ally_frame` | Update | WARM |
| `target_frame::update_queue` | Update | WARM |
| `target_frame::update_ally_queue` | Update | WARM |
| `target_indicator::update` | Update | WARM |
| `tier_lock_range_indicator::update` | Update | WARM |

---

## C14. Combat Feedback

Floating numbers, health bars, threat visualization, combat log. Registered via `UiPlugin`.

| System | Schedule | Path |
|--------|----------|------|
| `combat_ui::update_floating_text` | Update | WARM |
| `combat_ui::update_health_bars` | Update | WARM |
| `combat_ui::update_recovery_bars` | Update | WARM |
| `combat_ui::update_threat_queue_dots` | Update | WARM |
| `threat_icons::update` | Update | WARM |
| `threat_icons::spawn_pop_animation` | Update | WARM |
| `threat_icons::update_popping_icons` | Update | WARM |
| `resolved_threats::on_damage_resolved` | Update | WARM |
| `resolved_threats::update_entries` | Update | WARM |
| `resolved_threats::sync_container_position` | Update | WARM |
| `combat_log::on_damage_applied` | Update | WARM |
| `combat_log::on_queue_cleared` | Update | WARM |
| `combat_log::maintain_log` | Update | WARM |
| `combat_log::handle_scroll` | Update | WARM |
| `combat_log::auto_scroll_to_bottom` | Update | WARM |

**15 systems.** All run every frame. Most are event-driven (early exit when no events). The sheer count means Bevy's scheduler overhead is non-trivial. Three clusters are consolidation targets:

- **combat_log** (5 → 1): `on_damage_applied`, `on_queue_cleared`, `maintain_log`, `handle_scroll`, `auto_scroll_to_bottom` all operate on the same egui panel and the same `CombatLog` resource. One system.
- **threat_icons** (3 → 1): `update`, `spawn_pop_animation`, `update_popping_icons` are phases of the same visual update. One system.
- **resolved_threats** (3 → 1): `on_damage_resolved`, `update_entries`, `sync_container_position` — same pattern.

That's 15 → 8 without changing behavior — just fewer scheduler nodes.

---

## C15. Character Panel

Attribute viewing, respec, and UI interaction. Registered via `UiPlugin`.

| System | Schedule | Path |
|--------|----------|------|
| `character_panel::toggle_panel` | Update | COLD |
| `character_panel::handle_shift_drag` | Update | COLD |
| `character_panel::update_attributes` | Update | COLD |
| `character_panel::update_axis_button_visibility` | Update | COLD |
| `character_panel::update_apply_button` | Update | COLD |
| `character_panel_respec::handle_attribute_buttons` | Update | COLD |
| `character_panel_respec::handle_apply_button` | Update | COLD |
| `character_panel_respec::handle_respec_confirmed` | Update | COLD |
| `character_panel_respec::toggle_apply_button` | Update | COLD |

**9 systems.** All effectively cold — early exit when panel is closed. Could be gated behind `run_if(panel_open)` to avoid query overhead.

---

## C16. Dev Console

Hierarchical numpad menu. Registered via `DevConsolePlugin`.

| System | Schedule | Path | Notes |
|--------|----------|------|-------|
| `navigation::handle_console_input` | Update | COLD | [chain] |
| `ui_simple::update_console_visibility` | Update | COLD | [chain] |
| `ui_simple::update_console_menu` | Update | COLD | [chain] |
| `actions::execute_console_actions` | Update | COLD | [chain] |

**Chained** — runs in strict sequence. Negligible cost.

---

## C17. Diagnostics

Debug overlays. Registered via `DiagnosticsPlugin`.

| System | Schedule | Path |
|--------|----------|------|
| `grid::spawn_grid_mesh_task` | Update | COLD |
| `grid::poll_grid_mesh_task` | Update | COLD |
| `network_ui::update_network_metrics` | Update | WARM |
| `metrics_overlay::sample_metrics` | Update | WARM |
| `metrics_overlay::update_metrics_overlay` | Update | WARM |

---

## C18. Admin (feature-gated)

Developer flyover and debug tools. Only compiled with `--features admin`.

| System | Schedule | Path | Run Condition |
|--------|----------|------|---------------|
| `admin::execute_admin_actions` | Update | COLD | — |
| `admin::flyover_movement` | Update | COLD | `flyover_active` |
| `admin::tag_admin_chunks` | Update | COLD | — |
| `admin::poll_flyover_tile_tasks` | Update | COLD | `flyover_active` |
| `admin::flyover_generate_chunks` | Update | COLD | `flyover_active` + `on_timer(200ms)` |
| `admin::flyover_evict_chunks` | Update | COLD | `flyover_active` + `on_timer(1s)` |

---

## C19. Other

| System | Schedule | Path | Source |
|--------|----------|------|--------|
| `nntree::update` | Update | HOT | `common-bevy/plugins/nntree.rs` |
| `update_vignette_intensity` | Update | WARM | `client/plugins/vignette.rs` |

---

# Cross-Cutting Concerns

## 1. The Update Schedule is Overloaded

**Client:** 89 of 113 systems run in Update. Bevy parallelizes compatible systems, but systems sharing mutable access to the same components or resources serialize. The combat feedback block alone (15 systems) likely shares `Query<&ReactionQueue>` reads, so they can parallelize — but the scheduler must prove this at runtime.

**Action:** Group systems that share no data into explicit parallel sets. Gate inactive UI (character panel, dev console) behind `run_if` conditions.

## 2. No Explicit System Sets

Neither server nor client uses Bevy's `SystemSet` for domain grouping. All ordering is point-to-point (`.after(specific_system)`). This makes it hard to:
- Add a new system "after all combat" (there's no "combat" set)
- Reason about inter-domain ordering
- Profile at the domain level rather than per-system

**Action:** Define `SystemSet` enums for each domain group. Replace point-to-point ordering with set-level ordering where possible.

## 3. FixedUpdate vs Update Boundary — Combat Schedule Split

Server combat state (S4) runs in FixedUpdate (125ms). Combat resolution (S5) runs in Update. This is intentional — event-driven abilities shouldn't wait for the next tick — but has deeper implications than the obvious.

**The server runs Update uncapped** (`MinimalPlugins` with no `ScheduleRunnerPlugin` rate limit). Under light load, Update runs hundreds of times per 125ms fixed tick. Under heavy load, the ratio drops. This means:

- Ability handlers execute a **variable** number of times per fixed tick, proportional to server frame rate.
- Between fixed ticks, the ReactionQueue can accumulate multiple threats that all expire in the same tick.
- Combat responsiveness is technically frame-rate-dependent: a server under CPU pressure processes fewer ability events between state ticks.
- In practice this may not matter — a headless server should always have cycles to spare for Update, and abilities are gated by GCD/recovery/EventReader anyway. But it's a coupling that should be documented.

**The invariant to maintain:** ability validation (S5) reads `Target`, `Health`, `Stamina` etc. These are written by FixedUpdate systems (S4). Between fixed ticks, Update systems see the state from the *last* completed FixedUpdate. An ability that fires mid-tick sees regenerated resources from the previous tick, not the current one. This is standard Bevy behavior but means combat math has up to 125ms of staleness in resource values.

Document this boundary in GUIDANCE.md anti-patterns.

## 4. Missing `run_if` Guards — 18 Systems Running Unconditionally

| System Group | Count | Current | Should Be | Savings |
|-------------|-------|---------|-----------|---------|
| Character panel | 9 | Always runs | `run_if(panel_open)` | 9 systems skipped when panel closed (99%+ of play time) |
| Dev console | 4 | Always runs (chained) | `run_if(console_open)` | 4 systems skipped when console closed |
| Diagnostics | 5 | Always runs | `run_if(diagnostics_visible)` | 5 systems skipped when overlays off |
| Client targeting | 2 | Every frame | `on_timer(100ms)` | 90% reduction in NNTree queries |
| Combat feedback | 15 | Always runs | Acceptable | Most early-exit via EventReader |
| Server targeting | 1 | Every frame | **Keep at tick rate** (see S6 note) | — |

## 5. Ordering Constraints Summary

**Server critical chains:**
- `tick_stagger` → `process_knockback`
- `chase`, `kite`, `process_knockback` → `enforce_stagger`
- `update_area_of_interest` → `send_do` → `cleanup_despawned`

**Client critical chains:**
- `controlled::tick` → `do_input`
- `advance_interpolation` → `actor::update`
- `apply_movement_intent` → `do_incremental`
- `on_apply_damage` → `on_clear_queue`
- `do_spawn` → `dispatch_summary_tasks` → `poll_summary_meshes`

---

# Staff Engineer Assessment

## What's Working

1. **Hot/cold separation is real.** Async mesh pipeline, timer-gated NPC AI, event-driven combat resolution. The architecture distinguishes expensive work from cheap bookkeeping.
2. **Network pipeline is well-ordered.** PreUpdate receive → game logic → PostUpdate send. No mid-frame network I/O.
3. **Shared combat systems** (`common-bevy`) avoid duplication between client and server. Correct pattern.
4. **Observer pattern for combat** keeps ability handlers out of the per-frame budget when no abilities are active.

## What Needs Work

1. **No SystemSets.** Point-to-point ordering doesn't scale. Adding system #178 means understanding all existing orderings to find the right slot. Sets give you "after Combat" instead of "after handle_kick and after handle_volley and after..."
2. **Client Update is a flat bag of 89 systems.** No domain structure visible to the scheduler. The `UiPlugin` helps organizationally but doesn't express any parallel/sequential intent to Bevy.
3. **No per-domain budget tracking.** `MetricsPlugin` measures tick duration but not per-system or per-domain cost. When frame time increases, there's no breakdown. The MetricSnapshot mutex makes it worse — routing per-system timings through it would make the measurement system a bottleneck (see contention map).
4. **18 systems lack guards.** Character panel (9), dev console (4), diagnostics (5) all run unconditionally. Client targeting runs at 60Hz when 10Hz suffices.
5. **FixedUpdate/Update boundary for combat** is correct but undocumented. Server Update runs uncapped — the ratio of Update to FixedUpdate frames varies with server load. A new developer adding a combat system won't know which schedule to use without reading this map.
6. **No scaling annotations in code.** The systems that scale quadratically in player density (AOI, send_do) look identical to O(1) systems in the source. The non-linear cost is invisible until you hit the wall at 50 players.

## Recommended Next Steps

1. **Define SystemSets** — one enum per binary, one variant per domain group. Wire up set-level ordering. **This is the prerequisite that makes everything else work.** Without sets, `run_if` guards must be applied per-system (error-prone), Tracy shows 89 flat spans in Update with no grouping (barely useful), and adding system #178 requires understanding all existing point-to-point orderings.
2. **Add `run_if` guards** — 18 systems across character panel, dev console, and diagnostics. Client targeting to `on_timer(100ms)`. Server targeting stays at tick rate (ability validation dependency).
3. **Use Tracy for per-system profiling** — already behind the `trace` feature flag. Tracy uses lock-free ring buffers for span collection — zero contention on the MetricSnapshot mutex. Keep MetricSnapshot for the UDP metrics console (coarse server gauges only). Do not route per-system timing through MetricSnapshot.
4. **Consolidate combat feedback (15 → 8)** — combat_log (5 → 1), threat_icons (3 → 1), resolved_threats (3 → 1). Each cluster operates on the same resource/panel and can merge without behavior change. See C14 for details.
5. **Document the FixedUpdate/Update combat boundary** — in GUIDANCE.md anti-patterns. Include: server Update is uncapped, ability handlers have variable execution count per tick, resource values have up to 125ms staleness between ticks.

## Scaling Summary — Systems That Will Blindside You

Quick reference for capacity planning. N = total entities, M = players in mutual visibility, E = total entities in NNTree.

| System | Domain | Current Cost (10 players) | At 50 Players | Scaling |
|--------|--------|---------------------------|---------------|---------|
| `update_area_of_interest` | S1 | 10×10 = 100 ops | 50×50 = 2,500 ops | **O(N×M)** — quadratic in density |
| `send_do` | S1 | ~200 byte-copies | ~5,000 byte-copies | **O(events×M)** — quadratic in density |
| `chase` | S3 | ~60 RTree queries | ~300 RTree queries | O(N log E) — linear in NPCs |
| `update_targets` | S6 | ~10 RTree queries | ~50 RTree queries | O(N log E) — linear in entities |
| `controlled::apply` | S2 | ~30 Map reads | ~150 Map reads | O(N) — linear, high constant |
| `nntree::update` | S11 | ~5 RTree ops | ~25 RTree ops | O(changed × log E) |
| `poll_chunk_tasks` | S8 | Occasional write lock | More frequent write locks | O(M) in moving players |

The top two rows are the cliff. Everything else is a slope.
