# ADR-008: Optimistic Reaction Ability Prediction

## Status

Accepted - 2025-10-29

## Context

**Related RFC:** [RFC-003: Reaction Queue System](../01-rfc/003-reaction-queue-system.md)

Players use Dodge/Counter/Parry to clear threats. **Should client wait for server confirmation or predict optimistically?**

### Requirements

- Instant feedback (< 16ms)
- Server validation (stamina, queue state, GCD)
- Rollback handling (server denial)
- Network efficiency

### Options Considered

**Option 1: Wait for Server** - Send input, wait 100-200ms for confirmation
- ❌ Combat feels sluggish
- ❌ Unacceptable UX for reaction abilities
- ✅ No rollback complexity

**Option 2: Predict Optimistically** - Show effects immediately, server confirms/denies
- ✅ Instant feedback (0ms)
- ✅ Responsive combat feel
- ⚠️ Rollback on denial (rare)

## Decision

**Predict optimistically (Option 2).**

### Rationale

**Combat responsiveness critical:**
- Commercial MMO standard: < 50ms ability feedback
- Modern games use client prediction ubiquitously
- Players expect instant response

**High prediction success rate:**
- Stamina synchronized (ADR-004, < 1% drift)
- Queue state synchronized (ADR-006/007)
- GCD known client-side
- **Success rate: 99%+**

**Rollback acceptable:**
- 99% instant feedback (prediction correct)
- 1% brief rollback + error message
- **Trade-off worth responsive feel**

**Matches existing patterns:**
- Movement: `Offset.step` predicted
- Resources: `Stamina.step` predicted
- Queue follows same pattern

---

## Implementation

### Prediction Pattern

**Client predicts:**
1. Validate locally (stamina >= cost, queue not empty)
2. If valid: clear queue, consume stamina, send request
3. If invalid: show error, don't predict

**Server validates:**
1. Check stamina >= cost
2. Check queue not empty
3. Check GCD not active
4. If valid: execute, broadcast confirmation
5. If invalid: broadcast failure

**Client receives:**
- Confirmation → already predicted (nothing to do)
- Failure → show error (accept brief desync, server corrections fix within 125ms)

### Local Validation

Prevent bad predictions:
```rust
// Before predicting
if stamina.step < cost {
    show_error("Not enough stamina");
    return;  // Don't predict or send
}

if queue.threats.is_empty() {
    show_error("No threats to dodge");
    return;
}

// Prediction valid - proceed
queue.threats.clear();
stamina.step -= cost;
send_to_server(UseAbility::Dodge);
```

**Benefits:**
- Reduces prediction failures (5% → < 1%)
- Immediate feedback for obvious failures
- Lowers network traffic (don't send doomed requests)

### Rollback Strategy

**MVP approach:**
- Show error message on failure
- Accept brief visual desync
- Server corrections fix within 1-2 ticks (125-250ms)
- Don't restore full queue state (deferred to post-MVP)

**Future enhancement:**
- Server sends queue snapshot on failure
- Client restores pre-ability state
- Smoother rollback experience

---

## Consequences

### Positive

- **Instant feedback:** < 16ms (1 frame)
- **High success rate:** 99%+ correct
- **Network efficiency:** No additional traffic
- **Matches expectations:** Feels like single-player

### Negative

- **Rollback complexity:** Threats reappear on failure
- **Testing burden:** Validate accuracy, rollback UX, edge cases
- **Potential desyncs:** Brief client-server divergence

### Mitigations

- Show predicted state with visual distinction (70% opacity)
- Audio/visual feedback on rollback ("Not enough stamina" VO)
- Accept rare desync (server corrections within FixedUpdate)
- Local validation reduces failures to < 1%

---

## References

- **RFC-003:** Reaction Queue System
- **ADR-006:** Server-Authoritative Reaction Queue
- **ADR-002:** Server-Authoritative Resource Management (same pattern)
- **Pattern:** Movement prediction (`Offset.state/step`)

## Date

2025-10-29
