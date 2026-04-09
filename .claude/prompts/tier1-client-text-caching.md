# Tier 1: Client Text Caching — Stop Formatting Strings Every Frame

## Context

Staff Engineer review identified ~450 String allocations/sec from UI text that
reformats every frame even when the underlying values haven't changed. This is
Tier 1 priority item 2 (see `ROLES/STAFF-ENGINEER-MEMORY.md`).

## Problem

### resource_bars.rs (lines ~235-273)

`update()` runs every frame. Three `format!()` calls unconditionally:

```rust
**text = format!("{:.0} / {:.0}", health.step, health.max);   // line 237
**text = format!("{:.0} / {:.0}", stamina.step, stamina.max);  // line 255
**text = format!("{:.0} / {:.0}", mana.step, mana.max);        // line 273
```

These allocate a new String every frame (~180 allocs/sec at 60fps) even when
health/stamina/mana haven't changed. The bar WIDTH already interpolates
smoothly — the text just needs to show the integer values.

### ui.rs (lines ~129-180)

`update()` runs every frame. Two `format!()` calls unconditionally:

```rust
format!("{hour:02}:{minute:02} {day}.{week}.{season}")                        // line 158
format!("Haven: {} tiles | Zone: {} | Enemy Lv. {}", distance, zone_name, level)  // line 174
```

Time changes at most once per in-game minute. Distance/zone only changes when
the player moves to a new tile.

## What to change

### resource_bars.rs

Track the last-formatted integer values. Only call `format!()` when they change.
The values are f32 but displayed as `{:.0}` (no decimals), so track as
`(i32, i32)` per resource (current, max). Add fields to the existing marker
structs (`HealthBar`, `StaminaBar`, `ManaBar`) or to new small structs — your
call on which is cleaner.

Pattern:
```rust
let cur = health.step as i32;
let max = health.max as i32;
if health_bar.last_text != (cur, max) {
    health_bar.last_text = (cur, max);
    **text = format!("{cur} / {max}");
}
```

Repeat for stamina and mana.

### ui.rs

For `Info::Time`: the formatted string only changes when any of
(hour, minute, day, week, season) change. Compute those values, compare to a
cached tuple, only format on change. Store the cache on the `Info` enum variant
or in a Local<> system parameter.

For `Info::DistanceIndicator`: only changes when `player_loc` changes (which
triggers `Changed<Loc>`). Either:
- Gate the whole system on `Changed<Loc>` for the player entity, or
- Cache `(distance, zone, level)` and compare before formatting.

The simpler approach is a `Local<Option<(u32, DirectionalZone, u32)>>` that
skips formatting when unchanged.

## Constraints

- Don't change the visual behavior — same text, same format, same update
  timing from the player's perspective.
- Don't restructure the systems or change scheduling — just add dirty checks.
- `cargo build` and `cargo test` must pass.

## After completing

Update `ROLES/STAFF-ENGINEER-MEMORY.md`: mark Tier 1 item 2 as done with date
and brief description of what changed, same format as item 1.
