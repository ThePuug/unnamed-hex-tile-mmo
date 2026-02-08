# ADR-019: Unified Interpolation Model

## Status

Accepted

## Context

The current movement system uses a triple-state model in the `Offset` component (`state`, `step`, `prev_step`) that causes jitter on direction changes. The root cause is that physics updates `step` directly, then rendering lerps from `prev_step` to `step`. When direction changes mid-interpolation, the lerp target jumps, causing oscillation.

Additionally, local player and remote entity movement use fundamentally different code paths:
- **Local player:** Input queue → physics prediction → `step` updated → server confirms
- **Remote entity:** Server sends Loc/Heading → `state` recalculated → `step` interpolates toward `state`

This divergence makes the system hard to understand, debug, and extend.

**Reference:** RFC-016 (Movement System Rewrite)

## Decision

Separate **authoritative state** from **visual interpolation** into distinct components, using the same interpolation model for both local and remote entities.

**Core Principle:** "Where I am" (authoritative) is separate from "where I appear" (visual). Both local and remote entities use identical interpolation logic - only the source of authoritative updates differs.

### Core Mechanism

**New Components:**

```rust
/// Authoritative position - where physics/server says entity is
#[derive(Component)]
pub struct Position {
    pub tile: Qrz,       // Discrete hex tile
    pub offset: Vec3,    // Sub-tile offset (typically -0.5 to 0.5)
}

/// Visual interpolation state - purely for rendering
#[derive(Component)]
pub struct VisualPosition {
    pub from: Vec3,      // World-space interpolation start
    pub to: Vec3,        // World-space interpolation target
    pub progress: f32,   // 0.0 to 1.0
    pub duration: f32,   // Seconds for this interpolation
}
```

**Unified Update Flow:**

```
AUTHORITATIVE LAYER (physics/network):
┌────────────────────────────────────────────────────────┐
│ Local:  Input → Physics → Position updated            │
│ Remote: Server message → Position updated             │
└────────────────────────────────────────────────────────┘
                         │
                         ▼
INTERPOLATION LAYER (rendering):
┌────────────────────────────────────────────────────────┐
│ When Position changes:                                 │
│   visual.from = current rendered position              │
│   visual.to = world_pos(new Position)                  │
│   visual.progress = 0.0                                │
│                                                        │
│ Each frame:                                            │
│   visual.progress += dt / visual.duration              │
│   Transform = lerp(visual.from, visual.to, progress)   │
└────────────────────────────────────────────────────────┘
```

**Why This Fixes Jitter:**

Current system (jitter on direction change):
```
T=0:   step=(0.5, 0, 0), prev_step=(0, 0, 0), lerp toward step
T=50:  Direction change! step=(0.5, 0, -0.5), prev_step still (0.5, 0, 0)
       Lerp now goes backward, causing visual oscillation
```

New system (smooth direction change):
```
T=0:   Position=..., visual.to=(0.5, 0, 0), visual.from=(0, 0, 0)
T=50:  Direction change! Position updated
       visual.from = current rendered position (0.25, 0, 0)
       visual.to = new target (0.5, 0, -0.5)
       Smooth continuation from wherever we are
```

The key insight: interpolation always starts from **current visual position**, not from a previous physics result. Direction changes don't cause jumps because we're always smoothly continuing from where we are.

## Rationale

**Why not patch existing system:**
- Root cause is architectural (physics directly updates render target)
- Each patch risks new edge cases
- Complexity accumulates over time

**Why separate components:**
- Clear ownership (physics owns Position, rendering owns VisualPosition)
- Testable in isolation
- Same pattern works for any interpolated value (position, rotation, scale)

**Why unified local/remote:**
- Single code path means single set of bugs
- Easier to reason about
- Natural extension to knockbacks, dashes (just update Position)

## Consequences

**Positive:**
- Eliminates direction-change jitter (root cause fixed)
- Unified local/remote handling (simpler codebase)
- Clear separation of concerns (physics vs rendering)
- Easier to test (pure functions, isolated systems)
- Foundation for advanced movement (knockbacks update Position, visual catches up)

**Negative:**
- Migration effort (many systems reference Offset)
- Additional component per entity (VisualPosition: ~20 bytes)
- Learning curve for existing codebase knowledge

**Mitigations:**
- Phased migration (old and new coexist temporarily)
- Memory overhead negligible for entity counts we support
- Clear documentation in ADR and code comments

## Implementation Notes

**File Locations:**
- `common/components/position.rs` - Position + VisualPosition components
- `common/systems/movement.rs` - Canonical physics implementation (calculate_movement + helpers)
- `common/systems/physics.rs` - Thin delegation wrapper (apply → movement::calculate_movement)
- `client/systems/prediction.rs` - Local player prediction (predict_local_player + advance_interpolation)
- `client/systems/actor.rs` - Simplified Transform sync (VisualPosition.current() → Transform)

**System Ordering:**
```
FixedUpdate: controlled::apply → tick → interpolate_remote
Update:      renet → world::do_incremental → input::do_input →
             predict_local_player → advance_interpolation → actor::update → camera
```

**Migration Strategy:**
1. Add new components alongside Offset
2. Port systems one at a time
3. Remove Offset once all systems migrated
4. Clean up compatibility shims

## References

- RFC-016: Movement System Rewrite
- ADR-016: Movement Intent Architecture (existing intent system)
- GUIDANCE.md: Position & Movement System documentation

## Date

2025-02-08
