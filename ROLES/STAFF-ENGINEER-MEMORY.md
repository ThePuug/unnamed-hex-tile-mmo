# Staff Engineer Memory

Last updated: 2026-04-12

---

## Principles (from shipped MMO postmortems)

1. **Worst case IS the feature.** Design for the moment multiple players are visible, world is streaming, NPCs chasing, combat ticking, terrain to the horizon.
2. **O(n^2) visibility will find you.** Encode once, broadcast bytes. Reverse index for who-sees-what, not forward scan.
3. **Never block on a single client.** No client behavior should affect server processing of other clients.
4. **Budget every millisecond.** 125ms server tick covers ALL players, world gen, AI, combat, AOI, network. Measure or fly blind.
5. **Death by a thousand cuts.** Hot paths must be allocation-free. Pre-allocate, reuse, ArrayVec, &'static str.
6. **Hot path / cold path separation.** Hot path must never block on cold path. Lockless queues between them.
7. **Design for density, test at density.** Single-player localhost perf means nothing.
8. **Explicit degradation strategy.** Design what happens when budget is exceeded — don't discover it in production.

---

## Open Work

### Next Up (Tier 2 remainder)

- **Movement change detection** — Physics runs every tick for every entity including idle NPCs. With 200 NPCs, ~180 idle = 180 wasted physics evals/tick. Fix: filter on `Changed<KeyBits>` or `IsMoving` marker.

### LoD Branch (single-hex-lod)

**Completed this session (2026-04-09 → 2026-04-12):**
- Flyover local summary server (691dd19→036e599) — additions+removals through apply_batch
- FlyoverPlugin extraction (d0be542) — encapsulated with clear public API
- Unified mesh eviction (5cc1be7) — `!needed.contains(k)` for all r, deleted removed_regions
- Band overlap removed, r=2 gap fixed (cd71ab7)
- Band distance: player-centric horizontal + CAMERA_DISTANCE offset (f5f0c3a), MIN_SCREEN_PX 16→12
- FOV threaded through bands (927dd56) — gameplay 60°, flyover 90°
- Component::insert_into simplifies do_incremental (4f2031c) — dropped from 39/25 to below threshold
- Region-based flyover dispatch (88468e3) — one async task per mesh region, no partial builds
- Read-only mesh builder — removed cache.insert from mesh build path, eliminates write lock contention
- SummaryCache DashMap (7821f20) — per-region locking via DashMap<MeshRegionKey, Arc<RegionData>>, deleted global RwLock + dirty_regions + changed bookkeeping, completeness gate on dispatch

**Remaining:**

| Concern | Detail |
|---|---|
| Unbounded removal batch | Teleport dumps all removals in one SummaryBatch, no budget cap |
| Client poll mesh budget | No per-frame cap on mesh builds in poll_summary_meshes |
| Spec reconciliation | Architect role — flat-hex vs hex-native decimation from spec |
| `mesh_region_lattice()` alloc | Creates new HexLattice on every call, should be OnceLock |
| Cache miss speed | Flyover async center_z computation is slow on first activation at altitude — tens of thousands of elevation_at calls |
| Ordered outward dispatch | Meshes pop in haphazardly — no distance ordering. Frontier-based dispatch proposed but deferred. |

### Scaling Risks (before 50+ players)

- **AOI at density:** O(n^2) spawn event encoding will dominate tick budget at 50+ mutual-visibility players
- **spine.rs monolith:** 3,746 lines, O(peaks^2) ridgeline building, O(peaks) per tag query, no spatial index. Where terrain gen time lives.
- **NPC spatial queries:** Per-NPC NNTree queries in chase.rs. Pre-computed occupancy grid would be better.
- **Network delta encoding:** Full component state sent on every change. Delta compression for high-frequency components (stamina, mana) would cut bandwidth significantly.
- **No degradation strategy:** Unknown behavior when tick budget exceeded. Need to design it.
- **No load test harness:** All performance claims are theoretical until stress-tested at density.

### Architecture Debt

- **Client message budget** — DEFERRED intentionally. Capping hides upstream problems (AOI burst size, chunk delivery rate) that we need visible during dev. Correct sequence: instrument, fix sources, then cap as guardrail.
- **Targeting systems** — `update_targets()` and `update_ally_targets()` run every frame querying all entities. 10Hz is sufficient. Fix: `run_if(on_timer(Duration::from_millis(100)))`.
- **Server hot-path allocations** — `aoi.rs` ✅ FIXED (b4353f2, Local buffers). Remaining: `chase.rs` Vec<Entity> per NPC target search (conditional, low priority), `input.rs` bincode encode_to_vec per confirmation (medium priority at scale).
- **Client ships admin by default** — `client/Cargo.toml` defaults to `features = ["admin"]`, pulls in entire `world` crate. Release builds should not include flyover code. Fix: `default = []`.

---

## Design Checklist

Every new system must answer:

1. **Schedule:** Which schedule, and why?
2. **Budget:** How many ms allowed? What if exceeded?
3. **Data flow:** What reads/writes? Who depends on the output?
4. **Density:** Cost at 200 entities vs 20?
