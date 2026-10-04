//! How a part varies its bars. A part that plays one bar over and over
//! is a loop machine however the layers around it come and go — the
//! listener hears one riff, and every piece sounds like it. Players vary
//! by the phrase: a four-bar phrase is its first bar, a variant on the
//! second — one stroke moved, added or dropped — a plain third, and a
//! cadence on the fourth, where a drum fills or rolls, a bass walks into
//! the next chord, a pluck holds or anticipates, brass pushes. A variant
//! is never played two bars running. The build fails a part that plays
//! one bar more than a phrase running; what the variation is, is each
//! piece's, in its instruments' own idiom.

use crate::score::{Note, Score, TICKS_PER_EIGHTH as E};
use crate::theory::phrase;

/// What a bar is to its phrase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    First,
    Variant,
    Plain,
    Cadence,
}

/// Bar `bar`'s role in its phrase; phrases start at the loop's first bar.
pub fn role(bar: u32) -> Role {
    match bar % phrase::BARS {
        0 => Role::First,
        1 => Role::Variant,
        2 => Role::Plain,
        _ => Role::Cadence,
    }
}

/// A roll of `count` strokes on `pitch`, sixteenths from `start`, each
/// struck at the velocity `vel` gives its place — 0 the first, 1 the last
/// — so it grows into the bar it leads to.
pub fn roll(score: &mut Score, channel: u8, pitch: u8, start: u32, count: u32, mut vel: impl FnMut(f32) -> u8) {
    for k in 0..count {
        let x = if count > 1 { k as f32 / (count - 1) as f32 } else { 1.0 };
        score.add(Note { start: start + k * E / 2, len: E / 2 - 10, pitch, vel: vel(x), channel });
    }
}
