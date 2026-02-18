# ADR-030: Reaction Queue Window Mechanic

## Status
Accepted

## Context
With 3+ enemies attacking simultaneously, the reaction queue overflows because each enemy adds a separate threat and the player can only react to one at a time (Counter clears 1, Deflect clears all). The 1.5s recovery lockout between reactions means the queue fills faster than the player can process it. The problem isn't player weakness — it's throughput.

The old model used a hard capacity limit with overflow eviction: when the queue was full, the oldest threat was immediately resolved as damage. This punished players for being outnumbered regardless of skill.

## Decision
Replace the fixed-capacity queue with a **visibility window** over an unbounded stream:

1. **Queue is unbounded** — threats always insert, never evict on overflow
2. **Window size** (from Focus/Concentration tier) determines how many threats the player can see and interact with
3. **Counter** clears all visible threats (blanket reaction) — single 30 stamina payment
4. **Deflect** clears the entire queue (visible + hidden) — panic button at 50 stamina
5. **Knockback** stays single-target (displacement, not a pure reaction) — targets last visible threat
6. **Dismiss** stays instant and spammable — pops the front threat at full damage, no lockout

Threats behind the window still tick their timers and resolve as damage normally. The window just controls what the player can actively react to.

### Core Mechanism

```
Queue: [T1] [T2] [T3] | [T4] [T5] [T6]
        ---- window ---   --- hidden ---

Counter:  clears T1, T2, T3 (all visible)
Deflect:  clears T1-T6 (everything)
Knockback: targets source of T3 (last visible)
Dismiss:  pops T1 (front), applies full damage
```

## Rationale
- **Throughput scales with window size**: T3 Focus players see 4 threats and clear them all with one Counter, making Focus investment meaningful for multi-enemy combat
- **No artificial punishment**: Threats behind the window still resolve normally on their timers, but players aren't penalized for queue overflow with immediate forced damage
- **Blanket reactions feel powerful**: Clearing 3-4 threats simultaneously rewards timing and resource management
- **Deflect is the emergency button**: Full queue clear at higher cost gives players an escape valve

## Consequences
**Positive:** Multi-enemy combat becomes manageable through player skill and build investment. Focus builds have a clear combat identity.

**Negative:** Very large enemy groups can build up long hidden queues that steadily resolve as damage. This is intentional — the player should feel pressure to react quickly.

## Implementation Notes
- `ReactionQueue.capacity` renamed to `window_size`
- `is_full()` removed (queue never overflows)
- `visible_count()` and `hidden_count()` added as helpers
- `insert_threat()` simplified to always push_back
- `queue_capacity()` on ActorAttributes renamed to `window_size()` (same tier values)

## Date
2026-02-11
