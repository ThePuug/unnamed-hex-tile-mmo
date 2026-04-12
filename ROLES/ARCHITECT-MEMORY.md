# Architect Session Memory

## Active Concerns

None currently.

## Documentation Queue

None pending.

## Recent Implementations Reviewed

| Date | Summary | Specs Checked | Findings |
|------|---------|---------------|----------|
| 2026-04-09 | Unified mesh eviction + cache warming (5cc1be7) | lod.md | Added Mesh Eviction + Cache Warming sections. Unified !needed rule, removal no-ops, server still sends removal SummaryBatch but client ignores for cache. |
| 2026-04-09 | Per-band stats + band overlap/boundary fix (2df9507, cd71ab7) | lod.md | Two Rendering Regimes rewritten: gated/ungated boundary is local_max (loaded extent), not FIXED_STREAM_RADIUS_WU constant. Overlap inflation removal aligns with spec. |
| 2026-04-09 | Flyover summary eviction + FlyoverPlugin extraction (691dd19, 036e599, d0be542) | lod.md | Flyover producer description updated: elevation_fallback → FlyoverSummaryTracker dispatch/poll via apply_batch(). File moved systems/admin.rs → plugins/flyover.rs. Ordering drop noted (acceptable per anti-pattern #1). |
| 2026-04-09 | Terrain/SpawnerCache/EventCache deletion + metrics overlay caching | terrain-generation.md, world-events.md | Public API rewritten (Terrain→Composite). Implementation Gaps updated in both specs. Spine Caching section corrected. Elevation pipeline tense fixed (migration complete). Metrics commit: pure perf, no doc impact. |
| 2026-04-09 | lod.md full rewrite | lod.md, memory | Spec described deleted inscribed hex system; rewrote to match actual summary hex implementation. Removed hex_decimate.rs and hex_decimate_mesh.rs from memory (files deleted). |
| 2026-04-09 | Dead code removal, plugin extraction, Tracy, metrics alloc | docs, GUIDANCE.md, memory | No stale doc references found. Stale spatial_difficulty test note removed from memory. |
| 2026-03-31 | World event system concurrency (IndexRegistry, ConcurrentCellCache, async chunks) | world-events.md | Framework Contract updated: register_indexes, per-index RwLock, Composite concurrency model |

## Open Questions

None currently.

## Staleness Tracker

| Document | Last Reconciled | Notes |
|----------|----------------|-------|
| docs/design/lod.md | 2026-04-09 | Gated/ungated regimes rewritten for local_max boundary; flyover producer updated |
| docs/design/terrain-generation.md | 2026-04-09 | Public API, Spine Caching, Implementation Gaps all updated for Terrain deletion |
| docs/design/world-events.md | 2026-04-09 | Implementation Gaps updated — cleanup complete, flyover resolved |
| docs/design/metrics-console.md | 2026-03-31 | seg_row duplication gap noted |
| docs/adr/032-two-ring-lod-chunk-loading.md | 2026-03-31 | Superseded notice present |
| GUIDANCE.md | 2026-03-31 | World event system pattern current |
| docs/design/combat-balance.md | unknown | Not reviewed recently |
| docs/design/combat.md | unknown | Not reviewed recently |
| docs/design/attributes.md | unknown | Not reviewed recently |
| docs/design/haven.md | unknown | Not reviewed recently |
| docs/design/hubs.md | unknown | Not reviewed recently |
| docs/design/siege.md | unknown | Not reviewed recently |
| docs/design/triumvirate.md | unknown | Not reviewed recently |
