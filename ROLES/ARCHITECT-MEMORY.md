# Architect Session Memory

## Active Concerns

**lod.md spec has accumulated conflicting geometry sections.** The spec went through multiple rewrites in a single session (r=1 specific → inscribed hex general → slope clamping added → slope clamping removed → odd-radius-only constraint added). The file was also externally reverted during the session. Current state should be verified against what was actually committed.

**lod.md Implementation Deviations table (items 1-4)** added for the SummaryBatch system. These document real drift between spec and implementation — spec describes per-hexball decimation pipeline, implementation sends center_z per summary cell. Severity: drift. Not yet confirmed as intentional vs temporary.

## Documentation Queue

None pending — all summaries from this session have been processed.

## Recent Implementations Reviewed

| Date | Summary | Specs Checked | Findings |
|------|---------|---------------|----------|
| 2026-03-31 | Cascade overlap fix, plate scale 128→1800, Survey::none(), min_spacing predicate, metrics ring buffers, console changes, index trait has_cell removal | world-events.md, terrain-generation.md, metrics-console.md | Multiple updates applied — new predicates (Survey::none, min_spacing), plate scale, metrics terminology |
| 2026-03-31 | Spine min_spacing(10_000), determinism tests updated | world-events.md, terrain-generation.md | Spine survey/deform sections updated |
| 2026-03-31 | Crate rename terrain→world, viewer rewrite to Composite | world-events.md, terrain-generation.md, GUIDANCE.md | Cross-doc reference updates |
| 2026-03-31 | IndexRegistry interior mutability (RwLock), ConcurrentCellCache, global Mutex removal | world-events.md | Framework Contract trait signature, Composite concurrency model |
| 2026-03-31 | Per-index RwLock, Arc eviction safety, async metrics, async chunk generation | world-events.md | IndexRegistry HashMap immutable after init, register_indexes trait method |
| 2026-03-31 | Hex-native decimation algorithm (hex_decimate.rs) | lod.md | Implementation deviation for residual formula (later reverted — was a bug) |
| 2026-03-31 | Decimated mesh generation (hex_decimate_mesh.rs) | lod.md | File location added to memory |
| 2026-03-31 | QEM removal, console decimation threshold menu | lod.md | qem.rs file location removed from memory |
| 2026-03-31 | lod.md new spec created | terrain-generation.md, ADR-032 | New design spec, cross-references updated, ADR-032 superseded notice |
| 2026-03-31 | SummaryBatch server/client system | lod.md | 4 implementation deviations documented |
| 2026-03-31 | Flyover summary generation | — | No spec changes needed |
| 2026-03-31 | RemoteSummaryCache → SummaryCache unification | — | Memory updated |

## Open Questions

None currently.

## Staleness Tracker

| Document | Last Reconciled | Notes |
|----------|----------------|-------|
| docs/design/lod.md | 2026-03-31 | Multiple rewrites; verify committed state matches expectations |
| docs/design/world-events.md | 2026-03-31 | Concurrent Composite, per-index RwLock, register_indexes |
| docs/design/terrain-generation.md | 2026-03-31 | Survey::none, min_spacing, spine caching dead code note |
| docs/design/metrics-console.md | 2026-03-31 | seg_row duplication gap noted |
| docs/adr/032-two-ring-lod-chunk-loading.md | 2026-03-31 | Superseded notice added |
| GUIDANCE.md | 2026-03-31 | World event system pattern current |
| docs/design/combat-balance.md | unknown | Not reviewed this session |
| docs/design/combat.md | unknown | Not reviewed this session |
| docs/design/attributes.md | unknown | Not reviewed this session |
| docs/design/haven.md | unknown | Not reviewed this session |
| docs/design/hubs.md | unknown | Not reviewed this session |
| docs/design/siege.md | unknown | Not reviewed this session |
| docs/design/triumvirate.md | unknown | Not reviewed this session |
