# Developer Memory

Last updated: 2026-06-12

## Terrain Performance Rescue (2026-06-12, uncommitted)

Full multi-agent audit + benchmark of the terrain pipeline. All findings adversarially
verified against code. Benchmark harness: `crates/world/tests/perf_probe.rs`
(`cargo test -p world --release --test perf_probe -- --ignored --nocapture`).

### Root causes found (all verified real)

1. **Spawner survey amplification** — `Survey::all()` + filter resolved all 271 tiles
   of a spawner cell through `resolve_below` on first deform; results thrown away
   (no caching); every chunk computed twice (542 plate+spine queries per 271 tiles);
   every sparse LoD elevation sample paid a full 271-tile survey (~1ms each, ~70×).
2. **`None` query results never cached** — sea/flat tiles re-ran the spine query on
   every access, forever.
3. **Plate query brute force** — ~280 simplex evals/tile (one per Voronoi candidate),
   classify recomputed a per-plate constant per tile, DashMap `entry()` write locks
   on the hit path (9 per tile query).
4. **Deform cascade** — first touch of a spine cell deforms 169 plate cells; was
   ~140ms; `populate_chunk` re-evaluated the 6-simplex regime stack per macro cell
   with ~6.9× scan overlap; `plate_neighbors` unmemoized.
5. **Consumer scheduling** — spawn burst queued 1,387 chunk tasks unbounded in raster
   order on a ≤4-thread pool; `dispatch_summary_tasks` ran per frame per player
   (~47k HashSet ops) after the movement throttle was removed.

### Fixes applied (working tree, in suggested commit order)

- F1 `events/mod.rs`: cache `None` as default TileOutput; `resolve_below` write-through.
- F2 `WorldEvent::contributes_tiles()` (default true; SpawnerEvent false): index-only
  events skipped in tile materialization; new `Composite::ensure_indexed(coords)`;
  `generate_chunk` warms spawner index after materializing (survey hits warm cache);
  `spawners_near` ensures own cell; world-viewer warms sampled coords.
- F3 `plates.rs`: two-pass warped Voronoi with conservative prune bound
  (`WARP_NOISE_BOUND=1.1`, winner bit-identical, invariant test
  `warped_plate_pruning_matches_brute_force`); classify memo per plate id;
  `plate_neighbors` memo; regime memo per macro cell + suppression-hash early-outs
  (sound because regime is a sigmoid in (0,1)); DashMap get-before-entry.
- F4 `spine.rs`: `ridge_elevation_at` pre-noise distance rejection (bound 600+75×1.1);
  per-ridgeline `wobble_seed` hoisted into struct; `SpineInstance::sample_at` merges
  elevation+tag single pass (SpineEvent::query uses it); `grow_stream` single
  raw-gradient eval per step (was 2); `generate_ridge_paths` early return when
  `PATH_PROBABILITY == 0`; `apply_peak_to_plates` empty-map guard.
- F5 `server`: chunk gen pending queue, nearest-first, `MAX_CHUNK_TASKS=16`;
  `dispatch_summary_tasks` on 250ms timer.

### Measured results (perf_probe, release)

| Scenario | Before | After |
|---|---|---|
| Sparse LoD sample (marginal) | ~960µs | ~8µs |
| Dense region per tile | 28µs | ~6µs marginal |
| Warm re-access | 726ns | ~300ns |
| First touch (per spine cell, one-time) | ~140ms | ~84ms |
| Single plate-cell deform | 7.5ms | 2.7ms |

## Near-field rendering fix (2026-06-12, uncommitted)

**Bug:** in gameplay mode nothing rendered between ~36 WU (r=0 band edge) and
~599 WU (stream radius). Root cause: commit ff391b9 ("simplify: remove Map-tile
fallback from r>0 summary mesh builds") assumed server/flyover populate
SummaryCache for all r>0 regions — but the server only sends summaries BEYOND
FIXED_STREAM_RADIUS_WU and flyover only produces while active. Client-owned
r=1/r=2 regions inside the stream radius had no data source, and the dispatch
gate (`r>0 && !contains_region -> skip`) blocked them entirely.

**Fix (crates/client/src/systems/world.rs):**
1. `compute_auto_mode_regions`: bands straddling the stream-radius boundary are
   SPLIT into a gated segment + an ungated segment (previously the whole band
   went to one side, dropping the other segment's coverage — left a [348,599]
   hole for the r=2 band).
2. Dispatch gate: r>0 regions without cache data are skipped only when BEYOND
   the stream radius (server-owned). Client-owned regions dispatch and build
   from Map.
3. `collect_and_build_summary_mesh` r>0: cache-first, then full-hexball
   `select_center_z` from Map tiles (None until all tiles of a summary are
   loaded; region re-dispatches on map changes until complete). No cache
   write-back — Map stays the live authority for local regions.
4. Dispatch budget re-arm: `map.force_changed()` when MAX_MESH_TASKS budget
   saturates, so backlogs drain next frame instead of waiting for new data.

Ownership model now explicit: client owns r-any within FIXED_STREAM_RADIUS_WU
(from Map), server/flyover own beyond (via SummaryCache). lod.md needs this
reconciled (ARCHITECT).

## Summary LoD redesign plan (2026-06-12)

User asked for a full revisit of the summary approach ("disconnected unaligned
bands, sudden shifts when summary r changes"). 4-reader audit done (spec vs
geometry vs lifecycle vs producers). Plan written to
`.claude/plans/well-knit-world.md`. Core findings: all continuity machinery is
band-scoped (no cross-r stitching exists anywhere, incl. r=0→r=1 at 36 WU);
hexagons can't subdivide so cross-band vertex matching is impossible at ANY
radii; center_z re-derived per band with extremal rule → terrain breathes;
two center_z algorithms split at 598.5; despawn-before-build; bands at 60° FOV
while default zoom is 15°. Design: (1) nested levels scale ∈ {1,3,9,27,81} —
×3 nesting makes the existing 7-sample rule exactly hierarchical (samples land
on child centers); (2) band table anchored so scale-3 outer == stream radius
(ownership boundary == band boundary); (3) frontier curtains (chunked-LoD
downward skirts) for geometric closure + build-before-evict + hysteresis.
Phases P0-P4 in the plan file. **P0–P3 IMPLEMENTED 2026-06-12** (uncommitted,
all 646 workspace tests green):
- P0: client far_ground clamp fix (world.rs + flyover.rs use offset + y, not
  (y−offset).max(0)); nearest-first client mesh dispatch; REEVAL_MOVE_WU=16
  movement-triggered re-eval; shader fade = horizon formula (terrain.wgsl
  INV_TAN_HORIZON_MARGIN×0.92, was magic 627/90 hiding 39% of far field).
- P1: LOD_LEVELS=[0,1,4,13,40,121], BAND_QUALITY_K=119.75 (scale-3 outer ==
  598.5 == stream radius, fov-independent — compute_active_bands lost its
  fov param); sample_center_z everywhere (sample_center_z_opt for fallible
  Map source); visible_lod_regions footprint-aware (producers cover
  boundary-straddling regions); 3 new invariant tests (INV-010 nesting,
  anchor, contiguity); lod.md deviations section updated.
- P2: frontier curtains (CURTAIN_DEPTH_WU=24) on unmatched perimeter edges
  in append_cross_region_skirts; matching now checked against all 6
  neighbors (suppression) while skirt emission stays lower-key-owned;
  poll Phase 2 re-patches both-side neighbors (stale curtain → skirt swap,
  no coplanar z-fight). r=0 frontier closes from the r=1 side + r=0's own
  tile cliff skirts.
- P3: BAND_HYSTERESIS_MARGIN=0.08 (needed crisp / keep widened);
  build-before-evict (stale region held until overlapping needed regions
  have entities — region_center_world overlap test); RegionSource
  provenance on client SummaryCache (insert_region merges cells, promotes
  to Server; clear_flyover retains server data — fixes blank horizon after
  flyover with NO protocol change); server removals deleted from the wire
  (visible_set expansion deleted too — was ~16k wasted inserts/player/tick).
- P3.5 (2026-06-12, after user validation "way way better" but gaps at 1/4
  boundary): root cause = center-distance region membership vs ±half-extent
  footprints — regions centered just inside a band edge belonged to NEITHER
  band (gap crescents up to H(coarse) deep: ~140 WU at 1/4, ~420 at 4/13).
  Fix: footprint-overlap enumeration (band annulus expanded by
  0.5·mesh_region_extent_wu(r) both ways) in compute_auto_mode_regions AND
  visible_lod_regions (producer matches consumer). Adjacent levels now
  overlap slightly at boundaries instead of gapping; LEVEL_DEPTH_BIAS_WU =
  0.08/level-rank (render-only, in SummarySurface::flat) keeps overlapping
  plates from z-fighting on flat terrain (nested sampling makes equal z
  common) — finer level always wins. Known accepted artifact: coarse plates
  can poke through sloped r=0 terrain in the ~73-180 WU ring (pre-existing
  class of artifact; revisit with per-summary trimming if visible).
- P3.6 (2026-06-12, user screenshot: big missing ring in FLYOVER): TWO root
  causes found via a new analytic coverage test
  (`lod_bands_cover_horizon_without_gaps` in client world.rs — sweeps 360°
  to the horizon asserting geometric + data coverage, gameplay AND flyover):
  1. Flyover used the gameplay stream-radius constant as the gated/ungated
     ownership boundary, but flyover only keeps detail chunks in a small
     disc → un-rendered ring from flyover-chunk extent to 598.5. Fix:
     `local_boundary_wu` threaded through compute_auto_mode_regions +
     visible_lod_regions + the is_local gate; FlyoverState publishes
     detail_radius_chunks each frame (gen system), boundary =
     detail_radius × CHUNK_EXTENT_WU × √3/2.
  2. DEEPER, gameplay too: FIXED_STREAM_RADIUS_WU (598.5 = 21×28.5) is the
     chunk hexagon's CIRCUMRADIUS; actual coverage in edge directions ends
     at the APOTHEM (×√3/2 ≈ 518.3). Everything keyed on 598.5 over-claimed
     Map coverage → six gap crescents [518, 598.5] where the chunk gate
     correctly rejected regions that nobody else produced. Fix: new
     FIXED_STREAM_APOTHEM_WU + APOTHEM_FACTOR in chunk.rs; ownership
     boundary everywhere = apothem (server, client, flyover). Producers'
     circular coverage overlaps the hexagon corners — harmless (identical
     values, depth bias orders plates).
- P4 NOT started: summary pseudo-normals, refine-morph, frustum wedge,
  data-driven curtain depth. Wants user visual validation first.

### Flags / follow-ups (NOT fixed, reported)

- **Spawner archetype bug (pre-existing)**: deform stores Kiter placeholder for all
  placements; engagement_spawner consumes it → all server engagements are Kiter.
  Fix idea: resolve archetype in `spawners_near` via `tags_at` (warm cache).
- **Multi-waiter chunk gap (pre-existing)**: in-flight task keyed to first requesting
  entity; second player never receives that chunk.
- **Spine survey edge order-dependence**: `all_neighbors_in` checks centroid neighbors
  via global `tags_at` index; centroids at the survey footprint edge accept/reject
  depending on whether adjacent plate cells were deformed earlier → exploration-order
  dependent spine placement at footprint edges. Needs ARCHITECT review.
- **CellCache eviction still disabled** (tracked in world-events.md gaps).
- `default = ["trace"]` + TRACE log level in uncommitted Cargo.toml/main.rs changes:
  the `world=debug` filter activates the deform/query debug spans → measurable
  overhead on the exact hot path; fine for profiling, should not stay default.
- Docs: lod.md / world-events.md / terrain-generation.md need reconciliation for
  contributes_tiles + ensure_indexed (ARCHITECT session).
