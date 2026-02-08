# RFC-016: Movement System Rewrite

## Status

**Approved** - 2025-02-08

## Feature Request

### Player Need

From player perspective: **Smooth, predictable movement** - Movement should feel responsive without jitter, snapping, or desync artifacts.

**Current Problem:**
Without a rewrite:
- Direction changes cause visible jitter (especially at tile boundaries)
- Implementation is "jank" - complex interacting systems without clear intent
- Local player and remote entities use different code paths (inconsistent behavior)
- Offset component has confusing triple-state model (state/step/prev_step)
- Physics, prediction, and rendering are tightly coupled
- Debugging movement issues requires understanding 5+ files simultaneously

**We need a system that:**
- Eliminates direction-change jitter
- Has clear, understandable code with documented intent
- Unifies local and remote entity movement handling
- Separates concerns cleanly (physics, network, rendering)
- Is easy to debug, test, and extend

### Desired Experience

Players should experience:
- **Smooth Movement:** No jitter when changing direction or crossing tiles
- **Responsive Input:** Local player responds immediately to input
- **Consistent Behavior:** Local and remote entities move the same way visually
- **Predictable:** Movement behaves as expected, no surprises or artifacts

### Specification Requirements

**Movement Foundation:**
- Hexagonal grid movement with 6 cardinal directions
- Discrete tile positions (Loc) with sub-tile interpolation
- Server-authoritative with client-side prediction for local player
- Smooth visual interpolation for all entities

**Local Player:**
- Input captured and applied immediately (client-side prediction)
- Server validates and confirms inputs
- Reconciliation on desync (smooth correction, not snap)

**Remote Entities:**
- Server broadcasts movement intent when movement starts
- Client predicts toward destination
- Smooth interpolation during movement
- Graceful handling of late/dropped packets

**Visual Layer:**
- Single interpolation model for all entities
- Smooth transitions on direction change
- No jitter at tile boundaries
- Frame-rate independent rendering

### MVP Scope

**Phase 1 - Core Model (foundation):**
- New movement state model (replaces Offset.state/step/prev_step)
- Unified movement component for local and remote
- Clean physics calculation (deterministic, testable)

**Phase 2 - Local Player (prediction):**
- Input handling with prediction
- Server confirmation and reconciliation
- Smooth desync correction

**Phase 3 - Remote Entities (intent):**
- MovementIntent handling (leverage existing protocol)
- Destination-based interpolation
- Late packet handling

**Phase 4 - Visual Polish (rendering):**
- Direction-change smoothing
- Tile boundary handling
- Jump/fall integration

**Phase 1 excludes (deferred):**
- Knockback/forced movement
- Pathfinding
- Multi-tile waypoints
- Speed modifiers beyond Grace attribute

### Priority Justification

**HIGH PRIORITY** - Core gameplay feel is compromised by jitter.

**Why high:**
- Movement is the most fundamental player interaction
- Current jitter undermines combat positioning (core to spec)
- Technical debt makes bug fixes difficult
- Blocks future movement features (knockback, dashes)

**Benefits:**
- Eliminates visible jitter artifacts
- Simpler codebase (easier maintenance)
- Unified code path (fewer bugs)
- Foundation for advanced movement features

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Unified Interpolation Model**

#### Core Mechanism

**New State Model:**
Replace the confusing `Offset { state, step, prev_step }` with a clearer model:

```
MovementState {
    // Where physics says we are (server-authoritative position)
    authoritative: Vec3,

    // Visual interpolation
    visual_from: Vec3,      // Start position for current interpolation
    visual_to: Vec3,        // Target position for current interpolation
    visual_progress: f32,   // 0.0 to 1.0

    // Movement metadata
    is_moving: bool,
    direction: Qrz,         // Current heading
}
```

**Key Insight:** Separate "where I am" (authoritative) from "where I appear" (visual). Both local and remote entities use the same interpolation, just with different authoritative sources.

**Unified Flow:**
```
┌─────────────────────────────────────────────────────────────┐
│                    AUTHORITATIVE LAYER                       │
├─────────────────────────────────────────────────────────────┤
│  Local Player:     Input → Physics → (send to server)       │
│  Remote Entity:    (receive from server) → Apply            │
│                                                              │
│  Both produce: MovementState.authoritative                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    INTERPOLATION LAYER                       │
├─────────────────────────────────────────────────────────────┤
│  When authoritative changes:                                 │
│    visual_from = current visual position                     │
│    visual_to = new authoritative position                    │
│    visual_progress = 0.0                                     │
│                                                              │
│  Each frame:                                                 │
│    visual_progress += delta_time / interpolation_duration    │
│    render_position = lerp(visual_from, visual_to, progress)  │
└─────────────────────────────────────────────────────────────┘
```

**Direction Change Smoothing:**
Current jitter occurs because:
1. Direction change immediately updates `step`
2. `prev_step` lerping creates oscillation when physics runs multiple times

Fix: Direction changes update `visual_to`, not current position. Interpolation naturally smooths the transition.

#### Component Architecture

**New Components:**
```rust
/// Authoritative position (tile + sub-tile offset)
#[derive(Component)]
pub struct Position {
    pub tile: Qrz,           // Discrete hex tile
    pub offset: Vec3,        // Sub-tile offset (-0.5 to 0.5 range)
}

/// Visual interpolation state (rendering only)
#[derive(Component)]
pub struct VisualPosition {
    pub from: Vec3,          // World-space start
    pub to: Vec3,            // World-space target
    pub progress: f32,       // 0.0 to 1.0
    pub duration: f32,       // Seconds for this interpolation
}

/// Movement intent (for prediction/interpolation timing)
#[derive(Component)]
pub struct MovementIntent {
    pub destination: Qrz,    // Target tile
    pub duration_ms: u16,    // Expected travel time
    pub started_at: f32,     // Game time when movement started
}
```

**Removed Components:**
- `Offset` (replaced by Position + VisualPosition)

**Kept Components:**
- `Loc` (renamed to Position.tile conceptually, or keep as alias)
- `Heading` (direction entity faces)
- `KeyBits` (input state)
- `AirTime` (jump/fall state)

#### System Separation

**Physics Systems (FixedUpdate, deterministic):**
- `physics::calculate_movement` - Pure function: (Position, Heading, KeyBits, dt) → Position
- `physics::apply_local` - Apply physics result to local player
- `physics::apply_remote` - Apply server updates to remote entities

**Network Systems (Update):**
- `network::send_input` - Send local player input to server
- `network::receive_updates` - Apply server authoritative updates
- `network::send_intent` - Server broadcasts movement intent

**Rendering Systems (Update, after network):**
- `render::update_interpolation` - Advance visual_progress each frame
- `render::sync_transforms` - Apply VisualPosition to Transform

#### Performance Projections

**Overhead:**
- One additional component per entity (VisualPosition: 20 bytes)
- Simpler per-frame logic (fewer branches, no triple-lerp)
- Net: Slight improvement due to simpler code paths

**Development Time:**
- Phase 1 (Core Model): 3-4 hours
- Phase 2 (Local Player): 4-6 hours
- Phase 3 (Remote Entities): 3-4 hours
- Phase 4 (Visual Polish): 2-3 hours
- Testing & Polish: 2-3 hours
- Total: 14-20 hours (fits in one SOW)

#### Technical Risks

**1. Migration Complexity**
- *Risk:* Changing core components breaks many systems
- *Mitigation:* Phased migration, keep old components temporarily
- *Impact:* Manageable with careful ordering

**2. Physics Determinism**
- *Risk:* New physics doesn't match old exactly (breaks replays, tests)
- *Mitigation:* Port physics logic directly, test equivalence
- *Impact:* Low - physics calculation stays the same, just cleaner interface

**3. Network Protocol Changes**
- *Risk:* New components need different serialization
- *Mitigation:* Keep wire format compatible where possible
- *Impact:* Low - MovementIntent already exists

### System Integration

**Affected Systems:**
- `common/components/` - New Position, VisualPosition components
- `common/systems/physics.rs` - Refactored calculation
- `client/systems/input.rs` - Simplified input handling
- `client/systems/actor.rs` - New interpolation logic
- `server/systems/actor.rs` - Unified position updates
- `*/systems/renet.rs` - Component sync updates

**Compatibility:**
- Wire protocol: Compatible (MovementIntent unchanged)
- Existing features: Combat, abilities work (use Position.tile)
- Tests: Update to use new components

### Alternatives Considered

#### Alternative 1: Patch Existing System

Fix jitter bugs in current implementation without rewrite.

**Rejected because:**
- Root cause is architectural (triple-state model)
- Each fix risks introducing new edge cases
- Doesn't address code clarity issues
- Technical debt continues accumulating

#### Alternative 2: Full ECS Overhaul

Use Bevy's built-in Transform for everything, remove custom position handling.

**Rejected because:**
- Hex grid doesn't map cleanly to Transform
- Would require rewriting terrain, combat, everything
- Too large in scope for one SOW
- Custom position gives us control we need

#### Alternative 3: Velocity-Based Movement

Store velocity instead of destination, integrate each frame.

**Rejected for MVP because:**
- More complex for discrete hex movement
- Overkill for current features
- Consider for Phase 2 (knockbacks, dashes)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Decision:** Separate authoritative state from visual interpolation. This is the core insight that enables unified local/remote handling.

**Why this fixes jitter:** Current system updates `step` directly from physics, then lerps from `prev_step`. When direction changes mid-interpolation, the lerp target jumps, causing oscillation. New system: physics updates authoritative, rendering smoothly interpolates toward it.

**Extensibility:**
- Knockback: Just set authoritative position, visual catches up
- Dashes: Set authoritative to destination, adjust interpolation duration
- Pathfinding: Queue of destinations, pop when reached

**Testing Strategy:**
- Physics pure functions are easily unit tested
- Interpolation can be tested with mock time
- Integration tests verify smooth visual output

### PLAYER Validation

**From combat-system.md spec:**
- "Positioning matters" - Clean movement enables tactical positioning
- "Conscious but Decisive" - Responsive controls, deliberate movement

**Jitter Fix Validation:**
- Direction changes should be smooth, not jerky
- Tile boundaries should be invisible (no snap/pop)
- Movement should feel "weighty but responsive"

---

## Approval

**Status:** Approved

**Approvers:**
- ARCHITECT: ✅ Clean separation of authoritative/visual, unified model, testable
- PLAYER: ✅ Fixes jitter, simpler mental model, smooth movement

**Scope Constraint:** 14-20 hours (fits in one SOW)

**Dependencies:**
- None (rewrite is self-contained)

**Next Steps:**
1. Review and iterate on this RFC
2. Upon approval, extract ADR for "Unified Interpolation Model"
3. Create SOW-016 with phased implementation plan
4. Begin Phase 1 implementation

**Date:** 2025-02-08
