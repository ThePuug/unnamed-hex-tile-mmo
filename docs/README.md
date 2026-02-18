# Documentation

## Design Specifications (`design/`)

Game design documents. Each spec includes Implementation Deviations and Gaps sections at the bottom.

| Spec | Description |
|------|-------------|
| [combat.md](design/combat.md) | Combat philosophy, abilities, recovery, synergies, UI |
| [attributes.md](design/attributes.md) | Bipolar pairs, A/S/S model, three scaling modes |
| [combat-balance.md](design/combat-balance.md) | Super-linear scaling, queue capacity, dismiss, NPC coordination |
| [triumvirate.md](design/triumvirate.md) | Origin, Approach, Resilience classification |
| [hubs.md](design/hubs.md) | Dynamic settlements, influence, merging |
| [siege.md](design/siege.md) | Encroachment, anger, siege waves |
| [haven.md](design/haven.md) | Starter havens, spatial difficulty |

## Architecture Decision Records (`adr/`)

Non-obvious decisions preserved because someone would re-ask "why?"

| ADR | Decision |
|-----|----------|
| [001](adr/001-chunk-based-world-partitioning.md) | 8x8 chunks, 3 invariants, bandwidth analysis |
| [007](adr/007-timer-synchronization-via-insertion-time.md) | Send insertion time once, both sides calculate remaining |
| [010](adr/010-damage-pipeline-two-phase-calculation.md) | Snapshot attacker at insertion, defender at resolution |
| [012](adr/012-ai-targetlock-behavior-tree-integration.md) | Sticky targeting prevents behavior tree cascade failures |
| [016](adr/016-movement-intent-architecture.md) | Intent-then-confirmation for remote entity prediction |
| [018](adr/018-ability-execution-pipeline-architecture.md) | 3-stage pipeline with pure function extraction |
| [019](adr/019-unified-interpolation-model.md) | Position + VisualPosition replaces triple-state Offset |
| [020](adr/020-super-linear-level-multiplier.md) | Polynomial stat scaling preserves level advantage |
| [027](adr/027-commitment-tiers.md) | 20/40/60% thresholds for build identity |
| [030](adr/030-reaction-queue-window-mechanic.md) | Unbounded queue with visibility window (planned) |
| [031](adr/031-relative-meta-attributes-rework.md) | Rotated oppositions prevent single counter-builds |
