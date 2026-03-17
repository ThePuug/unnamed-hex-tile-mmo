/// Configurable number formatter for fixed-width character budgets.
///
/// Formats `f64` values into compact strings using the notation appropriate for
/// the configured width budget. The `width` field drives formatting decisions
/// (when to suffix, how many decimal places) but does NOT pad the output —
/// callers handle alignment via `format!("{:>w$}", ...)`.
///
/// Three orthogonal axes:
/// - **width**: character budget driving notation decisions (typically 3, 5, or 7)
/// - **precision**: how fractional digits are handled
/// - **overflow**: what happens when the value exceeds the budget

const SUFFIXES: [(f64, &str); 4] = [(1e3, "K"), (1e6, "M"), (1e9, "B"), (1e12, "T")];

// ── Types ──

/// How fractional digits are handled within the available character budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// Round to integer. No decimal point emitted in the raw range.
    Integer,
    /// Start at maximum decimal precision and reduce until the result fits.
    /// Gracefully degrades from 2dp → 1dp → 0dp as magnitude grows.
    Collapsing,
    /// Always emit exactly `n` digits after the decimal point in the raw range.
    /// Suffixed tiers use exactly `n-1` dp to maintain decimal alignment.
    Fixed(u8),
}

/// What happens when the value exceeds what raw formatting can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Scale down using K/M/B/T suffix tiers, each consuming 1 character.
    Suffix,
    /// Clamp to the maximum/minimum displayable integer and show that.
    Clamp,
}

/// Fixed-width number formatter.
///
/// Use the provided presets or construct directly for custom widths.
#[derive(Debug, Clone, Copy)]
pub struct NumFmt {
    pub width: usize,
    pub precision: Precision,
    pub overflow: Overflow,
}

// ── Presets ──

/// 5ch integer with suffix. Counts, peaks — integer metrics in a half-segment.
pub const INT5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };

/// 5ch collapsing decimal with suffix. Timings, ratios — decimal metrics in a half-segment.
pub const DEC5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };

/// 5ch integer with clamp. Coordinates, raw counts — values that shouldn't suffix.
pub const CLAMP5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Clamp };

// ── Implementation ──

impl NumFmt {
    /// Construct with validation. Panics if width is too narrow for the precision/overflow combo.
    ///
    /// Minimum width per precision: Integer=1, Collapsing=2 (digit+point), Fixed(n)=n+2.
    /// Suffix adds 1 to the minimum (the suffix character itself).
    pub fn new(width: usize, precision: Precision, overflow: Overflow) -> Self {
        let precision_min = match precision {
            Precision::Integer => 1,
            Precision::Collapsing => 2,
            Precision::Fixed(n) => n as usize + 2,
        };
        let suffix_cost = match overflow {
            Overflow::Suffix => 1,
            Overflow::Clamp => 0,
        };
        let min_width = precision_min + suffix_cost;
        assert!(
            width >= min_width,
            "NumFmt: width {width} too narrow for {precision:?}/{overflow:?} (minimum {min_width})"
        );
        Self { width, precision, overflow }
    }

    /// Format a value into a compact string. Output is NOT padded — callers
    /// handle alignment via `format!("{:>w$}", ...)` or `format!("{:^w$}", ...)`.
    pub fn fmt(&self, v: f64) -> String {
        // Handle near-zero for decimal modes
        match self.precision {
            Precision::Collapsing => {
                let dp = self.max_raw_decimals();
                let threshold = 0.5 * f64::powi(10.0, -(dp as i32));
                if v.abs() < threshold {
                    return format!("{:.prec$}", 0.0, prec = dp as usize);
                }
            }
            Precision::Fixed(n) => {
                let threshold = 0.5 * f64::powi(10.0, -(n as i32));
                if v.abs() < threshold {
                    return format!("{:.prec$}", 0.0, prec = n as usize);
                }
            }
            Precision::Integer => {
                if v.round() == 0.0 {
                    return "0".into();
                }
            }
        }

        // Try raw range first
        if let Some(s) = self.try_raw(v) {
            return s;
        }

        // Overflow handling
        match self.overflow {
            Overflow::Suffix => self.fmt_suffix(v),
            Overflow::Clamp => self.fmt_clamp(v),
        }
    }

    /// Try to format the value without a suffix (raw range).
    ///
    /// Raw range caps at `width-1` digits to ensure suffix tiers handle larger values
    /// (e.g. width=5: raw up to 9999, 10000+ goes to suffix as "10.0K").
    fn try_raw(&self, v: f64) -> Option<String> {
        let w = self.width;
        let max_raw = self.max_raw_abs();
        match self.precision {
            Precision::Integer => {
                let r = v.round();
                if r.abs() > max_raw { return None; }
                Some(format!("{r:.0}"))
            }
            Precision::Collapsing => {
                let max_dp = self.max_raw_decimals();
                for prec in (0..=max_dp).rev() {
                    if prec == 0 {
                        let r = v.round();
                        if r.abs() > max_raw { continue; }
                        let s = format!("{r:.0}");
                        if s.len() <= w {
                            return Some(s);
                        }
                    } else {
                        let s = format!("{v:.prec$}", prec = prec as usize);
                        if s.len() <= w {
                            return Some(s);
                        }
                    }
                }
                None
            }
            Precision::Fixed(n) => {
                let s = format!("{v:.prec$}", prec = n as usize);
                if s.len() <= w { Some(s) } else { None }
            }
        }
    }

    /// Format with K/M/B/T suffix tiers.
    fn fmt_suffix(&self, v: f64) -> String {
        let w = self.width;
        let a = v.abs();
        let sign = if v < 0.0 { "-" } else { "" };

        // Determine precision range for suffix tiers.
        // Fixed(n): use exactly n-1 dp in suffix (maintains decimal alignment).
        // Integer/Collapsing: try 2dp → 1dp → 0dp, taking first that fits.
        let (min_sfx_prec, max_sfx_prec) = match self.precision {
            Precision::Fixed(n) if n > 0 => (n as usize - 1, n as usize - 1),
            _ => (0, 2),
        };

        for &(div, sfx) in &SUFFIXES {
            let scaled = a / div;
            if scaled >= 1000.0 { continue; } // belongs to next tier

            for prec in (min_sfx_prec..=max_sfx_prec).rev() {
                // Check if rounding at this precision would push to next tier
                let factor = 10_f64.powi(prec as i32);
                if (scaled * factor).round() / factor >= 1000.0 { continue; }

                let num = format!("{scaled:.prec$}");
                let s = format!("{sign}{num}{sfx}");
                if s.len() <= w {
                    return s;
                }

                // Try dropping leading zero: "0.1K" → ".1K" (for narrow widths)
                if prec > 0 && num.starts_with("0.") {
                    let compact = format!("{sign}.{}{sfx}", &num[2..]);
                    if compact.len() <= w {
                        return compact;
                    }
                }
            }
        }

        // Fallback
        "?".into()
    }

    /// Clamp to the maximum/minimum displayable integer value.
    fn fmt_clamp(&self, v: f64) -> String {
        let w = self.width;
        let max_pos = 10_i64.pow(w as u32) - 1;
        let max_neg = -(10_i64.pow(w.saturating_sub(1) as u32) - 1);

        let clamped = if v > 0.0 {
            (v.round() as i64).min(max_pos)
        } else {
            (v.round() as i64).max(max_neg)
        };
        format!("{clamped}")
    }

    /// Maximum absolute value for the raw (unsuffixed) integer range.
    /// width=5 → 9999, width=3 → 99, width=7 → 999999.
    /// This is `10^(width-1) - 1`: suffix tiers handle anything larger.
    fn max_raw_abs(&self) -> f64 {
        (10_f64.powi(self.width as i32 - 1)) - 1.0
    }

    /// Maximum decimal places for collapsing mode's raw range.
    /// Chosen so that positive and negative near-zero values use the same dp,
    /// giving symmetric display around zero.
    /// width=5 → 2dp ("0.50" / "-0.50"), width=7 → 4dp, width=3 → 0dp.
    fn max_raw_decimals(&self) -> u8 {
        // Negative near-zero: "-0." + dp = 3 + dp chars.
        // For symmetry: 3 + dp <= width → dp <= width - 3.
        self.width.saturating_sub(3) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Width invariant: output never exceeds the configured width ──

    fn assert_max_width(fmt: &NumFmt, cases: &[f64]) {
        for &v in cases {
            let s = fmt.fmt(v);
            assert!(
                s.len() <= fmt.width,
                "{:?}.fmt({}) = {:?} (len {}, max {})",
                fmt, v, s, s.len(), fmt.width
            );
        }
    }

    const WIDE_RANGE: &[f64] = &[
        0.0, 0.001, 0.043, 0.5, 1.23, 9.99, 10.0, 92.6, 99.9, 100.0, 714.0, 999.0,
        1234.0, 9999.0, 10_000.0, 25148.0, 99_999.0, 100_000.0, 999_999.0,
        1_234_567.0, 10_000_000.0, 999_999_999.0, 1e12, 5e14,
        -0.5, -1.23, -9.99, -92.6, -714.0, -5678.0, -9999.0, -10_000.0,
        -99_999.0, -1_234_567.0,
    ];

    // ── Width tests across all axis combinations ──

    #[test]
    fn width_integer_suffix() {
        for w in [3, 5, 7] {
            assert_max_width(&NumFmt { width: w, precision: Precision::Integer, overflow: Overflow::Suffix }, WIDE_RANGE);
        }
    }

    #[test]
    fn width_collapsing_suffix() {
        for w in [3, 5, 7] {
            assert_max_width(&NumFmt { width: w, precision: Precision::Collapsing, overflow: Overflow::Suffix }, WIDE_RANGE);
        }
    }

    #[test]
    fn width_fixed_suffix() {
        for (w, n) in [(5, 2), (7, 3)] {
            assert_max_width(&NumFmt { width: w, precision: Precision::Fixed(n), overflow: Overflow::Suffix }, WIDE_RANGE);
        }
    }

    #[test]
    fn width_integer_clamp() {
        for w in [3, 5, 7] {
            assert_max_width(&NumFmt { width: w, precision: Precision::Integer, overflow: Overflow::Clamp }, WIDE_RANGE);
        }
    }

    // ── 5ch collapsing decimal + suffix ──

    #[test]
    fn collapsing_5ch_known_outputs() {
        let f = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
        assert_eq!(f.fmt(0.0), "0.00");
        assert_eq!(f.fmt(0.5), "0.50");
        assert_eq!(f.fmt(92.6), "92.60");
        assert_eq!(f.fmt(714.0), "714.0");
        assert_eq!(f.fmt(1234.0), "1234");
        assert_eq!(f.fmt(9999.0), "9999");
        assert_eq!(f.fmt(10_000.0), "10.0K");
        assert_eq!(f.fmt(25148.0), "25.1K");
        assert_eq!(f.fmt(100_000.0), "100K");
        assert_eq!(f.fmt(1_234_567.0), "1.23M");
        assert_eq!(f.fmt(-50.3), "-50.3");
        assert_eq!(f.fmt(-714.0), "-714");
        assert_eq!(f.fmt(-5678.0), "-5678");
        assert_eq!(f.fmt(-56789.0), "-57K");
    }

    // ── 5ch integer + suffix ──

    #[test]
    fn integer_5ch_known_outputs() {
        let f = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
        assert_eq!(f.fmt(0.0), "0");
        assert_eq!(f.fmt(42.0), "42");
        assert_eq!(f.fmt(999.0), "999");
        assert_eq!(f.fmt(1000.0), "1000");
        assert_eq!(f.fmt(9999.0), "9999");
        assert_eq!(f.fmt(10_000.0), "10.0K");
        assert_eq!(f.fmt(25000.0), "25.0K");
        assert_eq!(f.fmt(100_000.0), "100K");
        assert_eq!(f.fmt(-999.0), "-999");
        assert_eq!(f.fmt(-5678.0), "-5678");
        assert_eq!(f.fmt(-10_000.0), "-10K");
    }

    // ── 3ch integer + suffix (leading-zero-drop notation) ──

    #[test]
    fn integer_3ch_known_outputs() {
        let f = NumFmt { width: 3, precision: Precision::Integer, overflow: Overflow::Suffix };
        assert_eq!(f.fmt(0.0), "0");
        assert_eq!(f.fmt(99.0), "99");
        assert_eq!(f.fmt(100.0), ".1K");
        assert_eq!(f.fmt(500.0), ".5K");
        assert_eq!(f.fmt(999.0), "1K");
        assert_eq!(f.fmt(1000.0), "1K");
        assert_eq!(f.fmt(99_000.0), "99K");
        assert_eq!(f.fmt(100_000.0), ".1M");
        assert_eq!(f.fmt(-1.0), "-1");
        assert_eq!(f.fmt(-99.0), "-99");
    }

    // ── 7ch fixed-3dp + suffix (decimal at column 4) ──

    #[test]
    fn fixed3_7ch_known_outputs() {
        let f = NumFmt { width: 7, precision: Precision::Fixed(3), overflow: Overflow::Suffix };
        assert_eq!(f.fmt(0.0), "0.000");
        assert_eq!(f.fmt(5.0), "5.000");
        assert_eq!(f.fmt(0.5), "0.500");
        assert_eq!(f.fmt(92.6), "92.600");
        assert_eq!(f.fmt(999.999), "999.999");
        assert_eq!(f.fmt(1000.0), "1.00K");
        assert_eq!(f.fmt(9990.0), "9.99K");
        assert_eq!(f.fmt(10_000.0), "10.00K");
        assert_eq!(f.fmt(999_990.0), "999.99K");
        assert_eq!(f.fmt(1_000_000.0), "1.00M");
        assert_eq!(f.fmt(-0.5), "-0.500");
        assert_eq!(f.fmt(-9.999), "-9.999");
        assert_eq!(f.fmt(-99.999), "-99.999");
    }

    // ── Integer + clamp ──

    #[test]
    fn clamp_5ch_known_outputs() {
        let f = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Clamp };
        assert_eq!(f.fmt(0.0), "0");
        assert_eq!(f.fmt(42.0), "42");
        assert_eq!(f.fmt(99999.0), "99999");
        assert_eq!(f.fmt(100000.0), "99999"); // clamped
        assert_eq!(f.fmt(-42.0), "-42");
        assert_eq!(f.fmt(-9999.0), "-9999");
        assert_eq!(f.fmt(-10000.0), "-9999"); // clamped
    }

    #[test]
    fn clamp_7ch_known_outputs() {
        let f = NumFmt { width: 7, precision: Precision::Integer, overflow: Overflow::Clamp };
        assert_eq!(f.fmt(0.0), "0");
        assert_eq!(f.fmt(9999999.0), "9999999");
        assert_eq!(f.fmt(99999999.0), "9999999"); // clamped
        assert_eq!(f.fmt(-999999.0), "-999999");
        assert_eq!(f.fmt(-9999999.0), "-999999"); // clamped
    }

    // ── Suffix tier transitions ──

    #[test]
    fn suffix_transitions() {
        let f = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
        assert_eq!(f.fmt(999_000.0), "999K");
        assert_eq!(f.fmt(1_000_000.0), "1.00M");
        assert_eq!(f.fmt(999_000_000.0), "999M");
        assert_eq!(f.fmt(1_000_000_000.0), "1.00B");
        assert_eq!(f.fmt(999_000_000_000.0), "999B");
        assert_eq!(f.fmt(1_000_000_000_000.0), "1.00T");
    }

    // ── Construction validation ──

    #[test]
    #[should_panic(expected = "too narrow")]
    fn rejects_degenerate_width() {
        NumFmt::new(1, Precision::Collapsing, Overflow::Suffix);
    }

    #[test]
    #[should_panic(expected = "too narrow")]
    fn rejects_fixed_too_narrow() {
        NumFmt::new(3, Precision::Fixed(3), Overflow::Suffix);
    }

    #[test]
    fn accepts_valid_narrow() {
        let _ = NumFmt::new(2, Precision::Integer, Overflow::Suffix);
        let _ = NumFmt::new(3, Precision::Collapsing, Overflow::Suffix);
    }

    // ── Caller-side padding ──

    #[test]
    fn caller_pads() {
        let i5 = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
        let d5 = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
        let c5 = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Clamp };
        assert_eq!(format!("{:>5}", i5.fmt(42.0)), "   42");
        assert_eq!(format!("{:>5}", d5.fmt(0.5)), " 0.50");
        assert_eq!(format!("{:^7}", format!("q:{}", c5.fmt(1.0))), "  q:1  ");
        assert_eq!(format!("{:^7}", format!("q:{}", c5.fmt(999.0))), " q:999 ");
    }
}
