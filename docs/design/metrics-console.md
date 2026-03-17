# Metrics Console

## Philosophy

Both server and client consoles adopt a **satellite operations telemetry monitor** aesthetic — monospace character grids rendered as ASCII text. This is a deliberate choice over widget-based UI frameworks (egui panels, HTML dashboards):

- **Pixel-perfect layout control.** Character positions are deterministic. No layout negotiation, no reflow, no widget sizing surprises.
- **Composability.** Sparklines, labels, values, and separators are all characters. They compose in a single text buffer with no layering concerns.
- **Density.** Monospace grids pack more information per pixel than widget UIs. Operational consoles prioritize scan density over visual comfort.
- **Consistency.** Both consoles share the same grid primitive, the same formatting functions, the same visual language. A metric row on the server console reads identically to one on the client overlay.

The consoles are development tools, not player-facing UI. They exist to answer "is the system healthy" at a glance while playtesting.

---

## Grid System

### Segment (15ch)

The **segment** is the atomic grid unit: 15 characters wide.

15 is the unique integer width that supports four levels of binary subdivision with 1ch gutters and integer widths at every level:

```
Segment:  15ch
Half:      7ch  (15 - 1 gutter) / 2
Quarter:   3ch  ( 7 - 1 gutter) / 2
Eighth:    1ch  ( 3 - 1 gutter) / 2
```

No other base width has this property. 14ch breaks at the first split (6.5ch). 16ch breaks at the third (3ch → 1.5ch).

### Subdivision

Each level splits the parent into two equal children with a 1ch gutter between them:

```
Segment   ███████████████        15ch
Half      ███████ ███████         7ch + 1 + 7ch
Quarter   ███ ███ ███ ███         3ch + 1 + 3ch + 1 + 3ch + 1 + 3ch
Eighth    █ █ █ █ █ █ █ █         1ch (× 8, with 7 gutters)
```

### Tiling

A row can mix any combination of subdivision levels as long as widths sum correctly within a section. Gutters between adjacent cells are always 1ch regardless of subdivision level.

```
111111111111111 111111111111111 111111111111111
2222222 2222222 111111111111111 2222222 2222222
444 444 2222222 111111111111111 2222222 444 444
444 2222222 8 8 111111111111111 8 8 2222222 444
```

This allows metric rows to adapt their layout to their content — a label+value pair in two halves, a sparkline in a full segment, stats spread across quarters — all within the same grid.

### Content Widths

The subdivision levels map naturally to metric content:

| Width | Fits | Examples |
|-------|------|---------|
| 15ch (segment) | Sparkline (15 block chars), or padded label+value+unit | `▁▂▃▅▆▇█▆▅▃▂▁▂▃▅` |
| 7ch (half) | 5ch value + 2ch unit, or 5ch label + 2ch pad | `1.2e4 kb`, `FRAME  ` |
| 3ch (quarter) | Short label, small counter, flag | `pk `, `0! `, `RTT` |
| 1ch (eighth) | Single-char indicator, separator, flag | `!`, `●`, `│` |

---

## Super-Segments (Sections)

Sections are composed of N segments with 1ch gutters between them:

```
Width = N × 15 + (N - 1) × 1
```

| Segments | Section Width | Name | Use |
|----------|--------------|------|-----|
| 2 | 31ch | Compact | Narrow sidebar, minimal stats |
| 3 | 47ch | Standard | Client metrics overlay |
| 4 | 63ch | Wide | Extended diagnostics |
| 5 | 79ch | Full | Server console section (classic terminal width) |

The server console uses **79ch** sections (5 segments each). The client metrics overlay uses **47ch** (3 segments). Both derive from the same 15ch primitive — the only difference is how many segments tile into a section.

Multiple sections can tile horizontally with a section gap (typically 2-3ch) to form the full console width. The server console currently uses three 79ch sections side by side.

---

## Number Formatting

Three format widths derived from the grid subdivision levels. Suffix tiers: **K** (×10³), **M** (×10⁶), **B** (×10⁹), **T** (×10¹²).

### Short (3ch) — integer only

Fits a quarter-segment. Represents integer quantities from -99 to 99T. The decimal notation at suffix boundaries (`.1K`, `.9M`) is magnitude notation for integer values (100, 900,000), not actual fractional quantities.

```
 3ch short — -99 to 99T
 ───────────────────────────────────────
 "-99" .. " -1"     negative integer
 "  0"              zero
 "  1" .. " 99"     raw integer
 ".1K" .. ".9K"     100–900 (integer, magnitude notation)
 " 1K" .. "99K"     1,000–99,000
 ".1M" .. ".9M"     100K–900K (integer, magnitude notation)
 " 1M" .. "99M"     1M–99M
 ...continues through B, T
```

Transitions: `-1 → 0 → 1` (natural), `99 → .1K`, `99K → .1M`. The magnitude notation at each suffix boundary bridges integer values that don't fit in 2 digits at the current tier.

### Unit (5ch + 2ch) — integer or decimal

The 5ch number occupies a half-segment. The 2ch unit (`ms`, `hz`, `kb`, etc.) sits in the adjacent space. Has two display modes: **integer** and **decimal**.

The two modes share the same suffix bands — they only diverge in the raw (unsuffixed) range where decimal mode preserves fractional digits.

**Integer mode:**

```
 5ch unit integer — full range with precision
 ─────────────────────────────────────────────────────────
 -9E99 .. -1E14                           10^14-99
  -99T ..  -10T  (- 99_000_000_000_000)   10^12
 -9.9T .. -1.0T  (-  9_900_000_000_000)   10^11
 -.99T .. -.10T  (-    990_000_000_000)   10^10
  -99B ..  -10B  (-     99_000_000_000)   10^9
 -9.9B .. -1.0B  (-      9_900_000_000)   10^8
 -.99B .. -.10B  (-        990_000_000)   10^7
  -99M ..  -10M  (-         99_000_000)   10^6
 -9.9M .. -1.0M  (-          9_900_000)   10^5
 -.99M .. -.10M  (-            990_000)   10^4
  -99K ..  -10K  (-             99_000)   10^3
 -9999 ..  -100  (-              9_999)   10^0
   -99 ..    -1  (-                 99)   10^0
     0
     1 ..  9999  (               9_999)   10^0
 10.0K .. 99.9K  (              99_900)   10^2
  100K ..  999K  (             999_000)   10^3
 1.00M .. 9.99M  (           9_990_000)   10^4
 10.0M .. 99.9M  (          99_900_000)   10^5
  100M ..  999M  (         999_000_000)   10^6
 1.00B .. 9.99B  (       9_990_000_000)   10^7
 10.0B .. 99.9B  (      99_900_000_000)   10^8
  100B ..  999B  (     999_000_000_000)   10^9
 1.00T .. 9.99T  (   9_990_000_000_000)   10^10
 10.0T .. 99.9T  (  99_900_000_000_000)   10^11
  100T ..  999T  ( 999_000_000_000_000)   10^12
  1E15 ..  9E99                           10^15-99
```

Each suffix tier has three sub-bands: leading-dot (`.99X`), single-digit (`9.9X`), and double-digit (`99X`). The positive side enters suffixes at 10.0K (raw integers cover up to 9999). The negative side enters suffixes at -10K (raw integers cover down to -9999). Negative uses trimmed-leading-zero notation at the low end of each tier (`.99K`, `.10M`) — same pattern as short format.

**Decimal mode** — diverges from integer only in the raw range:

```
 5ch unit decimal — divergent range
 ───────────────────────────────────────
 -9999 ..  -100     integer (converged)
 -99.9 .. -10.0     1 decimal
 -9.99 .. -1.00     2 decimal
     0
  1.00 .. 99.99     2 decimal
 100.0 .. 999.9     1 decimal
 10.0K .. 99.9K     converged with integer
```

All suffix bands are identical to integer mode. Decimal mode adds fractional digits in the range -99.9 to 999.9, useful for precise percentages and sub-unit quantities. The positive side gets one extra decimal band (100.0–999.9) that integer mode shows as raw integers.

### Long (7ch) — decimal only, fixed point at column 4

Fits a full half-segment. The decimal point is **always at column 4** — every value in the entire range, positive or negative, raw or suffixed, has its decimal vertically aligned. This is the defining property of the long format.

Accepts integer inputs but formats them with trailing zeros to maintain the decimal position (e.g., `5` → `"  5.000"`). Provides fractional visibility in the range -99.999 to 999.999.

**Positive** — 3 decimal digits raw, 2 suffixed:

```
 7ch long positive — 0.001 to 999.99T
 ─────────────────────────────────────── 
 "  0.001" .. "  9.999"     3 decimal, no suffix
 " 10.000" .. "999.999"     3 decimal, no suffix
 "  1.00K" .. "  9.99K"     2 decimal + suffix (1,000–9,990)
 " 10.00K" .. "999.99K"     2 decimal + suffix
 "  1.00M" .. "999.99M"     2 decimal + suffix
 ...continues through B, T
```

**Negative** — sign consumes position 1 or 2, decimal stays at 4:

```
 7ch long negative — -99.99T to -0.001
 ───────────────────────────────────────
 " -0.001" .. " -0.999"     3 decimal, leading zero
 " -1.000" .. " -9.999"     3 decimal
 "-10.000" .. "-99.999"     3 decimal
 " -0.10K" .. " -0.99K"     2 decimal + suffix (-100 to -990)
 " -1.00K" .. "-99.99K"     2 decimal + suffix
 " -0.10M" .. "-99.99M"     2 decimal + suffix
 ...continues through B, T
```

Transitions: positive `999.999 → 1.00K`, negative `-99.999 → -0.10K`. The point never moves.

**Precision note:** At negative suffix boundaries, the fixed decimal position means some values are represented with less precision than the 7ch budget could technically afford. We accept this trade-off to maintain decimal alignment.

### Summary

| Format | Width | Grid level | Mode | Range | Negatives |
|--------|-------|-----------|------|-------|-----------|
| Short | 3ch | Quarter | Integer only | -99 to 99T | Down to -99 |
| Unit | 5ch+2ch | Half | Integer or decimal | -999T to 999T | Full range |
| Long | 7ch | Half | Decimal only | -99.99T to 999.99T | Full range |

Labels: short (3ch, quarter) or long (7ch, half). A standard metric row pairs a label half with a value half in one segment.

### Implementations

A single canonical implementation lives in `crates/common/src/numfmt.rs`. `NumFmt` struct with three orthogonal axes (`width`, `Precision`, `Overflow`) supports arbitrary width/mode combinations. Both consoles import it directly — no local formatting functions remain. Output is compact (unpadded); callers handle alignment via `format!`.

---

## Implementation

| Console | Crate | Section Width | Rendering |
|---------|-------|--------------|-----------|
| Server | `crates/console/` | 79ch (5 segments) | egui window, monospace label |
| Client overlay | `crates/client/` diagnostics | 47ch (3 segments) | egui area, monospace label |

Both build a `String` buffer using the grid math above and render it as a single `egui::RichText` label with a monospace font. The grid constants (`SEG_WIDTH`, `SEG_GAP`) are still per-crate; formatting functions are shared via `common::numfmt`.

---

## Implementation Deviations

None currently — implementation matches spec.

## Implementation Gaps

- Sparkline rendering in client overlay (segment 2 reserved but empty)
- Grid constants (`SEG_WIDTH`, `SEG_GAP`) still duplicated per-crate; extraction to shared location deferred