# RFC-007: Contextual Developer Console

## Status

**Implemented** - 2025-10-30

## Feature Request

### Player Need

From developer perspective: **Discoverable, organized debug tools** - No hunting through source code for keybindings.

**Current Problem:**
Without developer console:
- Debug features scattered across keyboard (J, H, G, V, F3, Digit1-3)
- No discoverability (must read source to find features)
- Key conflict risk (may clash with gameplay keys)
- No organization (features not grouped logically)
- Testing workflows inefficient (remember multiple keys)

**We need a system that:**
- Shows all available debug commands in visual menu
- Groups features logically (Terrain, Combat, Performance, etc.)
- Navigable with one hand (numpad only)
- Doesn't interrupt gameplay (overlay, not pause)
- Extensible (easy to add new features)

### Desired Experience

Developers should experience:
- **Discoverability:** Find any debug feature within 30 seconds (no docs)
- **Organization:** Features grouped by category (Terrain/Combat/Performance)
- **Quick access:** Single key opens console (NumpadDivide)
- **Clear feedback:** Current state visible for all toggles (ON/OFF)
- **No interruption:** Console overlay while gameplay continues

### Specification Requirements

**MVP Console:**
- Hierarchical menu (Root → 5 sub-menus)
- Numpad navigation (0-9, /)
- All existing debug features accessible:
  - Terrain: Grid overlay (J), Slope rendering (H), Fixed lighting (G)
  - Combat: Queue damage (Digit1), Drain stamina (Digit2), Drain mana (Digit3)
  - Performance: Performance UI (F3)
  - Visualization: Spawner markers (V)
- Current state indicators for toggles
- Breadcrumb navigation (shows menu path)

**Menu Structure:**
```
Root Menu:
  1. Terrain      [Slope: ON, Grid: OFF, Light: Fixed]
  2. Combat       [Test tools and cheats]
  3. Performance  [FPS: ON, Profiling: OFF]
  4. Visualization[Spawners: ON]
  5. Tools        [Camera, Spawn, Teleport - future]
  0. Close Console

Terrain Menu:
  1. Toggle Grid Overlay      [OFF]
  2. Toggle Slope Rendering   [ON]
  3. Toggle Fixed Lighting    [Fixed 9AM]
  4. Regenerate Mesh          [Action]
  0. Back to Main Menu
```

### MVP Scope

**Phase 1 includes:**
- Console UI panel (Bevy UI overlay)
- Root menu + 5 sub-menus
- Numpad navigation (NumpadDivide opens, 0-9 navigates)
- All existing debug features accessible via console
- Current state indicators
- Breadcrumb navigation
- Dual access (existing keybindings still work)

**Phase 1 excludes:**
- Text command input (type `/toggle grid`)
- Command history/autocomplete
- Mouse interaction
- Keybinding customization
- Console log output

### Priority Justification

**NICE-TO-HAVE** - Improves developer experience but doesn't block gameplay features. Current scattered keybindings functional, just less discoverable.

**Benefits:**
- Faster onboarding (new developers find features)
- Reduced key conflicts (frees J/H/G/V for gameplay)
- Better testing workflows (organized by category)

---

## Feasibility Analysis

### Technical Assessment

**Proposed Solution: Hierarchical Numpad-Navigable Menu with Event-Based Actions**

#### Core Mechanism

**Console State:**
```rust
pub struct DevConsole {
    pub visible: bool,
    pub current_menu: MenuPath,
    pub history: Vec<MenuPath>,  // Breadcrumb
}
```

**Menu Navigation:**
- NumpadDivide: Open/close console
- Numpad 1-9: Select menu option (navigate sub-menu or execute action)
- Numpad 0: Back to previous menu (or close if at root)

**Action Events:**
```rust
pub enum DevConsoleAction {
    ToggleGrid,
    ToggleSlopeRendering,
    QueueDamageThreat,
    DrainStamina,
    // ... others
}
```

**Benefits:**
- Discoverable (visual menu shows all options)
- Organized (features grouped by category)
- One-handed (numpad only, no mouse)
- Extensible (add action event, add menu item)

#### Performance Projections

**UI Rendering:**
- Console visible: < 1ms frame time increase
- Menu rebuilding: < 0.5ms per state change
- Memory: < 10KB (10-20 text entities)

**Development Time:**
- Phase 1 (MVP): 7 days
- Phase 2 (text commands): 5 days
- Phase 3 (advanced features): 10-15 days

#### Technical Risks

**1. Numpad Requirement**
- *Risk:* Laptops without numpad can't use console
- *Mitigation:* Phase 2 allows rebinding, text commands don't need numpad
- *Frequency:* Developers can use desktop keyboards

**2. Navigation vs Direct Keybindings (Speed)**
- *Risk:* Console = 3 keypresses (/, 1, 1), direct = 1 keypress (J)
- *Mitigation:* Keep frequent toggles as direct keys initially, Phase 2 text commands fast
- *Impact:* Acceptable for dev tool (not hot path)

**3. Two Ways to Do Same Thing**
- *Risk:* Confusing (console vs direct keys)
- *Mitigation:* Document console as preferred, Phase 2 deprecates direct keys

### System Integration

**Affected Systems:**
- Diagnostics (grid, slope, lighting toggles)
- Debug resources (stamina/mana drains)
- Performance UI (FPS overlay)
- Spawner visualization (cyan markers)

**Compatibility:**
- ✅ Reads existing state resources (DiagnosticsState)
- ✅ Writes to same resources as direct keybindings
- ✅ No duplication of state
- ✅ Event system decouples input from execution

### Alternatives Considered

#### Alternative 1: Flat Command List

Single menu with all 20+ commands (no hierarchy).

**Rejected because:**
- Poor organization (all features in one long list)
- Harder to find specific feature (scan entire list)
- Doesn't scale (100+ commands would be unwieldy)

#### Alternative 2: Text Commands Only

Type `/toggle grid` instead of navigating menus.

**Rejected for MVP because:**
- Harder to discover (must know command names)
- Requires text input implementation (more complex)
- No visual feedback until executed
- Deferred to Phase 2 (menu + text hybrid)

#### Alternative 3: Mouse-Based UI

Click buttons instead of numpad navigation.

**Rejected because:**
- Requires hand movement (slower than one-handed numpad)
- Interrupts gameplay flow (must grab mouse)
- Doesn't match "developer tool" aesthetic (prefer keyboard)

#### Alternative 4: egui for Rendering

Use egui library instead of Bevy UI.

**Rejected for MVP because:**
- Separate rendering pipeline (more complexity)
- Larger dependency
- Bevy UI sufficient for simple menus
- Reconsider for Phase 2 (text input might benefit from egui)

---

## Discussion

### ARCHITECT Notes

**Key Architectural Insight:** Event-based action system decouples navigation from execution, enabling future text commands without rewriting action handlers.

**Extensibility:**
- Add menu item: Update menu builder function
- Add action: Add enum variant, add handler case
- No changes to navigation system

**Future enhancements:**
- Phase 2: Text commands (type instead of navigate)
- Phase 3: Macros, scripting, remote console

### PLAYER Validation

N/A - This is a developer tool, not a player-facing feature.

**Developer UX Requirements:**
- ✅ Find any feature within 30 seconds
- ✅ Clear state indicators (ON/OFF visible)
- ✅ Quick toggle (single keypress to open)
- ✅ Organized categories (Terrain, Combat, etc.)

**Acceptance Criteria:**
- ✅ New developer can toggle grid without reading docs
- ✅ All existing features accessible via console
- ✅ Console doesn't pause gameplay (overlay)
- ✅ Navigation intuitive (breadcrumbs show location)

---

## Approval

**Status:** Approved for implementation

**Approvers:**
- ARCHITECT: ✅ Clean design, extensible, improves DX

**Scope Constraint:** Fits in one SOW (7 days for MVP)

**Dependencies:**
- None (uses existing debug features, adds UI layer)

**Next Steps:**
1. ARCHITECT creates ADR-013 documenting console architecture
2. ARCHITECT creates SOW-007 with implementation plan
3. DEVELOPER begins Phase 1 (console resources and UI)

**Date:** 2025-10-30
