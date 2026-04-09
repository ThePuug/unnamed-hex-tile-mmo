# Dead Code Cleanup

Adopt the DEVELOPER role. Remove the following dead code. One commit per item group (systems, event variants, modules). Build after each commit to verify nothing breaks.

## 1. Dead Server Systems

Remove these three systems — they are never triggered or do nothing:

**`combat::do_nothing`** — Empty function used as a scheduling workaround.
- Delete function at `crates/server/src/systems/combat.rs:211`
- Remove registration at `crates/server/src/main.rs:90`
- If removing it causes a Bevy system ordering issue, investigate the real cause rather than keeping the workaround

**`input::try_gcd`** — Vestigial. Drains the `Try` message reader but processes nothing. Its own doc comment says "This system could be removed entirely."
- Delete function at `crates/server/src/systems/input.rs:58-70`
- Remove registration at `crates/server/src/main.rs:111`
- The `Event::Gcd` variant is still actively used (flows through renet write_try → send_do → client). Only the server-side try_gcd handler is dead.

**`actor::try_discover`** — Legacy tile discovery system. Nothing in the codebase sends `Event::Discover` (only `Event::DiscoverChunk` is used). The handler at `crates/server/src/systems/actor.rs:286-309` reads messages that never arrive.
- Delete function at `crates/server/src/systems/actor.rs:286-309`
- Remove registration at `crates/server/src/main.rs:127`

## 2. Dead Event Variant

**`Event::Discover`** — No code path constructs this variant. The only reference was the `try_discover` handler deleted above.
- Remove variant at `crates/common-bevy/src/message.rs:15`
- This changes serde variant indices for all subsequent variants. Since client and server are always built together from the same workspace, this is safe. Rebuild both.

**`Event::SpawnEngagement`** — Only defined at `crates/common-bevy/src/message.rs:34`. Nothing in the codebase constructs, matches, or handles this variant. It was declared for ADR-014 but engagement spawning uses direct entity spawning instead.
- Remove variant at `crates/common-bevy/src/message.rs:34`

## 3. Dead Modules

**`crates/client/src/systems/effect.rs`** — Commented out of `mod.rs` (`// pub mod effect;`). References `bevy_hanabi` which is not a client dependency. References `EffectMap` which doesn't exist. Particle effect prototype that was abandoned.
- Delete the file entirely
- Remove the commented-out line `// pub mod effect;` from `crates/client/src/systems/mod.rs:16`
- Also remove the comment `// pub mod combat_vignette;` at line 15 (file already deleted, only comment remains)

**`crates/server/src/systems/diagnostics.rs`** — Contains only a comment: "This module is kept as a placeholder for future server diagnostics." Empty placeholders are not code.
- Delete the file
- Remove `pub mod diagnostics;` from `crates/server/src/systems/mod.rs:5`

**`crates/common-bevy/src/systems/combat/heal.rs`** — `apply_healing_system` is never registered as a Bevy system anywhere. `PendingHeal` component is never constructed. Dead code written for SOW-021 Phase 3 that was never wired up.
- Delete `crates/common-bevy/src/systems/combat/heal.rs`
- Delete `crates/common-bevy/src/components/heal.rs`
- Remove `pub mod heal;` from `crates/common-bevy/src/systems/combat/mod.rs:6`
- Remove `pub mod heal;` from `crates/common-bevy/src/components/mod.rs:7`
- Check for and remove any `use` of `heal::*` or `PendingHeal` that the compiler flags

## 4. Allocating Diagnostics Function

**`get_message_type_name`** at `crates/client/src/systems/renet.rs:19-53` — Returns `String` (heap allocation) for every received network message. Called 3 times per frame minimum (once per channel). Should return `&'static str` instead.
- Change return type from `String` to `&'static str`
- Replace all `"Foo".to_string()` with `"Foo"`
- The `Incremental` match arm returns dynamic strings like `"Inc:Loc"` — these are all string literals, so they can be `&'static str` directly

## Verification

After all removals, run:
```
cargo build
cargo build -p client --features admin
cargo test
```

All three must pass. The `cargo build` alone will catch any missed references since Rust's dead code analysis is exhaustive for used items.
