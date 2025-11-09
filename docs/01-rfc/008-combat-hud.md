# RFC-008: Combat HUD

## Status

**Approved** - 2025-10-31

## Feature Request

### Player Need

From player perspective: **Instant combat understanding** - Always know who you're fighting, what's coming at you, and what you can do about it.

**Current Problem:**
Without complete combat HUD:
- Resource pools exist but no clear visual hierarchy
- Reaction queue exists but looks generic (could be mistaken for ability slots)
- No action bar (Q/W/E/R abilities invisible until pressed)
- No target detail frame (can't see enemy resources/queue for decision-making)
- No facing indicator (heading unclear after movement stops)
- World-space health bars missing (hard to assess enemy HP)
- No combat state indicator (unclear when in/out of combat)

**We need a system that:**
- Shows all combat state at a glance (resources, threats, abilities, targets)
- Uses distinct visual languages for different information types
- Provides instant feedback (no manual reading required)
- Enables tactical decision-making (see enemy state, make informed choices)
- Scales from basic enemies (HP only) to smart enemies (full state)

### Desired Experience

Players should experience:
- **Clarity:** Understand combat state in < 1 second glance
- **Distinct Systems:** Never confuse threats (circular) with abilities (rectangular)
- **Tactical Info:** See enemy resources/queue to make strategic decisions
- **Action Readiness:** Know which abilities are ready before pressing keys
- **Urgency Awareness:** Instantly recognize critical situations (queue full, timer expiring)

### Specification Requirements

**MVP HUD (Phase 1 Critical from combat-hud.md):**

**1. Resource Bars (Bottom-Center):**
- Display Health/Stamina/Mana for local player
- Current/max values visible
- Color-coded (Red/Yellow/Blue)

**2. Action Bar (Bottom-Center, Below Resources):**
- Display Q/W/E/R abilities with keybinds
- Show resource costs, cooldowns, ready states
- State indicators (ready, cooldown, can't afford, out of range, no target)

**3. Reaction Queue Display (Top-Center):**
- Circular threat indicators with timer rings
- Attack type icons (⚔️ physical, 🔥 magic)
- Countdown timers (0.8s, 0.4s)
- Urgency indicators (pulsing when < 20% remaining)

**4. Target Indicators (World-Space):**
- Red circle on hostile target
- Green circle on ally target (when applicable)
- Tier lock badges (1/2/3)
- TAB lock markers

**5. Target Detail Frame (Top-Right):**
- Entity name + distance in hexes
- Exact HP numbers (80/100)
- Resource pools (if entity has them)
- Threat queue (if entity has one)
- Auto-show when target exists

**6. World-Space Health Bars (Above Entities):**
- Display Health on all entities in combat
- Show when in combat OR damaged
- Horizontal bars, orange/yellow fill

**7. Facing Indicator:**
- Arrow sprite above character pointing in heading direction
- Character position offset toward facing (subtle)
- Optional facing cone overlay (toggleable)

**8. Combat State Feedback:**
- Screen vignette when in combat (dark red edges)
- Resource bar glow when in combat (orange outline)

### MVP Scope

**Phase 1 includes:**
- All 8 UI systems (resource bars, action bar, queue, targets, health bars, facing, combat state)
- Layered architecture (world-space, screen-space, floating)
- Consistent visual language (circles=threats, rectangles=abilities)
- Client prediction for local player (instant feedback)
- Server-authoritative for remote entities (accuracy)

**Phase 1 excludes:**
- Status effects display (no buffs/debuffs yet)
- Damage numbers (handled by ADR-005)
- Ability tooltips (future enhancement)
- Colorblind modes (accessibility polish)
- UI customization/scaling (settings)

### Priority Justification

**CRITICAL** - Blocks MVP combat playability. Without action bar and target frame, combat is guesswork.

**Why critical:**
- Action bar: Players don't know which abilities are ready, what they cost, or if they're in range
- Target frame: Can't see enemy resources/queue for tactical decisions
- Facing indicator: Directional targeting unclear without visual heading
- Reaction queue redesign: Current boxes look generic, need distinct "threat" identity

**Benefits:**
- Tactical depth (see enemy state → make informed decisions)
- Clarity (never confused about combat state)
- Discoverability (abilities visible, not hidden behind keypresses)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Layered UI Architecture with Consistent Visual Language**

#### Core Mechanism

**Three Visual Layers:**

1. **World-Space UI:**
   - Health bars (above entities)
   - Target indicators (ground circles)
   - Parented to entity Transform

2. **Screen-Space Overlay:**
   - Resource bars (bottom-center)
   - Action bar (bottom-center, below resources)
   - Reaction queue (top-center)
   - Target detail frame (top-right)
   - Fixed screen positions

3. **Floating UI:**
   - Damage numbers (existing from ADR-005)
   - World-space but ephemeral

**Visual Language Consistency:**

**Shapes Communicate Purpose:**
- **Circular:** Threats, timers, urgent depleting resources
- **Rectangular:** Abilities, stable resources
- **Icons:** Attack types, ability types

**Colors Communicate State:**
- **Red:** Health, damage, danger, hostile targets
- **Yellow/Orange:** Stamina, warnings
- **Blue:** Mana, information
- **Green:** Healing, buffs, ally targets
- **Gray:** Inactive, disabled

**Client Prediction Strategy:**
- Resource bars: Use `step` for local player (predicted)
- Reaction queue: Client maintains queue state, runs timer countdown
- Health bars: `step` for local player, `state` for remote entities
- Target frame: Server-authoritative, client displays

#### Performance Projections

**UI Rendering:**
- Resource bars: 3 sprites + 3 text labels (low cost)
- Action bar: 4 slots × 5 elements = 20 UI elements
- Reaction queue: Max 3 circles × 4 elements = 12 UI elements
- Target indicators: 1-2 sprites (only active targets)
- Target frame: 1 container + variable content (0-50 elements)
- Health bars: N entities × 2 sprites (only visible in combat/damaged)
- **Total:** ~30-80 UI elements in typical combat (negligible for Bevy)

**Development Time:**
- Phase 1 (MVP): 17-23 days (8 phases)

#### Technical Risks

**1. World-Space UI Scaling**
- *Risk:* Health bars/indicators scale with camera zoom (too small/large)
- *Mitigation:* Orthographic camera, clamp scale, test at min/max zoom
- *Frequency:* One-time adjustment per zoom level

**2. Text Rendering Performance**
- *Risk:* Text updates expensive in Bevy (resource labels, timers, countdown)
- *Mitigation:* Bitmap fonts, dirty checking (only update when value changes)
- *Impact:* Acceptable for MVP, optimize if profiling shows issue

**3. UI Entity Overhead**
- *Risk:* Health bars on 20 NPCs = 40 sprites (spawning/despawning)
- *Mitigation:* Only show when in combat/damaged, object pooling
- *Impact:* Reduces count by 60-80%

**4. Color Accessibility**
- *Risk:* Red/green indicators problematic for colorblind players
- *Mitigation:* Combine color + shape (circles for targets) + icons (⚔️ for threats)
- *Future:* Colorblind mode (change colors, keep shapes/icons)

### System Integration

**Affected Systems:**
- Resource management (Health/Stamina/Mana components from ADR-002)
- Reaction queue (ReactionQueue component from ADR-003)
- Targeting (Target component from ADR-004)
- GCD tracking (Gcd component from ADR-011)
- Heading (Heading component from ADR-009)

**Compatibility:**
- ✅ Queries existing components (no new backend needed)
- ✅ Client-side only (no network impact)
- ✅ Uses existing prediction patterns (Offset, step)
- ✅ Extends ADR-005 floating text system (damage numbers)

### Alternatives Considered

#### Alternative 1: Single-Layer UI (All Screen-Space)

Render health bars, target indicators as screen-space UI (not world-space).

**Rejected because:**
- Harder to associate health bars with entities (spatial disconnect)
- Manual positioning for screen-space projection (complex)
- Doesn't scale to multiple entities (which bar is which enemy?)

#### Alternative 2: Different Visual Language (All Rectangular)

Use rectangular slots for both threats and abilities.

**Rejected because:**
- No instant visual differentiation (confusion)
- Combat-hud.md spec explicitly requires distinct shapes
- Genre conventions use circles for timers (MOBA standard)

#### Alternative 3: Target Frame Only Shows When Locked

Require tier lock (1/2/3) or TAB lock to see target detail frame.

**Rejected because:**
- Extra input friction (press key → see info)
- Hides tactical information (enemy resources/queue)
- Combat-hud.md spec: "Always show when target exists"
- Reduces decision-making quality (info behind lock)

#### Alternative 4: Bevy EGUI for All UI

Use bevy_egui instead of Bevy UI for entire HUD.

**Rejected for MVP because:**
- Separate rendering pipeline (more complexity)
- Bevy UI sufficient for simple bars/circles/text
- Reconsider for Phase 2 (text input, tooltips, complex layouts)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Three-layer system separates concerns cleanly. World-space for entity context, screen-space for player state, floating for events.

**Visual language critical:** Shapes must communicate purpose instantly. Circular = urgent/depleting, rectangular = stable/actions. Genre conventions support this (MOBAs, MMOs use circles for cooldowns/timers).

**Target frame always-visible:** Simplifies UX. "Current target = current interest" - if you're targeting them, you want their details. No hidden information, no lock friction.

**Extensibility:**
- Future: Status effects (icons above health bars, same circular style)
- Future: Minimap (top-left, doesn't conflict)
- Future: Party frames (left side, same resource bar style)

### PLAYER Validation

**From combat-hud.md spec:**

**Success Criteria (Phase 1 Critical):**
- ✅ Always know which enemy I'm targeting (red indicator unambiguous)
- ✅ Can see incoming threats clearly (reaction queue visually distinct)
- ✅ Know which direction I'm facing (arrow + position offset)
- ✅ Know what abilities I can use (action bar shows keybinds, costs, cooldowns, range)
- ✅ Can see enemy health state (bars show damage progress)
- ✅ Can see detailed target information (frame shows resources, queue for decisions)
- ✅ Understand when I'm in combat (visual/audio cues)
- ✅ Can manage resources (stamina/health/mana visible and readable)

**Visual Hierarchy Validated:**
- Top-center (threats): Urgency demands prominence, "danger zone" mentally
- Bottom-center (abilities + resources): Scanning pattern (what's ready? → can I afford? → execute)
- Top-right (target details): Out of combat area, doesn't block center action
- World-space (health bars): Spatial association, instant entity connection

**Shape Language Validated:**
- Circles = threats (reaction needed, time pressure, incoming danger)
- Rectangles = abilities (player actions, deliberate choices)
- Prevents confusion, carries throughout HUD

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Clean layering, visual consistency, extensible, integrates well
- PLAYER: ✅ Solves all Phase 1 Critical needs, tactical depth enabled

**Scope Constraint:** Fits in one SOW (17-23 days for 8 phases)

**Dependencies:**
- ADR-002: Health/Stamina/Mana components
- ADR-003: ReactionQueue component
- ADR-004: Target component
- ADR-009: Heading component
- ADR-011: Gcd component

**Next Steps:**
1. ARCHITECT creates ADR-014 documenting layered UI architecture
2. ARCHITECT creates SOW-008 with 8-phase implementation plan
3. DEVELOPER begins Phase 1 (resource bars foundation)

**Date:** 2025-10-31
