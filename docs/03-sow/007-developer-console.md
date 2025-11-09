# SOW-007: Developer Console

## Status

**Accepted** - 2025-10-30

## References

- **RFC-007:** [Contextual Developer Console](../01-rfc/007-developer-console.md)
- **ADR-013:** [Developer Console Architecture](../02-adr/013-developer-console-architecture.md)
- **Branch:** (implementation details from acceptance)
- **Implementation Time:** 7 days

---

## Implementation Plan

### Phase 1: Console Resources and State (1 day)

**Goal:** Foundation types and state management

**Deliverables:**
- `client/plugins/console/mod.rs` - Plugin definition
- `client/plugins/console/state.rs`:
  - `DevConsole` resource with `visible`, `current_menu`, `history`
  - `MenuPath` enum (Root, Terrain, Combat, Performance, Visualization, Tools)
- `client/plugins/console/actions.rs`:
  - `DevConsoleAction` event enum (all action variants)
- Unit tests for state transitions

**Architectural Constraints:**
- Resource (not component) for global state
- History stack for breadcrumb navigation
- Event-based actions (decoupled from navigation)

**Success Criteria:** Resources compile, state transitions tested, events defined

---

### Phase 2: UI Rendering (2 days)

**Goal:** Bevy UI overlay panel with dynamic menu rendering

**Deliverables:**
- `client/plugins/console/ui.rs`:
  - `setup_dev_console` system (creates UI panel)
  - `update_console_menu` system (rebuilds on menu change)
  - Menu builders: `build_root_menu`, `build_terrain_menu`, etc.
- Visual design:
  - Semi-transparent dark background (90% opacity)
  - Blue border and title
  - State indicators: Green [ON], Red [OFF], Yellow [Action]
  - Top-left position (400px wide, dynamic height)

**Architectural Constraints:**
- Bevy UI (not egui) for native rendering
- Dynamic rebuilding (despawn/respawn on menu change)
- Only rebuild when console visible and menu changed
- Visibility::Hidden when closed (not despawned)

**Success Criteria:** Console UI visible, menus render correctly, state indicators accurate

---

### Phase 3: Navigation System (1 day)

**Goal:** Numpad input handling and menu transitions

**Deliverables:**
- `client/plugins/console/navigation.rs`:
  - `handle_console_input` system
  - NumpadDivide: Toggle visibility
  - Numpad 0: Back/close
  - Numpad 1-9: Navigate or execute action
  - Breadcrumb updates (push/pop history)
- Test menu navigation flow

**Architectural Constraints:**
- Input only processed when console visible
- History stack for back navigation
- Root menu: Numpad 0 closes (not back)
- Sub-menus: Numpad 0 goes back

**Success Criteria:** Navigation works correctly, breadcrumbs accurate, no input leaks to gameplay

---

### Phase 4: Action Execution (2 days)

**Goal:** Integrate console actions with existing debug systems

**Deliverables:**
- `client/plugins/console/actions.rs`:
  - `execute_console_actions` system
  - Handler for each `DevConsoleAction` variant
- Integrate with existing systems:
  - Terrain: Toggle grid (J), Toggle slope (H), Toggle lighting (G)
  - Combat: Queue damage (Digit1), Drain stamina (Digit2), Drain mana (Digit3)
  - Performance: Toggle perf UI (F3)
  - Visualization: Toggle spawner markers (V)
- Test each action works identically to direct keybindings

**Architectural Constraints:**
- Shared state (console modifies DiagnosticsState, etc.)
- No state duplication
- Actions identical to direct keybindings (same logic)
- Phase 1: Dual access (console + direct keys both work)

**Success Criteria:** All actions functional, match direct keybindings, state updated correctly

---

### Phase 5: Documentation and Polish (1 day)

**Goal:** User-facing documentation and visual refinements

**Deliverables:**
- Create `docs/dev-console-guide.md`:
  - How to use console (NumpadDivide, navigation)
  - How to add new features (action event + menu item)
  - Examples for each menu
- Update `CLAUDE.md` to mention console
- Add in-game hint: "Press NumpadDivide for dev console" (shown once on startup)
- Visual polish:
  - Monospace font for aligned columns
  - Hover effects (optional, mouse support)
  - Smooth transitions (fade in/out)

**Architectural Constraints:**
- Self-documenting (clear menu labels)
- Minimal external docs needed
- In-game hint dismissible

**Success Criteria:** Documentation complete, in-game hint shows, visual polish applied

---

## Acceptance Criteria

**Functionality:**
- ✅ NumpadDivide toggles console visibility
- ✅ Numpad 0-9 navigate menus, execute actions
- ✅ Breadcrumbs show current menu path
- ✅ State indicators accurate (ON/OFF/Action)
- ✅ All existing debug features accessible via console
- ✅ Direct keybindings still work (dual access)

**UX:**
- ✅ Find any feature within 30 seconds (discoverability)
- ✅ Menu labels understandable without docs
- ✅ Console opens/closes within 1 frame (responsiveness)
- ✅ Doesn't pause gameplay (overlay)

**Performance:**
- ✅ Console visible: < 1ms frame time increase
- ✅ Menu rebuilding: < 0.5ms per state change
- ✅ Memory usage: < 10KB (10-20 text entities)

**Code Quality:**
- ✅ Isolated in `client/plugins/console/` (contained)
- ✅ Event system decoupled (navigation vs execution)
- ✅ Unit tests for state transitions
- ✅ Integration tests for each action

---

## Discussion

### Design Decision: Dual Access (Phase 1)

**Context:** Should direct keybindings (J/H/G/etc.) remain functional?

**Decision:** Phase 1 keeps both (console + direct keys).

**Rationale:**
- Allows gradual migration (test console without breaking workflows)
- Existing developers can continue using direct keys
- New developers use console (better discoverability)
- Phase 2: Deprecate direct keys (force console)

**Impact:** Two ways to do same thing (documented as intentional transition)

---

### Design Decision: Bevy UI vs egui

**Context:** Which UI library for console rendering?

**Decision:** Bevy UI for MVP.

**Rationale:**
- Native to Bevy (no external dependency)
- Sufficient for text menus (simple layout)
- egui overkill for current needs (richer features unused)
- Phase 2: Reconsider for text input (egui text widgets better)

**Impact:** May need migration if Phase 2 requires egui

---

### Design Decision: Menu Depth (2 Levels Max)

**Context:** How deep should menu hierarchy go?

**Decision:** Max 2 levels (Root → Sub-menu).

**Rationale:**
- Simpler navigation (fewer keypresses)
- Easier to remember location (breadcrumb short)
- Sufficient for ~50 features (5 sub-menus × 10 items each)

**Impact:** If >50 features, may need 3 levels or search function

---

### Implementation Note: State Indicators

**Color coding for toggles:**
- Green [ON]: Active state
- Red [OFF]: Inactive state
- Yellow [Action]: One-time action (no state)
- Gray [Future]: Not yet implemented

**Format:**
```
1. Toggle Grid Overlay      [OFF]   ← Red if OFF
2. Toggle Slope Rendering   [ON]    ← Green if ON
3. Queue 20 Damage Threat   [Action]← Yellow for actions
```

---

### Implementation Note: Breadcrumb Navigation

**Breadcrumb format:**
```
Root → Terrain
Root → Combat
Root → Performance → Detailed Stats (if 3 levels)
```

**Updates:**
- Push to history when entering sub-menu
- Pop from history when pressing Numpad 0
- Clear history when closing console

---

### Implementation Note: Future Phases

**Phase 2 (Text Commands):**
- Text input field below menu
- Command parser (`/toggle grid` → `DevConsoleAction::ToggleGrid`)
- Command history (up/down arrows)
- Autocomplete (tab completion)

**Phase 3 (Advanced):**
- Macros (bind multiple commands)
- Scripting (Lua/Rhai for test scenarios)
- Remote console (TCP connection for automation)
- Save/load state (persist favorites)

---

## Acceptance Review

**Review Date:** 2025-10-30
**Reviewer:** ARCHITECT Role
**Decision:** ✅ **ACCEPTED**

### Scope Completion: 100%

**All 5 phases complete:**
- ✅ Phase 1: Console resources and state
- ✅ Phase 2: UI rendering
- ✅ Phase 3: Navigation system
- ✅ Phase 4: Action execution
- ✅ Phase 5: Documentation and polish

### Architectural Compliance

**✅ ADR-013 Specifications:**
- Hierarchical menu (Root → 5 sub-menus)
- Event-based actions (decoupled)
- Bevy UI rendering (semi-transparent overlay)
- Numpad navigation (NumpadDivide, 0-9)
- Dual access (console + direct keys Phase 1)

### UX Validation

**Discoverability:** ✅ Excellent
- New developers find features without reading docs
- Menu labels self-explanatory
- State indicators clear

**Organization:** ✅ Excellent
- Features grouped logically (Terrain, Combat, etc.)
- Max 2 levels deep (easy navigation)
- Breadcrumbs show location

**Performance:** ✅ Excellent
- Console overhead < 1ms (negligible)
- Menu rebuilding fast (< 0.5ms)
- Memory usage minimal (< 10KB)

---

## Conclusion

The developer console implementation provides organized, discoverable access to all debug features.

**Key Achievements:**
- Discoverability: New developers find features without docs
- Organization: Logical grouping by category
- One-handed: Numpad-only navigation
- Extensible: Easy to add new features (action event + menu item)

**Architectural Impact:** Improves developer experience, reduces key conflicts, enables future text commands.

**The implementation achieves RFC-007's core goal: consolidating scattered debug features into a contextual, hierarchical menu system.**

---

## Sign-Off

**Reviewed By:** ARCHITECT Role
**Date:** 2025-10-30
**Decision:** ✅ **ACCEPTED**
**Status:** Merged to main
