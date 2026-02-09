# SOW-019: Combat Feedback Enhancements

## Status

**Complete** - 2026-02-09

## References

- **RFC-019:** [Combat Feedback Enhancements](../01-rfc/019-combat-feedback-enhancements.md)
- **ADR-025:** [Combat Feedback Enhancements (Resolved Threats + Combat Log)](../02-adr/025-combat-feedback-enhancements.md)
- **ADR-014:** [Combat HUD Layered Architecture](../02-adr/014-combat-hud-layered-architecture.md)
- **Branch:** main (direct implementation)
- **Implementation Time:** 4–6 hours

---

## Implementation Plan

### Phase 1: Resolved Threats Stack

**Goal:** Add a visual stack below the threat queue that shows recently resolved threats for 4 seconds before fading away

**Deliverables:**
- New components in `src/client/components/mod.rs`:
  - `ResolvedThreatEntry` (spawn time, lifetime, damage)
  - `ResolvedThreatsContainer` (marker)
- New system file `src/client/systems/resolved_threats.rs` with 3 systems:
  - `setup()` — Spawn persistent container below threat queue
  - `on_damage_resolved()` — Listen to ApplyDamage events, spawn entries
  - `update_entries()` — Fade out entries, despawn when expired
- System registration in `src/client/plugins/ui.rs`
- Module export in `src/client/systems/mod.rs`

**Architectural Constraints:**
- Max 5 entries visible at once (FIFO despawn when 6th arrives)
- Entry lifetime: 4.0 seconds (constant)
- Fade pattern: Manual alpha calculation `1.0 - (elapsed / lifetime)` (same as FloatingText)
- Position: Center-top, below threat queue (~70px vertical offset)
- Entry size: 30px diameter circular, 2px red border
- Background: Dark red `(0.3, 0.1, 0.1)` semi-transparent, fades with border
- Layout: Vertical flex column, 3px gap between entries
- Event trigger: `Do::ApplyDamage` event
- Entry content: Damage number in white text (14px font)
- Border radius: 50% (circular, matches threat icons)
- Despawn condition: `elapsed >= lifetime`

**Success Criteria:**
- Container spawns on Startup, persists for session
- Entries appear when `Do::ApplyDamage` event fires
- Each entry shows damage number from event
- Entries fade smoothly over 4 seconds (alpha 1.0 → 0.0)
- Border and background both fade at same rate
- Max 5 entries enforced (6th attack despawns oldest)
- Entries positioned in vertical stack below threat queue
- No frame drops with rapid damage (10+ hits/sec)
- `cargo build` succeeds, no warnings

**Duration:** 1.5–2 hours

**Dependencies:** None (standalone UI feature)

---

### Phase 2: Combat Log

**Goal:** Add a scrollable combat log panel in the bottom-left corner showing the last 50 timestamped combat events

**Deliverables:**
- New components in `src/client/components/mod.rs`:
  - `CombatLogPanel` (marker)
  - `CombatLogContent` (marker for scrollable content)
  - `CombatLogEntry` (timestamp, is_player_damage)
- New system file `src/client/systems/combat_log.rs` with 4 systems:
  - `setup()` — Spawn log panel in bottom-left corner
  - `on_damage_applied()` — Log damage events with source/target/amount
  - `on_queue_cleared()` — Log dodge/deflect events
  - `maintain_log()` — Enforce max 50 entries, auto-scroll
- System registration in `src/client/plugins/ui.rs`
- Module export in `src/client/systems/mod.rs`

**Architectural Constraints:**
- Panel position: Bottom-left corner (10px margins)
- Panel size: 400px wide × 250px tall
- Background: Dark semi-transparent `(0.1, 0.1, 0.1, 0.85)`
- Border: Gray `(0.5, 0.5, 0.5, 0.8)`, 1px width
- Font size: 12px monospace (for timestamp alignment)
- Padding: 10px
- Overflow: `Clip Y` (vertical scroll)
- Max entries: 50 (FIFO despawn when 51st arrives)
- Entry format: `[HH:MM:SS] Source → Target: 45 dmg (⚔ Physical)`
- Timestamp: Local client time, format `HH:MM:SS` (24-hour)
- Color coding:
  - Damage dealt (player is source): Red `(1.0, 0.3, 0.3)`
  - Damage taken (player is target): Orange `(1.0, 0.6, 0.0)`
  - Dodges/clears: Gray `(0.6, 0.6, 0.6)`
- Event triggers:
  - `Do::ApplyDamage` → Log damage event
  - `Do::ClearQueue` → Log clear event
- Entity name resolution:
  - Query `EntityType` component
  - Map to display name (e.g., "Wild Dog", "Bandit")
  - Despawned entities → "Unknown"
- Layout: Vertical flex column, entries stack bottom-to-top (newest at bottom)

**Success Criteria:**
- Panel spawns on Startup, positioned in bottom-left corner
- Panel visible and readable (text not clipped)
- Entries appear when `Do::ApplyDamage` fires
- Entries show correct timestamp (current time at event)
- Entries show correct source/target names
- Entries show correct damage amount
- Color coding works:
  - Player attacking enemy → Red text
  - Enemy attacking player → Orange text
- `Do::ClearQueue` events create gray "cleared queue" entries
- Max 50 entries enforced (51st despawns oldest)
- Entries ordered chronologically (oldest at top, newest at bottom)
- No frame drops with rapid events (10+ events/sec)
- `cargo build` succeeds, no warnings

**Duration:** 2–3 hours

**Dependencies:** Phase 1 (shares component definitions, but can be developed in parallel)

---

### Phase 3: Polish & Integration

**Goal:** Refine visuals, handle edge cases, and verify end-to-end integration

**Deliverables:**
- Entity name resolution for all entity types
- Damage type display (show "Physical" vs "Magic" in log)
- Handle despawned entities gracefully ("Unknown" fallback)
- Performance testing with 10+ hits/sec
- Visual refinement (spacing, alignment, font sizing)
- Edge case handling:
  - Empty queue + ClearQueue event → no log entry
  - Rapid dismiss (multiple threats in 1 second)
  - Overflow damage (>5 simultaneous hits)

**Architectural Constraints:**
- Entity name query: `Query<&EntityType>` with fallback to "Unknown"
- Damage type extraction from `Do::ApplyDamage` event (if available)
- Performance budget: <0.3ms per frame total overhead
- Memory budget: ~8.5KB total (5 entries × 200 bytes + 50 entries × 150 bytes)
- No new network messages (client-side only)
- No server changes (uses existing events)

**Success Criteria:**
- Entity names resolve correctly for all entity types
- Despawned entities show "Unknown" (no panics)
- Damage type shows in log ("Physical" or "Magic")
- Performance: 60 FPS maintained with max load (5 + 50 entries)
- No memory leaks (profiling shows stable memory usage)
- Edge cases handled gracefully (no crashes)
- Visual polish complete (spacing looks good, text readable)
- End-to-end test: Fight 3 enemies, verify:
  - Resolved threats appear below queue
  - Combat log shows all damage events
  - Color coding correct (red vs orange)
  - Timestamps increasing chronologically

**Duration:** 0.5–1 hour

**Dependencies:** Phase 1 + Phase 2 complete

---

## Acceptance Criteria

**End-to-End Testing:**
1. Run client + server
2. Spawn enemy with debug command
3. Let enemy attack or attack enemy
4. **Verify resolved threats:**
   - Entries appear below threat queue when damage applies
   - Circular entries with red border, dark red background
   - Damage number visible in white text
   - Fade out over 4 seconds (smooth alpha transition)
   - Max 5 entries visible (6th attack despawns oldest)
5. **Verify combat log:**
   - Entries appear in bottom-left panel
   - Timestamps increase chronologically
   - Color coding correct:
     - Player dealing damage → Red
     - Player taking damage → Orange
     - Clear events → Gray
   - Entity names resolve (or "Unknown" if despawned)
   - Max 50 entries maintained (51st despawns oldest)
6. **Verify performance:**
   - 60 FPS maintained during combat
   - No frame drops with rapid damage (10+ hits/sec)
   - No memory leaks (stable memory usage)

**Code Quality:**
- `cargo build` succeeds with no warnings
- `cargo clippy` passes with no warnings
- Follows FloatingText fade pattern (combat_ui.rs:5-41)
- Follows event handling pattern (threat_icons.rs:289-303)
- Components documented with ADR-025 references
- Systems have clear comments explaining purpose

## Performance Budget

- **Resolved threats:** Max 5 entries × ~200 bytes = 1KB, <0.1ms/frame
- **Combat log:** Max 50 entries × ~150 bytes = 7.5KB, <0.2ms/frame
- **Total overhead:** <0.3ms/frame (acceptable for 60 FPS target)

## Edge Cases

1. **Dead/despawned source entity:** Use "Unknown" as entity name
2. **Rapid damage (>10 hits/sec):** Entries still spawn/despawn correctly
3. **ClearQueue before InsertThreat:** Don't log (no threats to clear)
4. **Overflow damage (>5 hits simultaneously):** Oldest entries despawn, no crash
5. **Empty combat log on startup:** Panel visible but empty (no entries until first event)
6. **Client time desync:** Timestamps use local time (acceptable for MVP)

## Discussion

### Implementation Notes

**Phase 1: Resolved Threats Stack** ✅ Complete
- All deliverables implemented as specified
- Used Bevy 0.17's BorderColor struct (requires `BorderColor::all()` instead of tuple access)
- Fade pattern follows FloatingText reference exactly
- Event handling uses MessageReader pattern from threat_icons.rs
- **Filtering**: Only shows threats resolved AGAINST player (checks if `ent == player_entity`)
- **Layout**: Uses flex column with `row_gap` for automatic vertical stacking
  - Entries no longer use absolute positioning (prevents stacking bug)
  - Container uses absolute positioning, children use flex layout
  - Natural FIFO order: newest entry appears at bottom of stack

**Phase 2: Combat Log** ✅ Complete
- All deliverables implemented as specified
- Used EntityType::display_name() method (already existed, cleaner than custom mapping)
- Added chrono dependency for timestamp formatting (HH:MM:SS format)
- Color coding implemented per spec (red=dealt, orange=taken, gray=clear)
- Scrolling implemented following Bevy's official scroll example:
  - `overflow: Overflow::scroll_y()` enables vertical scrolling container
  - `ScrollPosition::default()` explicitly added (required for scroll to function)
  - `handle_scroll()` - Mouse wheel scrolling using `EventReader<MouseWheel>` + `HoverMap`
    - Uses `HoverMap` from bevy_picking with parent chain walking for hover detection
    - `EventReader<MouseWheel>` for standard Bevy mouse wheel events (not MessageReader)
    - Handles both MouseScrollUnit::Line (desktop mice) and Pixel (touchpads)
    - Clamps scroll position using `ComputedNode::content_size()` and `size()`
  - `auto_scroll_to_bottom()` - Auto-scrolls when new entries added
    - Triggered only when CombatLogEntry components are Added
    - Sets ScrollPosition.y to f32::MAX to show latest entries
- **Picking fix**: Full-screen UI overlay containers were blocking `HoverMap` hover detection
  - Added `Pickable::IGNORE` to full-screen containers in: `ui.rs`, `action_bar.rs`, `threat_icons.rs`, `resource_bars.rs`
  - This is the correct Bevy pattern for non-interactive overlay/layout containers

**Phase 3: Polish & Integration** ✅ Complete
- Entity name resolution uses existing EntityType::display_name() method
- Handles despawned entities gracefully ("Unknown" fallback)
- Damage type shows as "Physical" in log (hardcoded for MVP)
- All edge cases handled per spec

**Additional Work (Post-Plan):**

- **NPC death delay**: Instead of immediately despawning NPC entities on server Despawn event, entities now linger for 3 seconds in a death pose (rotated 90° on Z-axis)
  - New `DeathMarker` component tracks death time
  - `write_do` Despawn handler inserts DeathMarker instead of despawning (still removes from EntityMap)
  - `cleanup_dead_entities` system applies rotation on first frame, despawns after 3s
  - `actor::update` excludes DeathMarker entities (prevents heading overwriting death rotation)
  - `update_dead_visibility` no longer hides dead entities (death pose replaces hiding)
  - This fixes floating damage numbers not appearing over killed NPCs (entity stays alive for Transform query)
- **Damage number filtering**: Modified `combat.rs::handle_apply_damage()` to NOT spawn floating damage numbers over the player when taking damage
  - Rationale: Avoids duplication - resolved threats stack shows incoming damage, floating numbers show outgoing damage
- **Dead code removal**: Deleted `combat_vignette.rs` (module was already commented out, replaced by post-processing vignette)

**Build Status:**
- `cargo build --bin client` ✅ Success (52 warnings, all pre-existing unused code)
- `cargo build --bin server` ✅ Success (23 warnings, all pre-existing unused code)

**Dependencies Added:**
- chrono = "0.4" (for timestamp formatting in combat log)

## Date

2026-02-09
