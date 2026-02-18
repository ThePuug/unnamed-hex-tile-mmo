# The Triumvirate

The Actor Triumvirate is a three-dimensional classification system for describing any entity (character, creature, faction, or force) through three fundamental aspects: their **Origin**, their **Approach**, and their **Resilience**.

## Origin

Origin describes where an entity comes from—their fundamental nature and source of existence. Most origins exist in opposing pairs, with one standing alone.

* **Evolved**: Product of natural selection and biological processes ↔ *Synthetic*
* **Synthetic**: Crafted by artificial means or intelligent design ↔ *Evolved*
* **Essential**: Pure manifestation of fundamental forces or concepts ↔ *Corrupted*
* **Corrupted**: Twisted, blighted, perverted from original form ↔ *Essential*
* **Mythic**: Born from legend, collective belief, and remembered stories ↔ *Forgotten*
* **Forgotten**: Ancient beings erased from memory, lost to time ↔ *Mythic*
* **Indiscernible**: Origin cannot be traced or categorized *(unique—unknowable source, includes phantasms and incomprehensible horrors)*

## Approach

Approach describes how an entity engages with others—both in combat and social interaction. Each approach has a natural opposite, and each has **two signature skills** that define its tactical identity.

* **Direct**: Closes distance, confrontational, bold ↔ *Distant*
  * Signatures: **Charge** / **Lunge**
* **Distant**: Maintains range, reserved, indirect ↔ *Direct*
  * Signatures: **Volley** / **Mark**
* **Ambushing**: Surprise attack, shocking reveals, sudden moves ↔ *Patient*
  * Signatures: **Ambush** / **Trap**
* **Patient**: Methodical, calculated, observant ↔ *Ambushing*
  * Signatures: **Taunt** / **Counter**
* **Binding**: Immobilizes, controls, extracts commitment ↔ *Evasive*
  * Signatures: **Root** / **Snare**
* **Evasive**: Mobile, disrupts through positioning ↔ *Binding*
  * Signatures: **Flank** / **Disorient**
* **Overwhelming**: Surrounds, dominates through presence *(unique—no clean opposite)*
  * Signatures: **Overpower** / **Petrify**

## Resilience

Resilience describes how an entity withstands pressure—both physical harm and social/psychological stress. Each resilience type has a natural opposite, and each has **two signature skills** that define its defensive identity.

* **Vital**: Physical endurance, emotional stamina ↔ *Mental*
  * Signatures: **Regenerate** / **Endure**
* **Mental**: Consciousness under duress, intellectual fortitude ↔ *Vital*
  * Signatures: **Focus** / **Dispel**
* **Hardened**: Physical armor, callused to appeals ↔ *Shielded*
  * Signatures: **Fortify** / **Deflect**
* **Shielded**: Magical wards, protected by reputation ↔ *Hardened*
  * Signatures: **Ward** / **Repel**
* **Blessed**: Divine favor, sustained by conviction ↔ *Primal*
  * Signatures: **Heal** / **Cleanse**
* **Primal**: Elemental resistance, raw authenticity ↔ *Blessed*
  * Signatures: **Enrage** / **Attune**
* **Eternal**: Exists across time, cannot be permanently ended *(unique—always returns)*
  * Signatures: **Revive** / **Defy**

## Skill Variations

Each actor has access to **4 signature skills** (2 from Approach, 2 from Resilience). While the mechanical function of these skills remains consistent, **Origin provides thematic and visual flavor** for how they manifest:

### Example: Berserker (Direct/Vital)

* Base kit: Charge, Lunge, Regenerate, Endure
* **Evolved Berserker** = Troll (biological regeneration, bestial charge)
* **Synthetic Berserker** = Combat Mech (nanite repair, rocket-assisted charge)
* **Essential Berserker** = Wrath Elemental (fury sustains it, explosive charge)
* **Corrupted Berserker** = Blighted Warrior (twisted regeneration, unnatural lurch)
* **Mythic Berserker** = Nephilim of War (battle-glory heals, heroic charge)
* **Forgotten Berserker** = Lost Champion (ancient vitality, weathered technique)
* **Indiscernible Berserker** = The Butcher (unknowable regeneration, reality-defying approach)

---

## Implementation Deviations

Where the current implementation intentionally differs from spec:

| # | Area | Spec Says | Implementation | Rationale |
|---|------|-----------|----------------|-----------|
| 1 | Ability set | Players choose Approach + Resilience, get 2 signature skills from each | MVP has 4 fixed abilities (Lunge, Overpower, Knockback, Deflect) not following Triumvirate | ADR-009; validate combat mechanics before build diversity |
| 2 | Deflect scope | Clears first queued threat (Hardened signature) | Clears all queued threats (50 stamina) | Simplified MVP defensive option |
| 3 | Origin system | Origin provides thematic flavor for skill visuals | Players are "Evolved" by default, no mechanical impact | Visual flavor deferred until core system exists |

## Implementation Gaps

**Critical for build diversity:** Full Approach set (7 approaches, 14 signatures — only 2/14 exist), full Resilience set (7 resiliences, 14 signatures — ~0.5/14 exist), Triumvirate selection at character creation

**Medium:** Approach/Resilience opposites (Direct ↔ Distant, etc.), attribute integration with leanings

**Post-MVP:** Additional Origins (Synthetic, Essential, Corrupted, Mythic, Forgotten, Indiscernible), Origin skill flavoring (visual/audio variations)

---

**Related Design Documents:**
- [Combat System](combat.md) — How approach/resilience skills work mechanically
- [Attribute System](attributes.md) — Attributes are independent from Triumvirate (decoupled)
