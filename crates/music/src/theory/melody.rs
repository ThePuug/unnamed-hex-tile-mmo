//! A theme and what a phrase makes of it. A theme is a skeleton of
//! degrees on the strong beats of two bars, in one of a few shapes a
//! listener follows — an arch, a descent to the finalis, a circling, a
//! leap and its recovery, a wave, and the blues' own, the
//! fall from the fifth and the riff round the tonic — with the
//! feet between filled by steps toward the next skeleton tone, so a
//! leap is only ever the shape's own and every leap is answered. A
//! piece draws from the shapes its style sings in. Degrees are from the
//! theme's home; a piece puts them in a register.

use super::groove::Groove;
use super::phrase::{Form, Slot};
use super::Meter;
use crate::rng::Rng;

/// A tone of a tune: its onset and length in eighths within its bar,
/// and its degree from home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tone {
    pub onset: u32,
    pub len: u32,
    pub degree: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Arch,
    Descent,
    Circling,
    LeapBack,
    Wave,
    /// The blues' shout and fall: from the fifth down the pentatonic,
    /// under the tonic to the flat seventh, and home.
    Tumble,
    /// The blues' riff round the tonic: up to the third, back, under to
    /// the flat seventh, home.
    Riff,
    /// An anthem's rise: up by steps to the fifth, a turn, and the fifth
    /// held, the open ending a speed metal chorus asks on.
    Rise,
    /// A hook's fall in thirds by steps: from the fifth down, a step back
    /// up between, to home.
    Call,
}

/// The shapes a Balkan tune sings in: each comes down to its finalis at
/// the bottom of its range, and none climbs to a peak to resolve there,
/// which is a Western tune's way.
pub const FOLK_SHAPES: [Shape; 5] = [Shape::Arch, Shape::Descent, Shape::Circling, Shape::LeapBack, Shape::Wave];

/// The shapes a blues sings in, the fall the most often; counted in
/// the pentatonic's steps, each keeps within the fifth over home, the
/// core a blues line moves in.
pub const BLUES_SHAPES: [Shape; 4] = [Shape::Tumble, Shape::Riff, Shape::Tumble, Shape::Wave];

/// The shapes a speed metal tune sings in, as Helloween's choruses do:
/// mostly by step, an octave wide at most, rising to its height or
/// falling from it in a sequence.
pub const SPEED_SHAPES: [Shape; 2] = [Shape::Rise, Shape::Call];

impl Shape {
    /// The shape over six strong beats; a skeleton of another length
    /// samples it.
    fn table(self) -> [i32; 6] {
        match self {
            Shape::Arch => [0, 2, 4, 4, 2, 0],
            Shape::Descent => [4, 3, 2, 1, 0, 0],
            Shape::Circling => [0, 1, 0, -1, 0, 1],
            Shape::LeapBack => [0, 4, 3, 2, 1, 0],
            Shape::Wave => [0, 2, 1, 3, 2, 0],
            Shape::Tumble => [3, 2, 1, 0, -1, 0],
            Shape::Riff => [0, 2, 0, -1, 0, 0],
            Shape::Rise => [0, 1, 2, 4, 3, 4],
            Shape::Call => [4, 2, 3, 1, 2, 0],
        }
    }

    /// A degree from home for each of `n` strong beats.
    pub fn skeleton(self, n: usize) -> Vec<i32> {
        let t = self.table();
        (0..n).map(|i| t[(i as f32 * 5.0 / (n.max(2) - 1) as f32).round() as usize]).collect()
    }
}

/// A theme: a shape on the strong beats of two bars, and each bar's feet.
#[derive(Clone, Debug)]
pub struct Theme {
    pub shape: Shape,
    pub feet: [Vec<(u32, u32)>; 2],
}

impl Theme {
    /// A theme in one of `shapes` on the groove's feet.
    pub fn draw(groove: &Groove, shapes: &[Shape], rng: &mut Rng) -> Theme {
        Theme { shape: shapes[rng.below(shapes.len())], feet: [groove.feet(rng), groove.feet(rng)] }
    }

    /// The theme's two bars as tones: the skeleton on each group's first
    /// foot, the feet after it stepping toward the next skeleton tone —
    /// a neighbour and back where the next is the same — so the tune
    /// moves by step between the shape's own leaps.
    pub fn bars(&self, meter: &Meter) -> [Vec<Tone>; 2] {
        let groups = meter.groups.len();
        let skeleton = self.shape.skeleton(2 * groups);
        let strong = meter.strong_eighths();
        let mut bars: [Vec<Tone>; 2] = [Vec::new(), Vec::new()];
        for (k, feet) in self.feet.iter().enumerate() {
            let mut group = 0usize;
            let mut in_group = 0i32;
            for (onset, len) in feet {
                if group + 1 < groups && *onset >= strong[group + 1] {
                    group += 1;
                    in_group = 0;
                }
                let here = skeleton[k * groups + group];
                let next = skeleton[(k * groups + group + 1) % skeleton.len()];
                let extra = feet.iter().filter(|(o, _)| *o >= strong[group] && strong.get(group + 1).is_none_or(|s| *o < *s)).count() as i32 - 1;
                let degree = if in_group == 0 {
                    here
                } else if next == here {
                    here + (in_group % 2)
                } else {
                    here + ((next - here) as f32 * in_group as f32 / (extra + 1) as f32).round() as i32
                };
                bars[k].push(Tone { onset: *onset, len: *len, degree });
                in_group += 1;
            }
        }
        bars
    }
}

/// A bar as a singer takes it: in each group at most the skeleton
/// tone, held, and one more — the group's last foot where that is a
/// step from the skeleton, else the foot nearest the middle of the
/// way, so a run of steps thins to a step and not to a leap. A voice
/// holds where an instrument runs.
pub fn sung(bar: &[Tone], meter: &Meter) -> Vec<Tone> {
    let strong = meter.strong_eighths();
    let mut out = Vec::new();
    for (g, start) in strong.iter().enumerate() {
        let end = strong.get(g + 1).copied().unwrap_or(meter.eighths());
        let group: Vec<&Tone> = bar.iter().filter(|t| *start <= t.onset && t.onset < end).collect();
        match group.as_slice() {
            [] => {}
            [one] => out.push(**one),
            [first, rest @ ..] => {
                let last = rest[rest.len() - 1];
                let second = if (last.degree - first.degree).abs() <= 2 {
                    last
                } else {
                    let midway = (first.degree + last.degree) as f32 / 2.0;
                    rest.iter().min_by_key(|t| ((t.degree as f32 - midway).abs() * 2.0) as i32).unwrap()
                };
                out.push(Tone { onset: first.onset, len: second.onset - first.onset, degree: first.degree });
                out.push(Tone { onset: second.onset, len: end - second.onset, degree: second.degree });
            }
        }
    }
    out
}

/// A bar's tones every one `shift` degrees away.
pub fn sequence(bar: &[Tone], shift: i32) -> Vec<Tone> {
    bar.iter().map(|t| Tone { degree: t.degree + shift, ..*t }).collect()
}

/// A bar whose last tone is `end`, the tones before it drawn toward it
/// until each step is a step, so a cadence is approached and not
/// leapt at; the first tone, the bar's strong beat, stays.
pub fn ending(bar: &[Tone], end: i32) -> Vec<Tone> {
    let mut out = bar.to_vec();
    let n = out.len();
    if n == 0 {
        return out;
    }
    out[n - 1].degree = end;
    for i in (1..n - 1).rev() {
        let next = out[i + 1].degree;
        let gap = out[i].degree - next;
        if gap.abs() > 2 {
            out[i].degree = next + 2 * gap.signum();
        }
    }
    out
}

/// A bar coming down to `end` from `from`, one tone a group: toward
/// the cadence figure — by step from above through the tone over the
/// end to the tone under it, then the end — at most two degrees a
/// step, the last tone `end` however far that leaves it, and never the
/// end a group early, where the neighbour on the line's side stands
/// instead; how a sentence comes to its close, by step, with any leap
/// saved for the arrival, and never on a tone struck again and again.
pub fn cadence(meter: &Meter, from: i32, end: i32) -> Vec<Tone> {
    let strong = meter.strong_eighths();
    let groups = strong.len();
    let figure = |g: usize| if g + 1 == groups { end } else if g + 2 == groups { end - 1 } else { end + (groups - 2 - g) as i32 };
    let mut prev = from;
    strong
        .iter()
        .enumerate()
        .map(|(g, onset)| {
            let len = strong.get(g + 1).copied().unwrap_or(meter.eighths()) - onset;
            let mut degree = if g + 1 == groups { end } else { prev + (figure(g) - prev).clamp(-2, 2) };
            if g + 1 < groups && degree == end {
                degree = if prev > end { end + 1 } else { end - 1 };
            }
            prev = degree;
            Tone { onset: *onset, len, degree }
        })
        .collect()
}

/// Four bars of the theme in `form`: a period says the theme twice,
/// its second bar ending on `mid` the first time and on `end` the
/// second; a sentence says the first bar, again a step up, the second
/// bar, and comes down to `end`. Each half-phrase's last tone gives up
/// an eighth or two to a breath, since a line that never rests is an
/// instrument and not a singer.
pub fn phrase(theme: &Theme, meter: &Meter, form: Form, mid: i32, end: i32) -> Vec<Vec<Tone>> {
    let bars = theme.bars(meter);
    let slots = form.slots();
    let mut out: Vec<Vec<Tone>> = Vec::new();
    for (i, slot) in slots.iter().enumerate() {
        let bar = match slot {
            Slot::Idea(k) => {
                let b = &bars[*k];
                if form == Form::Period && *k == 1 {
                    ending(b, if i == 1 { mid } else { end })
                } else {
                    b.clone()
                }
            }
            Slot::Sequence => sequence(&bars[0], 1),
            Slot::Cadence => {
                let from = out.last().and_then(|b| b.last()).map_or(0, |t| t.degree);
                cadence(meter, from, end)
            }
        };
        out.push(bar);
    }
    for i in [1, 3] {
        if let Some(last) = out[i].last_mut() {
            last.len -= (last.len - 1).min(2);
        }
    }
    out
}

/// A degree's place on a line: `from` stepping to `to` over `n` steps,
/// a step or a third at a time, turning about the way where there are
/// more steps than the distance needs, so a run never stands still and
/// never leaps.
pub fn run_between(from: i32, to: i32, n: u32) -> Vec<i32> {
    let delta = to - from;
    (1..n)
        .map(|i| {
            let straight = from + (delta as f32 * i as f32 / n as f32).round() as i32;
            let turn = if delta.unsigned_abs() * 2 < n { [0, 1, 0, -1][i as usize % 4] } else { 0 };
            straight + turn
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::counterpoint::unrecovered_leap;
    use super::super::groove;
    use super::super::phrase::FORMS;
    use super::*;

    /// Every shape at every length opens at home or on a leap it then
    /// recovers, and never wanders past a sixth.
    #[test]
    fn shapes_hold_their_span() {
        for shape in FOLK_SHAPES.iter().chain(BLUES_SHAPES.iter()).chain(SPEED_SHAPES.iter()) {
            for n in [4, 6, 8, 10] {
                let s = shape.skeleton(n);
                assert_eq!(s.len(), n);
                assert!(s.iter().all(|d| (-1..=5).contains(d)), "{shape:?} {s:?}");
                assert_eq!(unrecovered_leap(&s), None, "{shape:?} {s:?}");
            }
        }
    }

    /// A phrase in every form on every dance keeps every leap answered
    /// and ends where it was told to.
    #[test]
    fn every_phrase_answers_its_leaps_and_ends_as_told() {
        for groove in groove::all() {
            let meter = groove.meter();
            for seed in 0..24 {
                let theme = Theme::draw(groove, &FOLK_SHAPES, &mut Rng::new(seed));
                for form in FORMS {
                    // Endings as a piece gives them: the open chord's tone
                    // nearest home, or home.
                    for (mid, end) in [(1, 0), (-1, 0), (-2, 1), (1, -2)] {
                        let bars = phrase(&theme, &meter, form, mid, end);
                        let degrees: Vec<i32> = bars.iter().flatten().map(|t| t.degree).collect();
                        assert_eq!(unrecovered_leap(&degrees), None, "{} {form:?} seed {seed}: {degrees:?}", groove.name);
                        let sung_line: Vec<i32> = bars.iter().flat_map(|b| sung(b, &meter)).map(|t| t.degree).collect();
                        assert_eq!(unrecovered_leap(&sung_line), None, "{} {form:?} seed {seed} sung: {sung_line:?}", groove.name);
                        assert_eq!(bars[3].last().unwrap().degree, end);
                        for (i, bar) in bars.iter().enumerate() {
                            let covered: u32 = bar.iter().map(|t| t.len).sum();
                            let breath = if i % 2 == 1 { 2 } else { 0 };
                            assert!(covered <= groove.eighths() && covered + breath >= groove.eighths(), "{}: a bar with a hole", groove.name);
                        }
                    }
                }
            }
        }
    }

    /// A run steps or skips a third, never leaps, and arrives.
    #[test]
    fn a_run_never_leaps() {
        for (from, to, n) in [(0, 7, 4), (3, 3, 6), (5, 0, 4), (0, 1, 6), (2, -3, 8)] {
            let line: Vec<i32> = std::iter::once(from).chain(run_between(from, to, n)).chain(std::iter::once(to)).collect();
            for w in line.windows(2) {
                assert!((w[1] - w[0]).abs() <= 2, "{from}→{to} over {n}: {line:?}");
            }
        }
    }
}
