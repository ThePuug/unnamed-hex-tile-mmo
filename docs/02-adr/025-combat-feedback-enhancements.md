# ADR-025: Combat Feedback Enhancements (Resolved Threats + Combat Log)

## Status

**Accepted** - 2026-02-09

## Context

**Related RFC:** [RFC-019: Combat Feedback Enhancements](../01-rfc/019-combat-feedback-enhancements.md)
**Related ADR:** [ADR-014: Combat HUD Layered Architecture](014-combat-hud-layered-architecture.md)

The current combat system provides threat queue visualization (circular icons with timer rings) and floating damage numbers when damage is applied. However, there's a critical gap in combat feedback:

1. **No visual link between queue resolution and damage** — When a threat timer expires, the threat icon disappears and damage numbers appear, but there's no persistent indication that "this threat just resolved and caused this damage"
2. **No combat history** — Once damage numbers fade (2 seconds), the event is lost. Players can't review what happened 5 seconds ago
3. **Debugging difficulty** — Without a timeline of events, it's hard to verify combat mechanics are working correctly

### Requirements

- **Immediate feedback:** Show which threats just resolved before they disappear from memory
- **Historical context:** Maintain a scrollable log of recent combat events
- **Minimal overhead:** Keep performance impact under 0.5ms/frame (60 FPS target)
- **Visual consistency:** Follow ADR-014's visual language (circles for threats, colors for states)
- **Event-driven:** Use existing `Do::ApplyDamage` and `Do::ClearQueue` events

### Options Considered

**Option 1: Resolved Threats Stack + Combat Log** ✅ **SELECTED**
- Resolved threats stack: Persistent circular entries below threat queue, fade over 4s
- Combat log: Scrollable text panel in bottom-left corner, last 50 events
- Both features use existing event system, no new network messages
- Follows FloatingText pattern (manual alpha fade, timed despawn)

**Option 2: Extended Floating Text**
- Make damage numbers linger longer (10s instead of 2s)
- Add more detailed text (source name, damage type)
- ❌ Clutters the screen with too much text
- ❌ No historical review (still disappears)
- ❌ Hard to read during fast combat

**Option 3: Single Combat Log Only**
- Just add the combat log panel, skip resolved threats stack
- ❌ Doesn't provide immediate spatial feedback
- ❌ Requires looking away from action to see what happened
- ✅ Simpler implementation, but less useful

**Option 4: Replay System**
- Record combat events, allow scrubbing timeline
- ❌ Massive scope increase (recording, playback UI)
- ❌ Not needed for MVP feedback
- ⏸️ Consider for post-MVP if needed

## Decision

**Add two complementary combat feedback features using existing event system and ADR-014 patterns.**

### Core Mechanism

**Feature 1: Resolved Threats Stack**

A vertical stack of circular entries below the threat queue that shows recently resolved threats for 4 seconds before fading away.

**Visual Specs:**
```
Position:     Center-top, below threat queue (~70px down)
Layout:       Vertical flex column, 3px gap
Entry size:   30px diameter circular
Border:       2px red, fades over 4s
Background:   Dark red (0.3, 0.1, 0.1), semi-transparent, fades over 4s
Content:      Damage number in white text
Max entries:  5 (oldest despawns when 6th added)
Lifetime:     4.0 seconds (full fade-out)
```

**Trigger:** `Do::ApplyDamage` event
- Spawns entry when damage is applied
- Enforces max 5 entries (FIFO despawn)
- Uses manual alpha fade (same pattern as FloatingText in combat_ui.rs:5-41)

**Feature 2: Combat Log**

A scrollable panel in the bottom-left corner showing the last 50 combat events with timestamps.

**Visual Specs:**
```
Position:     Bottom-left corner (10px margins)
Size:         400px wide × 250px tall
Background:   Dark semi-transparent (0.1, 0.1, 0.1, 0.85)
Border:       Gray (0.5, 0.5, 0.5, 0.8)
Font size:    12px
Padding:      10px
Overflow:     Clip Y (vertical scroll)
Max entries:  50 (FIFO despawn)
```

**Entry Format:**
```
[HH:MM:SS] Source → Target: 45 dmg (⚔ Physical)
```

**Color Coding:**
- Damage dealt (player is source): Red `(1.0, 0.3, 0.3)`
- Damage taken (player is target): Orange `(1.0, 0.6, 0.0)`
- Dodges/clears: Gray `(0.6, 0.6, 0.6)`

**Triggers:**
- `Do::ApplyDamage` → Log damage event
- `Do::ClearQueue` → Log dodge/deflect event

### Component Design

**New Components** (`src/client/components/mod.rs`):
```rust
/// Resolved threat entry - fades out after showing damage resolution
#[derive(Component)]
pub struct ResolvedThreatEntry {
    pub spawn_time: Duration,
    pub lifetime: f32,  // 4.0 seconds
    pub damage: f32,
}

/// Marker for resolved threats container
#[derive(Component)]
pub struct ResolvedThreatsContainer;

/// Marker for combat log panel
#[derive(Component)]
pub struct CombatLogPanel;

/// Marker for combat log content (scrollable)
#[derive(Component)]
pub struct CombatLogContent;

/// Combat log entry with metadata for color coding
#[derive(Component)]
pub struct CombatLogEntry {
    pub timestamp: String,  // Pre-formatted "HH:MM:SS"
    pub is_player_damage: bool,  // true = dealt, false = taken
}
```

### System Design

**New Files:**
- `src/client/systems/resolved_threats.rs`
- `src/client/systems/combat_log.rs`

**Resolved Threats Systems:**
1. `setup()` — Spawn persistent container below threat queue
2. `on_damage_resolved()` — Listen to ApplyDamage events, spawn entries
3. `update_entries()` — Fade out entries, despawn when expired

**Combat Log Systems:**
1. `setup()` — Spawn log panel in bottom-left
2. `on_damage_applied()` — Log damage events with source/target/amount
3. `on_queue_cleared()` — Log dodge/deflect events
4. `maintain_log()` — Enforce max 50 entries, auto-scroll

## Rationale

### 1. Two Features Are Complementary

**Resolved Threats Stack = Immediate Feedback:**
- Shows "this threat just resolved" in player's peripheral vision
- Spatial context (below queue where threat was)
- 4-second lifetime long enough to glance and register
- Matches ADR-014 visual language (circles for threats)

**Combat Log = Historical Context:**
- Permanent record of last 50 events
- Timestamp allows correlating events ("what happened at 12:34:56?")
- Color coding distinguishes dealt vs taken damage
- Scrollable for review after combat

Neither feature alone provides both immediate + historical feedback.

### 2. Follows Existing Patterns

**FloatingText Pattern (combat_ui.rs:5-41):**
```rust
let elapsed = (time.elapsed() - entry.spawn_time).as_secs_f32();
let alpha = 1.0 - (elapsed / entry.lifetime).clamp(0.0, 1.0);
border_color.0 = border_color.0.with_alpha(alpha);
if elapsed >= entry.lifetime {
    commands.entity(entity).despawn();
}
```

Resolved threats stack uses identical manual fade-out:
- No complex animations or bevy_easings dependency
- Simple, deterministic, proven pattern
- <0.1ms overhead per entry

**Event-Driven (threat_icons.rs:289-303):**
- Both features listen to existing `Do` events
- No new network messages required
- Client-side only (zero server impact)

### 3. Minimal Performance Overhead

**Resolved Threats Stack:**
- Max 5 entries × ~200 bytes = 1KB memory
- 5 alpha calculations per frame = <0.1ms
- FIFO despawn prevents unbounded growth

**Combat Log:**
- Max 50 entries × ~150 bytes = 7.5KB memory
- Append-only updates (no full redraw)
- Text rendering cached by Bevy
- Estimated <0.2ms per frame

**Total: <0.3ms/frame** (well under 0.5ms budget)

### 4. Debugging Value

**Combat Mechanics Verification:**
- "Did Counter negate the threat?" → Check log
- "How much damage did I take vs deal?" → Compare red vs orange entries
- "Did Deflect clear all 3 threats?" → Log shows 3 clear events

**Player Understanding:**
- "Why did I just take 60 damage?" → See resolved threat with 60 dmg
- "Where did that damage come from?" → Log shows source entity
- "What happened during that fight?" → Scroll log to review

## Consequences

### Positive

**1. Immediate Visual Link (Queue → Damage)**
- Resolved threat appears below queue when damage applies
- Player sees "this threat caused this damage" without guessing
- 4-second lifetime gives time to glance and register

**2. Combat History for Review**
- 50 events × 2 sec avg = ~100 seconds of history
- Timestamps allow precise correlation
- Color coding makes dealt vs taken damage obvious

**3. Zero Network Overhead**
- Client-side only (uses existing events)
- No new message types, no server changes
- Just UI rendering and event listeners

**4. Extensible for Future Features**
- Healing events → Green entries
- Status effects → Gray entries with effect name
- Ability usage → Blue entries
- All follow same pattern (timestamp + color + text)

**5. Follows ADR-014 Visual Language**
- Resolved threats = circles (matches threat queue)
- Red color for damage (consistent with health bars)
- Screen-space overlay (doesn't occlude world)

### Negative

**1. Screen Real Estate Usage**
- Resolved threats stack: ~160px vertical space (5 entries × 30px + gaps)
- Combat log panel: 400px × 250px in bottom-left corner
- May feel cluttered on smaller screens

**Mitigation:** Both features can be toggled off via settings (future). Resolved threats auto-disappear after 4s. Log is non-intrusive (bottom-left corner).

**2. Text Rendering Performance**
- 50 log entries with timestamps = dynamic text
- Text rendering expensive in Bevy (texture atlas updates)

**Mitigation:** Use monospace font for timestamps (easier caching). Only update log on new events (not every frame). Bevy batches text efficiently.

**3. Entity Name Resolution Complexity**
- Damage events have `Entity` source/target, not names
- Need to query `EntityType` component and map to display names
- Despawned entities → "Unknown" (can't query)

**Mitigation:** Store entity names in event handler scope. Fall back to "Unknown" if entity despawned. Future: cache entity names in resource.

**4. Timestamp Synchronization**
- Client uses local time for timestamps (not synced server time)
- Events may appear out-of-order if latency spikes
- Timestamps not comparable across clients

**Mitigation:** Timestamps are for relative comparison within one client session. Acceptable for MVP. Future: use server.current_time() for synced timestamps.

### Neutral

**1. Scrollable UI Complexity**
- Bevy UI `Overflow::ClipY` provides basic scrolling
- No scroll bar (future enhancement)
- Manual scroll with mouse wheel or drag (future enhancement)

**2. Max Capacity Trade-offs**
- Resolved threats: 5 entries → Handles normal combat (1-3 threats at once)
- Combat log: 50 entries → ~100 seconds of history
- If combat is faster (5+ hits/sec), log fills quickly

Acceptable for MVP. Monitor real usage and adjust if needed.

## Implementation Notes

**File Structure:**
```
src/client/components/mod.rs          # Add 5 new component definitions
src/client/systems/resolved_threats.rs # 3 systems (setup, on_damage, update)
src/client/systems/combat_log.rs      # 4 systems (setup, on_damage, on_clear, maintain)
src/client/systems/mod.rs             # Export new modules
src/client/plugins/ui.rs              # Register 7 new systems
```

**Integration Points:**
- Events: `Do::ApplyDamage`, `Do::ClearQueue` (existing in common/message.rs)
- Queries: `EntityType`, `Actor` (existing components)
- Resources: `Server` (for current_time), `Time` (for elapsed time)
- Pattern: `FloatingText` (combat_ui.rs:5-41 for fade-out reference)

**System Registration:**
```rust
// In Startup schedule (after camera::setup)
resolved_threats::setup.after(crate::client::systems::camera::setup),
combat_log::setup.after(crate::client::systems::camera::setup),

// In Update schedule
resolved_threats::on_damage_resolved,
resolved_threats::update_entries,
combat_log::on_damage_applied,
combat_log::on_queue_cleared,
combat_log::maintain_log,
```

## Validation Criteria

**Functional:**
- Resolved threat entries appear when damage is applied
- Entries fade out over 4 seconds and despawn
- Max 5 entries enforced (6th spawns, oldest despawns)
- Combat log shows damage events with correct timestamps
- Combat log shows clear events in gray
- Max 50 log entries enforced (51st spawns, oldest despawns)

**Visual:**
- Resolved threats positioned below threat queue (centered)
- Circular entries match threat queue style (circles, red border)
- Fade-out is smooth and completes in exactly 4 seconds
- Combat log positioned in bottom-left corner
- Color coding works (red=dealt, orange=taken, gray=clears)

**Performance:**
- 60 FPS maintained with 5 resolved threats + 50 log entries
- No frame drops when spawning entries rapidly (10+ hits/sec)
- Memory stable (no leaks from despawned entities)

**Edge Cases:**
- Despawned source entity → "Unknown" in log
- Rapid damage (>10 hits/sec) → oldest entries despawn correctly
- Empty queue + ClearQueue event → no log entry (nothing to clear)

## References

- **ADR-014:** Combat HUD Layered Architecture (visual language, component patterns)
- **combat_ui.rs:5-41** FloatingText fade pattern (alpha calculation, despawn logic)
- **threat_icons.rs:289-303** Event handling pattern (MessageReader, Do events)
- **common/message.rs** Event definitions (Do::ApplyDamage, Do::ClearQueue)

## Date

2026-02-09
