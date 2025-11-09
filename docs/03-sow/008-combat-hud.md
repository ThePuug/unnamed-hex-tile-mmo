# SOW-008: Combat HUD

## Status

**Proposed** - 2025-10-31

## References

- **RFC-008:** [Combat HUD](../01-rfc/008-combat-hud.md)
- **ADR-014:** [Combat HUD Layered Architecture](../02-adr/014-combat-hud-layered-architecture.md)
- **Spec:** [Combat HUD Specification](../spec/combat-hud.md) (Phase 1 Critical)
- **Branch:** (proposed)
- **Implementation Time:** 17-23 days

---

## Implementation Plan

### Phase 1: Resource Bars (Foundation)

**Goal:** Display Health/Stamina/Mana bars at bottom-center

**Deliverables:**
- `client/systems/ui/resource_bars.rs` - Setup and update systems
- `client/components/resource_bar.rs` - Component definitions
- UI entities: 3 bars (background + foreground + text label each)

**Architectural Constraints:**
- Bottom-center screen position (fixed)
- Horizontal row layout: Stamina (left), Health (center), Mana (right)
- Use `step` for local player (predicted values)
- Color-coded: Red (Health), Yellow (Stamina), Blue (Mana)
- Always visible when player exists
- Text labels show "current / max"

**Success Criteria:**
- Resource bars visible at bottom-center
- Bars update when Health/Stamina/Mana change
- Text shows current/max values
- Colors distinct and readable

**Duration:** 2 days

---

### Phase 2: Action Bar Display

**Goal:** Display Q/W/E/R abilities with costs, cooldowns, ready states

**Deliverables:**
- `client/systems/ui/action_bar.rs` - Setup and update systems
- `client/components/action_bar.rs` - Component definitions
- UI entities: 4 rectangular ability slots

**Architectural Constraints:**
- Bottom-center, directly below resource bars
- Rectangular shape (distinct from circular threats per ADR-014)
- Each slot shows: keybind label (Q/W/E/R), ability icon, resource cost, cooldown overlay, state-indicating border
- State indicators: Ready (green), Cooldown (gray), Can't afford (red), Out of range (orange), No target (yellow)
- Query Gcd component for cooldown state
- Query Stamina/Mana for resource sufficiency
- MVP: Hardcoded abilities (Q=BasicAttack, E=Dodge, W/R=Empty)

**Success Criteria:**
- 4 ability slots visible below resource bars
- Keybinds labeled (Q, W, E, R)
- Border changes color based on state
- GCD overlay appears when Gcd active

**Duration:** 2-3 days

---

### Phase 3: Reaction Queue Display

**Goal:** Visualize incoming threats at top-center with timer rings

**Deliverables:**
- `client/systems/ui/reaction_queue.rs` - Setup and update systems
- `client/components/reaction_queue.rs` - Component definitions
- UI entities: 3 circular threat slots

**Architectural Constraints:**
- Top-center screen position (fixed)
- Circular shape (distinct from rectangular abilities per ADR-014)
- Each slot shows: attack type icon (center), timer ring (circular progress), countdown text (optional)
- Timer ring depletes clockwise (start at top, sweep right)
- Client maintains queue state (synced via `Do::InsertThreat`)
- Client runs timer countdown (local interpolation)
- Urgency indicator: Pulsing border when timer < 20% remaining
- Empty slots shown (faint gray, shows queue capacity)

**Success Criteria:**
- 3 circular slots visible at top-center
- Threats appear when inserted (icon + timer ring)
- Timer rings deplete smoothly
- Empty slots visible (faint gray)
- Pulsing when timer < 20%

**Duration:** 3-4 days

---

### Phase 4: Target Indicators (World-Space)

**Goal:** Red circle on hostile target, green circle on ally target

**Deliverables:**
- `client/systems/ui/target_indicators.rs` - Update system
- `client/components/target_indicator.rs` - Component definition
- Sprites: Circular indicator (red/green)

**Architectural Constraints:**
- World-space (parented to target entity)
- Position: Below entity (ground-level)
- Z-order: Behind entity sprite (render first)
- Colors: Red (hostile), Green (ally)
- Semi-transparent (doesn't obscure terrain)
- Query local player's Target component (from ADR-004)
- Spawn indicator on Target.hostile entity
- Spawn indicator on Target.ally entity (if exists)
- Despawn indicators when targets change

**Success Criteria:**
- Red circle appears on hostile target
- Green circle appears on ally target (if targeted)
- Indicators follow entity movement
- Indicators update when player changes target

**Duration:** 2 days

---

### Phase 5: Target Detail Frame

**Goal:** Show detailed enemy state (HP, resources, queue) in top-right frame

**Deliverables:**
- `client/systems/ui/target_frame.rs` - Update system
- `client/components/target_frame.rs` - Component definition
- UI entities: Container + header + HP bar + resource bars + queue display

**Architectural Constraints:**
- Top-right screen position (fixed)
- Auto-show when target exists, auto-hide when no target
- Content: Entity name + distance (hexes), HP bar with exact numbers, resource bars (if entity has them), threat queue (if entity has queue)
- Scaling by entity type: Basic enemies (HP only), Elite enemies/players (HP + resources + queue)
- Calculate distance: Hex count between player and target
- Use server-confirmed `state` (not predicted)

**Success Criteria:**
- Frame appears at top-right when target selected
- Frame disappears when no target
- Shows exact HP numbers (80/100)
- Shows distance in hexes (3h)
- MVP: Basic enemies show name + HP only

**Duration:** 2-3 days

---

### Phase 6: World-Space Health Bars

**Goal:** Health bars above all entities in combat

**Deliverables:**
- `client/systems/ui/health_bars.rs` - Update system
- `client/components/health_bar.rs` - Component definition
- Sprites: 2 rectangles per bar (background + foreground)

**Architectural Constraints:**
- World-space (parented to entity)
- Position: Above entity (1.5 world units up)
- Z-order: Above entity sprite (render last)
- Visibility rules: Always show for players, show for NPCs if in combat OR damaged, hide for NPCs if out of combat AND full HP
- Use `step` for local player, `state` for remote entities
- Scale foreground sprite X-axis for fill effect

**Success Criteria:**
- Health bars appear above entities in combat
- Bars update smoothly when damage taken
- Bars hide when out of combat (NPCs only)
- Player health bar always visible

**Duration:** 2-3 days

---

### Phase 7: Combat State Feedback + Facing Indicator

**Goal:** Visual indication of in-combat state + heading direction

**Deliverables:**
- `client/systems/ui/combat_state.rs` - Vignette and glow system
- `client/systems/ui/facing_indicator.rs` - Arrow and offset system
- `client/components/facing_indicator.rs` - Component definitions
- Sprites: Screen vignette overlay, arrow sprite

**Architectural Constraints:**

**Combat State Feedback:**
- Screen vignette: Full-screen overlay, dark red edges, semi-transparent, opacity based on combat state
- Resource bar glow: Orange outline when in combat
- Interpolate opacity (smooth fade in/out)

**Facing Indicator:**
- Arrow sprite above character (world-space)
- Update arrow rotation to match Heading component
- Character position offset toward heading (0.15 hex radius)
- Optional: Facing cone overlay (60° cone, toggleable with TAB)

**Success Criteria:**
- Screen edges darken when in combat
- Resource bars glow orange when in combat
- Effects fade smoothly when entering/exiting combat
- Arrow appears above character pointing in facing direction
- Character positioned on hex to indicate facing

**Duration:** 2-3 days

---

### Phase 8: UI Polish and Optimization

**Goal:** Smooth animations, performance optimization

**Deliverables:**
- Timer ring animation polish (smooth circular progress)
- Health bar interpolation (smooth fill changes)
- Object pooling for health bars
- Text update optimization (dirty checking)
- Camera culling for off-screen health bars

**Architectural Constraints:**
- Timer rings: 60fps, no jitter
- Pulsing borders: Sin wave interpolation when timer < 20%
- Health bar interpolation: Lerp over 0.2s (like Offset)
- Object pooling: Pool size 50 bars (covers 25 NPCs), reuse despawned entities
- Text optimization: Only update when value changes (dirty checking)
- Camera culling: Don't render health bars for off-screen entities

**Success Criteria:**
- Timer rings deplete smoothly (no stuttering)
- Health bars interpolate (no snapping)
- 100 NPCs in combat < 10% CPU (UI systems)
- Text updates minimal (dirty checking working)

**Duration:** 2-3 days

---

## Acceptance Criteria

**Functionality:**
- ✅ All 8 UI components implemented and functional
- ✅ Resource bars update when Health/Stamina/Mana change
- ✅ Action bar shows abilities with correct states (ready/cooldown/can't afford)
- ✅ Reaction queue shows threats with timers
- ✅ Target indicators appear on correct entities
- ✅ Target frame shows enemy state (HP/resources/queue when applicable)
- ✅ Health bars visible in combat, hidden out of combat (NPCs)
- ✅ Facing indicator tracks heading (arrow + character offset)
- ✅ Combat state feedback smooth (vignette/glow)

**UX:**
- ✅ Find any combat state info within 1 second (discoverability)
- ✅ Circular threats ≠ rectangular abilities (visual language works)
- ✅ Tactical decisions informed (enemy resources/queue visible)
- ✅ No confusion about which system is which

**Performance:**
- ✅ 20 NPCs in combat: 60fps maintained
- ✅ UI systems overhead < 10% CPU
- ✅ Text updates < 1ms per frame

**Code Quality:**
- ✅ Isolated in `client/systems/ui/` (contained)
- ✅ Queries existing components (no new backend)
- ✅ Client-side only (no network sync)

---

## Discussion

### Implementation Note: Timer Ring Rendering

**Options:**
- Sprite sheet (pre-rendered frames): Simplest, good for MVP
- bevy_prototype_lyon (vector shapes): More flexible, requires dependency
- Custom mesh (arc primitive): Complex, highest control

**Decision:** Start with sprite sheet (simplest), migrate to lyon if animation smoothness requires it.

### Implementation Note: UI Framework

**Current:** Bevy UI (built-in, sufficient for bars/circles/text)

**Future:** Consider bevy_egui migration when:
- Need rich text (ability tooltips with formatting)
- Need text input (future chat, command input)
- Need complex layouts (settings menu, character sheet)

For MVP, Bevy UI is sufficient.

### Implementation Note: Object Pooling Strategy

**Health bars spawn/despawn frequently** (entities enter/exit combat).

**Pooling approach:**
- Pre-spawn pool entities on startup? (eager)
- Lazy spawn on demand, cache despawned? (lazy)

**Decision:** Lazy spawn for MVP (simpler), optimize with pooling if profiling shows issue.

---

## Acceptance Review

(To be filled upon completion)

---

## Conclusion

The Combat HUD implementation provides complete visual feedback for all combat systems, enabling tactical decision-making and instant combat state understanding.

**Key Achievements:**
- Layered architecture (world-space, screen-space, floating)
- Consistent visual language (circles=threats, rectangles=abilities)
- Client prediction for local player (instant feedback)
- Target frame enables tactical decisions (see enemy state)

**Architectural Impact:** Establishes UI patterns for future features (status effects, minimap, party frames). Visual language extensible and genre-familiar.

**The implementation achieves RFC-008's core goal: instant combat understanding at a glance.**
