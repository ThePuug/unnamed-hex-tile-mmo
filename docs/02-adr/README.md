# Architecture Decision Records (ADRs)

This directory contains technical architecture decisions for the unnamed hex-tile MMO. ADRs document **how we build systems**, not what features players experience (see `docs/00-spec/` for that).

## When to Read ADRs

- **Before implementing features** - Check if related ADRs exist, understand established patterns
- **When systems interact** - Understand how components/systems are designed to work together
- **During debugging** - ADRs explain the "why" behind architectural choices
- **Before proposing changes** - Understand rationale for current approach

## ADR Quick Reference

**Legend:** ✅ Accepted | 📋 Proposed | ⏭️ Superseded • 🌍 World/Terrain | ⚡ Resources | ⚔️ Mechanics | 🎨 UI/Tools

| ADR | Status | Title | Category | Date |
|-----|:------:|-------|:--------:|------|
| 001 | ✅ | Chunk-Based World Partitioning | 🌍 | 2025-10-28 |
| 002 | ✅ | Server-Authoritative Resource Management | ⚡ | 2025-10-29 |
| 003 | ✅ | Component-Based Resource Separation | ⚡ | 2025-10-29 |
| 004 | ✅ | Deterministic Resource Regeneration | ⚡ | 2025-10-29 |
| 005 | ✅ | Derived Combat Stats (On-Demand) | ⚡ | 2025-10-29 |
| 006 | ✅ | Server-Authoritative Reaction Queue | ⚔️ | 2025-10-29 |
| 007 | ✅ | Timer Synchronization via Insertion Time | ⚔️ | 2025-10-29 |
| 008 | ✅ | Optimistic Reaction Ability Prediction | ⚔️ | 2025-10-29 |
| 009 | 📋 | Heading-Based Directional Targeting | ⚔️ | — |
| 010 | ✅ | Damage Pipeline Two-Phase Calculation | ⚔️ | 2025-10-31 |
| 011 | ⏭️ | GCD Component Cooldown Tracking | ⚔️ | 2025-10-30 |
| 012 | ✅ | AI TargetLock Behavior Tree Integration | ⚔️ | 2025-10-30 |
| 013 | ✅ | Developer Console Architecture | 🎨 | 2025-10-30 |
| 014 | ✅ | Combat HUD Layered Architecture | 🎨 | 2025-10-31 |
| 015 | ✅ | Projectile System Architecture | ⚔️ | 2025-11-03 |
| 016 | ✅ | Movement Intent Architecture | ⚔️ | 2025-11-05 |
| 017 | ✅ | Universal Lockout + Synergy Architecture | ⚔️ | 2025-11-07 |
| 018 | 📋 | Ability Execution Pipeline | ⚔️ | 2025-11-07 |
| 019 | ✅ | Unified Interpolation Model | 🌐 | 2025-02-08 |
| 020 | ✅ | Super-Linear Level Multiplier | ⚡ | 2026-02-09 |
| 021 | ✅ | Commitment-Ratio Queue Capacity | ⚔️ | 2026-02-09 |
| 022 | ✅ | Dismiss Mechanic | ⚔️ | 2026-02-09 |
| 023 | 📋 | Coordinated Hex Assignment | 🤖 | 2026-02-09 |
| 024 | 📋 | Per-Archetype Positioning Strategy | 🤖 | 2026-02-09 |

### When to Read Each ADR

**World & Terrain:**
- **001 - Chunk-Based World Partitioning:** 8×8 hex chunks for discovery/caching/network. Read when working with terrain, FOV, or chunk systems.

**Combat Resources:**
- **002 - Server-Authoritative Resource Management:** Server owns state, client predicts via `.step`. Read when syncing any resource between client/server.
- **003 - Component-Based Resource Separation:** Separate Health/Stamina/Mana components. Read when adding new resources or querying combat state.
- **004 - Deterministic Resource Regeneration:** Both sides calculate regen from last event time. Read when implementing continuous resource changes.
- **005 - Derived Combat Stats:** Calculate armor/resistance on-demand from attributes. Read when adding derived stats or stat calculations.

**Combat Mechanics:**
- **006 - Server-Authoritative Reaction Queue:** Server owns queue, client predicts clears. Read when working with reaction abilities or queue display.
- **007 - Timer Synchronization via Insertion Time:** Client calculates timers from server's insertion time. Read when displaying countdowns or time-sensitive UI.
- **008 - Optimistic Reaction Ability Prediction:** Client predicts ability effects immediately. Read when implementing any ability with instant feedback requirements.
- **009 - Heading-Based Directional Targeting:** Auto-target nearest in facing cone. Read when implementing targeting, abilities, or directional mechanics.
- **010 - Damage Pipeline Two-Phase Calculation:** Outgoing at attack time, mitigation at resolution. Read when working with damage, attributes, or combat formulas.
- **011 - GCD Component Cooldown Tracking:** Component-based global cooldown. Read when implementing abilities or cooldown UI. **Note: Superseded by ADR-017 for variable recovery durations.**
- **012 - AI TargetLock Behavior Tree Integration:** Sticky targeting with behavior tree nodes. Read when working with AI combat or behavior trees.
- **015 - Projectile System Architecture:** Entity-based projectiles with travel time and collision detection. Read when implementing ranged attacks or dodgeable projectiles.
- **016 - Movement Intent Architecture:** Broadcast movement destinations before completion for remote entity prediction. Read when working with movement, network sync, or remote entity rendering.
- **017 - Universal Lockout + Synergy Architecture:** Variable recovery durations with early unlock synergies. Read when implementing ability pacing, combat flow, or synergy systems.
- **018 - Ability Execution Pipeline:** Three-stage pipeline (validation → execution → broadcasting) with pure function extraction. Read when implementing new abilities or refactoring ability systems.
- **020 - Super-Linear Level Multiplier:** Polynomial level scaling for HP, damage, and reaction stats plus reaction window gap modifier. Read when working with stat derivation, level scaling, or reaction timers.
- **021 - Commitment-Ratio Queue Capacity:** Focus investment ratio determines queue slots instead of raw points. Read when working with queue capacity, attribute investment, or Focus scaling.
- **022 - Dismiss Mechanic:** New queue management verb — skip front threat at full unmitigated damage with no lockout. Read when working with reaction queue, combat input, or queue UI.
- **023 - Coordinated Hex Assignment:** Engagement entity assigns unique approach hexes to child NPCs, recalculates on player tile change. Read when working with engagement spawning, NPC AI targeting, or multi-NPC coordination.
- **024 - Per-Archetype Positioning Strategy:** Surround/Cluster/Perimeter/Orbital strategies determine hex preference per archetype. Read when working with archetype behavior, engagement composition, or NPC spatial patterns.
- **019 - Unified Interpolation Model:** Separate authoritative position from visual interpolation, unified local/remote handling. Read when working with movement, position, or entity rendering.

**UI & Developer Tools:**
- **013 - Developer Console Architecture:** Hierarchical menu with event-based actions. Read when adding debug features or console commands.
- **014 - Combat HUD Layered Architecture:** 3-layer UI (world-space/screen-space/floating) with visual language. Read when implementing any combat UI or HUD elements.

## ADR Template

Use this as a starting point - tailor to fit your decision. Not all sections may apply.

```markdown
# ADR-XXX: [Decision Title]

## Status
[Proposed | Accepted | Deprecated | Superseded by ADR-XXX]

## Context
What problem are we solving? What are the constraints?
Reference related RFCs, specs, or existing ADRs.

## Decision
What specific approach are we taking? (1-2 paragraphs max)

### Core Mechanism
Code snippets, diagrams, or concrete examples showing how it works.

## Rationale
Why this approach over alternatives? (Key points only)

## Consequences
**Positive:** What this enables or improves
**Negative:** What becomes harder or costs more
**Mitigations:** How we address the negatives

## Implementation Notes
File locations, integration points, system ordering - practical details for implementers.

## References
- Related RFCs, specs, ADRs
- External resources if applicable

## Date
YYYY-MM-DD
```

## ADR Lifecycle

**Creation** → **Implementation** → **Acceptance** → **Active** → (optional) **Obsolescence**

- **Proposed**: Decision documented, seeking review/feedback
- **Accepted**: Approved and ready to implement (or already implemented)
- **Active**: Current architectural pattern in use
- **Deprecated**: No longer recommended, kept for historical context
- **Superseded**: Replaced by newer ADR (reference the replacement)

**Note:** Most ADRs stay "Accepted" and "Active" indefinitely. Only mark as deprecated/superseded when patterns fundamentally change.

## Guidelines

**Keep ADRs focused:**
- One decision per ADR
- Technical architecture only (game design goes in specs)
- Explain tradeoffs, not just final choice

**When to create an ADR:**
- Significant architectural pattern (affects multiple systems)
- Non-obvious tradeoffs (needs justification)
- Future reference value (others will ask "why?")

**When NOT to create an ADR:**
- Obvious/standard patterns (e.g., "use ECS for game entities")
- Implementation details (e.g., specific variable names)
- Temporary decisions (e.g., MVP scope cuts)
