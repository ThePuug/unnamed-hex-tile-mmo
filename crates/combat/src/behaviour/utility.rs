//! Utility scoring, the Infinite Axis Utility System (Dave Mark, Mike
//! Lewis): a decision scores the product of its considerations, each one
//! input scaled between two bounds and mapped through a response curve,
//! and in each of an NPC's channels the highest-scoring decision is taken
//! where it beats doing nothing.
//!
//! A consideration at 0 vetoes its decision. A product falls as terms are
//! added, so a score is compensated for its count ([`score`]) and a
//! decision with many considerations does not lose to one with few for
//! that alone. A consideration fully met, a condition that holds, adds
//! nothing to the count, so it moves no score.

/// The shape a response curve rises by over its input, scaled to 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// `x^power`: 1 is a straight line, above it rises late, below early
    Power(f32),
    /// An S through `mid`, `steep` sharp, pinned to 0 and 1 at the bounds
    Logistic { mid: f32, steep: f32 },
}

/// A response curve: its shape, turned to fall where `falling`, lifted so
/// it never drops below `floor`. A curve with a floor never vetoes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub shape: Shape,
    pub falling: bool,
    pub floor: f32,
}

impl Curve {
    /// A straight rise from 0 to 1
    pub const RISING: Curve = Curve { shape: Shape::Power(1.0), falling: false, floor: 0.0 };
    /// A straight fall from 1 to 0
    pub const FALLING: Curve = Curve { shape: Shape::Power(1.0), falling: true, floor: 0.0 };

    /// This curve lifted to `floor`
    pub const fn floored(self, floor: f32) -> Curve {
        Curve { floor, ..self }
    }

    /// The response at `x`, an input already scaled to 0..=1
    pub fn at(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let x = if self.falling { 1.0 - x } else { x };
        let y = match self.shape {
            Shape::Power(power) => x.powf(power),
            Shape::Logistic { mid, steep } => {
                let s = |x: f32| 1.0 / (1.0 + (-steep * (x - mid)).exp());
                (s(x) - s(0.0)) / (s(1.0) - s(0.0))
            }
        };
        self.floor + (1.0 - self.floor) * y.clamp(0.0, 1.0)
    }
}

/// One input to a decision, as data: where it is read from, the bounds it
/// is scaled between, and the curve that answers it.
///
/// Every consideration is a curve each archetype's mind carries and every
/// search tunes, so one is added only after it is discussed. A number
/// another already reads is read through it, sharing its setting as
/// `leash_left` does across both channels.
#[derive(Debug)]
pub struct Consideration<V> {
    pub name: &'static str,
    pub read: fn(&V) -> f32,
    /// The input at which the curve starts, and the one at which it ends;
    /// either way round
    pub bounds: (f32, f32),
    pub curve: Curve,
}

impl<V> Clone for Consideration<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for Consideration<V> {}

impl<V> Consideration<V> {
    /// Its response to what `view` reads
    pub fn answer(&self, view: &V) -> f32 {
        let (from, to) = self.bounds;
        let raw = (self.read)(view);
        let x = if to == from { if raw >= to { 1.0 } else { 0.0 } } else { (raw - from) / (to - from) };
        self.curve.at(x)
    }
}

/// A decision's score from its considerations' responses, `weight` times
/// their product compensated for `n`, the count of those short of 1: the
/// product's shortfall is made up by `1 − 1/n` of itself times the
/// product, so many terms weigh as few would. Stops at the first 0, which
/// vetoes it.
pub fn score(weight: f32, responses: impl IntoIterator<Item = f32>) -> f32 {
    let (mut product, mut count) = (1.0_f32, 0_u32);
    for response in responses {
        if response <= 0.0 {
            return 0.0;
        }
        if response < 1.0 {
            product *= response;
            count += 1;
        }
    }
    if count == 0 {
        return weight;
    }
    let makeup = 1.0 - 1.0 / count as f32;
    weight * (product + (1.0 - product) * makeup * product)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rises(curve: Curve) -> bool {
        (0..10).all(|i| curve.at(i as f32 / 10.0) <= curve.at((i + 1) as f32 / 10.0))
    }

    #[test]
    fn every_shape_runs_from_its_floor_to_one_and_rises_or_falls_throughout() {
        let shapes = [
            Shape::Power(1.0),
            Shape::Power(3.0),
            Shape::Power(0.3),
            Shape::Logistic { mid: 0.5, steep: 10.0 },
            Shape::Logistic { mid: 0.2, steep: 4.0 },
        ];
        for shape in shapes {
            let up = Curve { shape, falling: false, floor: 0.0 };
            assert!((up.at(0.0)).abs() < 1e-4 && (up.at(1.0) - 1.0).abs() < 1e-4, "{shape:?} spans 0..1");
            assert!(rises(up), "{shape:?} rises");
            let down = Curve { falling: true, floor: 0.3, ..up };
            assert!((down.at(0.0) - 1.0).abs() < 1e-4 && (down.at(1.0) - 0.3).abs() < 1e-4, "{shape:?} falls to its floor");
            assert!((0..10).all(|i| down.at(i as f32 / 10.0) >= down.at((i + 1) as f32 / 10.0)), "{shape:?} falls");
        }
    }

    #[test]
    fn a_consideration_scales_its_input_between_its_bounds_either_way_round() {
        let read = |x: &f32| *x;
        let up = Consideration { name: "up", read, bounds: (10.0, 20.0), curve: Curve::RISING };
        assert_eq!(up.answer(&5.0), 0.0);
        assert!((up.answer(&15.0) - 0.5).abs() < 1e-6);
        assert_eq!(up.answer(&30.0), 1.0);
        let down = Consideration { bounds: (20.0, 10.0), ..up };
        assert!((down.answer(&12.0) - 0.8).abs() < 1e-6);
        let step = Consideration { bounds: (1.0, 1.0), ..up };
        assert_eq!((step.answer(&0.99), step.answer(&1.0)), (0.0, 1.0), "equal bounds make a step");
    }

    #[test]
    fn a_zero_vetoes_and_a_long_list_is_not_beaten_for_its_length() {
        assert_eq!(score(1.0, [0.9, 0.0, 0.9]), 0.0);
        let few = score(1.0, [0.8, 0.8]);
        let many = score(1.0, [0.8; 6]);
        let raw = 0.8_f32.powi(6);
        assert!(many > raw * 1.5 && many < few, "made up for its count: {many} against {few}, {raw} raw");
        assert!(score(1.0, [1.0; 5]) == 1.0, "a perfect score stays perfect");
        assert_eq!(score(1.0, [0.5, 1.0, 1.0]), score(1.0, [0.5]), "a condition met moves nothing");
        assert!(score(2.0, [0.5]) > score(1.0, [0.9]), "a weight sets the class");
    }
}
