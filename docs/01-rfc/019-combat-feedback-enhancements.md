# RFC-019: Combat Feedback Enhancements

## Status

**Accepted** - 2026-02-09

## Feature Request

### Player Need

From player perspective: **I need to understand what's happening in combat without rewinding a recording** - Currently, when damage numbers fade after 2 seconds, the combat event is lost from memory. There's no way to review "what just hit me?" or "did that threat resolve?" without perfect attention span.

**Current Problem:**

Without persistent combat feedback:
- Threats disappear from queue when they resolve, with no indication they caused damage
- Damage numbers fade after 2 seconds — events are forgotten immediately
- No combat timeline or history to review after a fight
- Hard to debug combat mechanics ("did Counter work?" requires perfect attention)
- Player confusion: "Why did I just take 60 damage?" with no visible source

**We need a system that:**
- Shows which threats just resolved (immediate feedback, not instant disappearance)
- Maintains a combat history/timeline for review
- Links queue threats to damage application visually
- Helps debug combat mechanics (verifiable timeline)
- Uses minimal screen space and performance

### Desired Experience

Players should experience:
- **Immediate clarity:** When a threat resolves, see it persist for 4 seconds below the queue with damage amount
- **Historical context:** Scroll the combat log to review "what happened 10 seconds ago?"
- **Cause and effect:** Visual link between queue threat → resolved threat → damage number
- **Debugging confidence:** "Did Deflect clear all 3 threats?" → Check log for 3 clear events
- **Performance:** 60 FPS maintained even with rapid damage (10+ hits/sec)

### Specification Requirements

**Resolved Threats Stack:**
- Circular entries below threat queue (matches threat icon style)
- Shows last 5 resolved threats (oldest despawns when 6th arrives)
- Each entry displays damage amount
- Fade out over 4 seconds (manual alpha animation)
- Positioned center-top, below threat queue icons

**Combat Log:**
- Scrollable panel in bottom-left corner (400×250px)
- Shows last 50 combat events (FIFO)
- Timestamped entries with format: `[HH:MM:SS] Source → Target: 45 dmg (⚔ Physical)`
- Color-coded by event type:
  - Red: Damage dealt (player is source)
  - Orange: Damage taken (player is target)
  - Gray: Dodges/clears
- Auto-scrolls to bottom on new entries
- No network overhead (client-side only, uses existing events)

### MVP Scope

**Phase 1 includes:**
- Resolved threats stack with circular entries and fade-out
- Combat log panel with damage event logging
- Max 5 resolved threats, max 50 log entries
- Color coding for damage dealt vs taken
- Timestamp formatting

**Phase 1 excludes:**
- Entity name caching (use "Unknown" for despawned entities)
- Damage type emojis (⚔ Physical, 🔥 Magic) — show as text
- Log filtering/search
- Manual scrolling (future enhancement)
- Settings to toggle visibility
- Heal events (not yet implemented in game)

### Priority Justification

**MEDIUM PRIORITY** - Combat works without this, but player understanding and debugging suffer. Critical for combat testing and validation.

## Context

**Related Systems:**
- **ADR-014:** Combat HUD Layered Architecture (establishes visual language)
- **Threat Queue Icons:** Shows pending threats with timer rings
- **FloatingText:** Damage numbers that fade after 2 seconds
- **Event System:** `Do::ApplyDamage`, `Do::ClearQueue` already exist

**Design Constraints:**
- Must follow ADR-014 visual language (circles for threats, colors for states)
- Must use existing event system (no new network messages)
- Must maintain 60 FPS with max entries (5 resolved threats + 50 log entries)
- Must work with despawned entities (source may be dead when logging)

## Alternatives Considered

**Alternative 1: Extended Floating Text**
- Make damage numbers linger 10 seconds instead of 2 seconds
- ❌ Clutters screen with too much text
- ❌ Still no historical review
- ❌ Hard to read during fast combat

**Alternative 2: Single Combat Log Only (No Resolved Threats)**
- Just add the combat log panel
- ❌ No immediate spatial feedback
- ❌ Requires looking away from action
- ✅ Simpler, but less useful

**Alternative 3: Replay System**
- Record combat, allow timeline scrubbing
- ❌ Massive scope (recording, playback UI)
- ❌ Overkill for MVP feedback needs
- ⏸️ Consider post-MVP

**Alternative 4: On-Hover Tooltips**
- Hover over threat icons to see "will deal 45 damage"
- ❌ Doesn't show resolved threats (past events)
- ❌ Requires mouse cursor (action combat)
- ❌ Doesn't help with history

**Selected: Resolved Threats Stack + Combat Log**
- Complementary features (immediate + historical)
- Minimal scope, follows existing patterns
- Zero network overhead
- Solves both feedback problems

## Success Criteria

**Functional:**
- Resolved threat entries appear when damage applies
- Entries fade over 4 seconds and despawn
- Max 5 entries enforced (6th despawns oldest)
- Combat log shows timestamped damage events
- Max 50 log entries enforced (51st despawns oldest)
- Color coding works (red=dealt, orange=taken, gray=clear)

**Visual:**
- Resolved threats positioned below threat queue (centered)
- Circular entries match threat icon style
- Fade-out smooth and completes in 4 seconds
- Combat log readable in bottom-left corner

**Performance:**
- 60 FPS with 5 resolved threats + 50 log entries
- No frame drops with rapid damage (10+ hits/sec)
- Memory stable (no leaks)

**UX:**
- Players can glance at resolved threats and see recent damage
- Players can scroll log to review combat history
- Visual link between queue threats and resolved threats is clear

## References

- **ADR-014:** Combat HUD Layered Architecture
- **Threat Icons System:** `src/client/systems/threat_icons.rs`
- **FloatingText Pattern:** `src/client/systems/combat_ui.rs:5-41`
- **Event System:** `src/common/message.rs` (Do::ApplyDamage, Do::ClearQueue)

## Date

2026-02-09
