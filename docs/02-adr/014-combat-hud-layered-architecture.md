# ADR-014: Combat HUD Layered Architecture

## Status

**Accepted** - 2025-10-31

## Context

**Related RFC:** [RFC-008: Combat HUD](../01-rfc/008-combat-hud.md)

Combat system requires 8+ distinct UI components: resource bars, action bar, reaction queue, target indicators, target frame, health bars, facing indicator, combat state feedback. Without architectural guidance, these components risk visual confusion and poor organization.

### Requirements

- 8 UI components must coexist without clutter
- Instant visual differentiation (threats vs abilities vs resources)
- Tactical information accessible at a glance
- Extensible for future features (status effects, minimap, party frames)
- Genre-standard conventions (reduce learning curve)

### Options Considered

**Option 1: Layered Architecture with Visual Language** ✅ **SELECTED**
- Three visual layers: world-space, screen-space, floating
- Consistent shape/color language (circles=threats, rectangles=abilities)
- Clear separation of concerns

**Option 2: Single-Layer UI (All Screen-Space)**
- All UI elements in screen-space overlay
- ❌ Harder to associate health bars with entities
- ❌ Manual positioning for projected elements

**Option 3: Ad-Hoc Component Design**
- Each UI component designed independently
- ❌ No visual consistency, risk of confusion
- ❌ Hard to extend (no patterns to follow)

**Option 4: Minimalist HUD (Fewer Components)**
- Only show resource bars + reaction queue
- ❌ Doesn't meet combat-hud.md Phase 1 Critical requirements
- ❌ Lacks action bar, target frame (critical for tactical play)

## Decision

**Use layered UI architecture with consistent visual language system.**

### Core Mechanism

**Three Visual Layers:**

**1. World-Space UI:**
- Health bars (above entities)
- Target indicators (ground circles below entities)
- Parented to entity Transform, follows entity movement
- Z-ordering: indicators below entity, bars above entity

**2. Screen-Space Overlay:**
- Resource bars (bottom-center, fixed position)
- Action bar (bottom-center, below resources)
- Reaction queue (top-center)
- Target detail frame (top-right)
- Combat state feedback (vignette, glows)

**3. Floating UI:**
- Damage numbers (world-space but ephemeral, from ADR-005)
- Status effect text (future)

**Visual Language Consistency:**

**Shapes Communicate Purpose:**
```
Circular:     Threats, timers, urgent depleting resources
Rectangular:  Abilities, stable resources, inventory (future)
Icons:        Attack types (⚔️, 🔥), ability types, status effects
```

**Colors Communicate State:**
```
Red:           Health, damage, danger, hostile targets
Yellow/Orange: Stamina, physical resources, warnings
Blue:          Mana, magic resources, information
Green:         Healing, buffs, ally targets
Gray:          Inactive, disabled, unavailable
```

**Client Prediction Strategy:**

**Resource Bars:**
- Local player: Use `step` (predicted)
- Remote entities: Use `state` (server-confirmed)

**Reaction Queue:**
- Client maintains queue state (synced via `Do::InsertThreat`)
- Client runs timer countdown (local interpolation)
- Server authoritative for insertion/removal

**Health Bars:**
- Local player: Use `step` (predicted damage from ADR-005)
- Remote entities: Use `state` only (no prediction)

---

## Rationale

### 1. Three-Layer Separation of Concerns

**World-space = Entity Context:**
- Health bars spatially associated with entities (instant recognition)
- Target indicators show "this is the target" on entity itself
- Scales to multiple entities (each has own health bar)

**Screen-space = Player State:**
- Resources, abilities, queue always visible (fixed position)
- No occlusion by entities or terrain
- Familiar "HUD" paradigm (bottom = player, top = threats)

**Floating = Event Feedback:**
- Damage numbers ephemeral (appear, fade, disappear)
- Don't persist (would clutter screen)
- World-space for spatial context (damage from this entity)

### 2. Visual Language Prevents Confusion

**Problem:** 8+ UI components risk looking similar (boxes everywhere).

**Solution:** Shapes encode purpose.

**Circular = Urgent/Depleting:**
- Reaction queue threats (0.8s remaining → action needed)
- Timer rings (circular progress, clockwise depletion)
- Mental model: "running out" (circle empties)

**Rectangular = Stable/Actions:**
- Action bar abilities (Q/W/E/R remain until player presses)
- Resource bars (change slowly, not urgent timers)
- Mental model: "this is mine to control"

**Genre Conventions:**
- MOBAs use circles for cooldowns (League, Dota)
- MMOs use rectangles for action bars (WoW, FFXIV)
- Players expect these patterns

### 3. Client Prediction for Instant Feedback

**Local player needs immediate feedback:**
- Pressing Dodge → stamina drains instantly (use `step`)
- Taking damage → health bar updates immediately (use `step`)
- Inserting threat → queue shows threat instantly (client maintains state)

**Remote entities need accuracy:**
- Enemy health updates on server tick (use `state`)
- No prediction needed (remote state less critical than local)
- Reduces complexity (no rollback for remote entities)

### 4. Target Frame Always-Visible

**Decision:** Show target detail frame whenever player has target selected. No lock required.

**Rationale:**
- "Current target = current interest" - if targeting, you want details
- Tactical decision-making requires information NOW (not after lock keypress)
- Example: "Can they dodge?" needs stamina visible immediately
- Simplifies UX (no hidden information, no lock friction)

**Scaling:** Basic enemies show minimal info (HP only), smart enemies show full state (resources + queue).

---

## Consequences

### Positive

**1. Instant Visual Differentiation**
- Circular threats ≠ rectangular abilities (never confused)
- Color-coded states (red=danger, green=safe)
- Genre-familiar (reduces learning curve)

**2. Tactical Depth Enabled**
- Target frame shows enemy resources/queue (inform decisions)
- Action bar shows ability costs/cooldowns (plan actions)
- Reaction queue shows threat timers (prioritize reactions)

**3. Clear Spatial Association**
- Health bars above entities (instant "which bar is which enemy?")
- Target indicators on entities (unambiguous targeting)
- Screen-space for abstract state (resources, abilities)

**4. Extensibility**
- Future status effects: Icons above health bars (same circular style)
- Future minimap: Top-left corner (doesn't conflict)
- Future party frames: Left side (same resource bar style)

**5. Minimal Network Impact**
- Client-side only (UI rendering, no server sync)
- Uses existing prediction patterns (Offset, step)
- Queries existing components (no new backend)

### Negative

**1. World-Space UI Scaling Complexity**
- Health bars scale with camera zoom (may be too small/large)
- Target indicators affected by zoom
- Requires testing at min/max zoom levels

**Mitigation:** Orthographic camera, clamp scale based on zoom.

**2. Text Rendering Performance**
- Resource labels: "120 / 150" × 3 = dynamic text
- Countdown timers: "0.8s" × 3 = updating every frame
- Text rendering expensive in Bevy

**Mitigation:** Bitmap fonts, dirty checking (only update when value changes), optional text hiding.

**3. UI Entity Overhead**
- Health bars on 20 NPCs = 40 sprites (background + foreground)
- Spawning/despawning entities when visibility toggles
- Hierarchical transforms (parented to entities)

**Mitigation:** Only show when in combat/damaged (reduces count by 60-80%), object pooling.

**4. Color Accessibility**
- Red/green target indicators (colorblind issue)
- Red health bars (deuteranopia)
- Orange reaction queue borders (protanopia)

**Mitigation:** Combine color + shape (circles for targets), combine color + icons (⚔️ for threats), future colorblind mode.

**5. Visual Complexity with Many Entities**
- 20 NPCs = 20 health bars = visual noise
- Hard to distinguish individual entities

**Mitigation:** Only show health bars in combat/damaged, only show 1 hostile + 1 ally indicator (clear targeting), future camera culling.

### Neutral

**1. UI Framework Choice**
- MVP uses Bevy UI (built-in, simple)
- May migrate to bevy_egui for complex UI later (tooltips, text input)
- Current choice sufficient for bars/circles/text

**2. Animation Complexity**
- Timer ring depletion (smooth circular progress)
- Pulsing borders (sin wave interpolation)
- Health bar interpolation (smooth fill changes)

May require custom rendering or sprite sheets. Start simple (no animations), add polish later.

---

## Implementation Notes

**File Structure:**
```
src/client/systems/ui/
  ├── resource_bars.rs      # Health/Stamina/Mana (screen-space)
  ├── action_bar.rs         # Q/W/E/R abilities (screen-space)
  ├── reaction_queue.rs     # Circular threats (screen-space)
  ├── target_indicators.rs  # Red/green circles (world-space)
  ├── target_frame.rs       # Detail frame (screen-space)
  ├── health_bars.rs        # Above entities (world-space)
  ├── facing_indicator.rs   # Arrow + offset (world-space)
  └── combat_state.rs       # Vignette + glow (screen-space)
```

**Integration Points:**
- Queries: Health, Stamina, Mana (ADR-002)
- Queries: ReactionQueue (ADR-003)
- Queries: Target (ADR-004)
- Queries: Heading (ADR-009)
- Queries: Gcd (ADR-011)

**Network:** All client-side (no sync needed)

---

## Validation Criteria

**Functional:**
- 8 UI components coexist without visual confusion
- Local player sees instant feedback (step prediction)
- Remote entities show server-confirmed state (state)
- Target frame appears when target exists, disappears when no target

**Visual:**
- Circular threats ≠ rectangular abilities (distinct)
- Color-coded states clear (red=danger, green=safe)
- Spatial association works (health bars → entities)

**Performance:**
- 20 NPCs in combat → 60fps maintained
- Text updates < 1ms per frame
- UI entity overhead negligible (< 10% CPU)

**UX:**
- Player knows combat state at a glance (< 1 second)
- Tactical decisions informed (enemy resources/queue visible)
- No confusion about which system is which (visual language works)

---

## References

- **RFC-008:** Combat HUD
- **Spec:** `docs/00-spec/combat-hud.md` (Phase 1 Critical features)
- **ADR-002:** Combat Foundation (Health/Stamina/Mana components)
- **ADR-003:** Reaction Queue System (ReactionQueue component)
- **ADR-004:** Ability System and Directional Targeting (Target component)
- **ADR-005:** Damage Pipeline (damage numbers, floating text)
- **ADR-009:** Heading-Based Directional Targeting (Heading component)
- **ADR-011:** GCD Component (Gcd component)

## Date

2025-10-31
