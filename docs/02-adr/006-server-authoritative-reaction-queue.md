# ADR-006: Server-Authoritative Reaction Queue

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-003: Reaction Queue System](../01-rfc/003-reaction-queue-system.md)

Reaction queue holds incoming damage before application. **Who owns queue state - client or server?**

### Requirements

- Cheat prevention (especially PvP)
- Server-validated damage application
- Client responsiveness (instant feedback on reactions)
- Network efficiency

### Options Considered

**Option 1: Client Authority** - Client owns queue, tells server what happened
- ❌ Cheating trivial (fake cleared threats)
- ❌ No server validation
- ❌ Unsuitable for PvP

**Option 2: Server Authority, No Prediction** - Server owns queue, client waits for confirmation
- ✅ Cheat-proof
- ❌ 100-200ms latency (unresponsive combat)
- ❌ Poor UX (press Dodge, nothing happens)

**Option 3: Server Authority + Client Prediction** - Server owns, client predicts for local player
- ✅ Instant feedback (0ms)
- ✅ Cheat-proof (server validates)
- ✅ Rare rollbacks (99%+ prediction success)
- ⚠️ Prediction complexity

## Decision

**Server authority with client prediction (Option 3).**

### Rationale

**Server authority non-negotiable:**
- PvP requires server validation (prevent cheating)
- Server is source of truth for damage application
- Validates: stamina cost, queue state, GCD, ability allowed

**Client prediction required for UX:**
- Commercial MMO standard: < 50ms reaction feedback
- 100-200ms delay feels broken
- Players expect instant response (press Dodge → queue clears now)

**High prediction success rate:**
- Stamina synchronized (ADR-004, < 1% drift)
- Queue state synchronized on inserts
- GCD known client-side
- Estimated failures: < 1% of ability uses

**Rollback acceptable:**
- 99% instant feedback (correct prediction)
- 1% brief rollback with error message
- Trade-off worth responsive feel

**Matches existing patterns:**
- Movement: `Offset.step` predicted, `Loc.state` authoritative
- Resources: `Stamina.step` predicted, `Stamina.state` authoritative
- Queue follows same pattern

---

## Implementation

### Authority Model

**Server owns:**
- Queue state (`ReactionQueue` component)
- Threat insertion (on damage events)
- Threat expiry (checks in FixedUpdate)
- Threat resolution (applies damage)
- Queue clears (validates ability usage)

**Client displays:**
- Queue visualization (icons with timers)
- Predicted state for local player
- Server-confirmed state for remote players

### Prediction Flow

**Client predicts:**
1. Validate locally (stamina >= cost, queue not empty)
2. If valid: clear queue, reduce stamina, send request
3. If invalid: show error, don't predict or send

**Server validates:**
1. Check stamina >= cost
2. Check queue not empty
3. Check GCD not active
4. If valid: execute and broadcast confirmation
5. If invalid: broadcast failure

**Client receives:**
- Confirmation: Queue already cleared (prediction correct)
- Failure: Show error (accept brief desync, server corrections fix within 125ms)

### Local Validation

Prevent bad predictions by validating client-side first:
- Stamina check (prevent obvious failures)
- Queue empty check
- Reduces prediction failures from potential 5% to < 1%
- Lowers network traffic (don't send doomed requests)

---

## Consequences

### Positive

- **Instant feedback:** < 16ms (1 frame) for ability effects
- **Cheat prevention:** Server validates everything
- **Network efficiency:** No additional traffic (request + confirmation would exist anyway)
- **High success rate:** 99%+ prediction correct
- **Matches expectations:** Feels like single-player game

### Negative

- **Rollback complexity:** Threats reappear on failure (jarring)
- **Potential desyncs:** Brief client-server divergence (< 125ms)
- **Testing burden:** Must validate prediction accuracy, rollback UX, edge cases

### Mitigations

- Show predicted state with visual distinction (70% opacity)
- Audio/visual feedback on rollback ("Not enough stamina" VO)
- Accept rare desync (server corrections within FixedUpdate)
- MVP: Simple error message, no queue restoration (full rollback deferred)

---

## References

- **RFC-003:** Reaction Queue System
- **ADR-007:** Timer Synchronization via Insertion Time
- **ADR-008:** Optimistic Reaction Ability Prediction (detailed patterns)
- **ADR-002:** Server-Authoritative Resource Management (same pattern)
- **Pattern:** Movement prediction (`Offset.state/step`)

## Date

2025-10-29
