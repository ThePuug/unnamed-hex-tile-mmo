//! What an instrument goes through between the player's hands and the
//! stage: a guitar's pedal, its amp and the cabinet the amp drives, and
//! the delay after them. The instrument is the recording (`voices`); the
//! player is the hands (`players`); the rig is the signal path, tuned here
//! once for every part that plays through it, and `amp` plays one.
//!
//! A rig shapes only an instrument recorded at the jack (`Voice::direct`),
//! which sounds as a guitar's pickup does before any amp: an instrument
//! recorded through an amp already has one, and a program the default
//! bank plays has its own. A part plays through the rig its role and
//! program call for (`of`) unless its score fits another
//! (`Score::rig_up`).
//!
//! The amp follows a high-gain head of the eighties as the literature
//! models one (`proofs/research/guitar-lead-findings.md`): a mid boost
//! into the input, as a Tube Screamer pushes one — the bass under its
//! high-pass passing clean, so the low end stays tight — then cascaded
//! triode stages, each clipping a little asymmetrically, a coupling
//! high-pass between them; a tone stack; a power stage clipping softly;
//! the cabinet as a recorded impulse response.

/// A rig. Every part is optional: a rig without an amp is a clean DI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    pub name: &'static str,
    pub amp: Option<Amp>,
    pub cabinet: Option<Cabinet>,
    pub delay: Option<Delay>,
}

/// A head: the pedal before it, the gain into its first stage, its
/// stages, and its controls, dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Amp {
    pub boost: Option<Boost>,
    pub drive_db: f32,
    pub stages: &'static [Stage],
    /// The tone stack: a shelf under `BASS_HZ`, a bell at `mid_hz`, a
    /// shelf over `TREBLE_HZ`; and the power stage's presence, a shelf
    /// over `PRESENCE_HZ`.
    pub bass_db: f32,
    pub mid_db: f32,
    pub mid_hz: f32,
    pub treble_db: f32,
    pub presence_db: f32,
}

/// A mid boost before the amp: its clipping path's high-pass, its gain,
/// and the tone control's low-pass on the sum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boost {
    pub hz: f32,
    pub gain_db: f32,
    pub tone_hz: f32,
}

/// A triode stage: the coupling high-pass into it, its gain, and its
/// bias, the offset that makes its clipping asymmetric.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub low_cut_hz: f32,
    pub gain_db: f32,
    pub bias: f32,
}

/// A cabinet and the microphone on it, as an impulse response recorded
/// by jesterdyne (freesound.org/people/jesterdyne/packs/6385, CC BY 4.0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cabinet {
    /// An Engl 4x12 of Celestion Vintage 30s, an SM57 off-centre: the
    /// mid bark of a metal record.
    V30,
    /// A Hughes & Kettner Celestion 4x12, an SM57 at the centre: rounder,
    /// its top darker.
    Celestion,
}

/// A stereo delay on the amp's sound: a repeat on each side, timed in
/// eighths of the score's tempo, fed back, darkened each time round, and
/// mixed under the dry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delay {
    pub left_eighths: f32,
    pub right_eighths: f32,
    pub feedback: f32,
    pub mix: f32,
    pub tone_hz: f32,
}

pub const BASS_HZ: f32 = 120.0;
pub const TREBLE_HZ: f32 = 2500.0;
pub const PRESENCE_HZ: f32 = 4000.0;

/// A Tube Screamer's: 720 Hz under its clipping, its tone near the middle.
const SCREAMER: Boost = Boost { hz: 720.0, gain_db: 18.0, tone_hz: 4000.0 };

/// Three stages of a hot-rodded head: the second biased cold, as a
/// JCM800's is, where its asymmetry comes from.
const HOT: &[Stage] = &[
    Stage { low_cut_hz: 30.0, gain_db: 14.0, bias: 0.1 },
    Stage { low_cut_hz: 60.0, gain_db: 16.0, bias: 0.35 },
    Stage { low_cut_hz: 40.0, gain_db: 8.0, bias: 0.05 },
];

/// Two stages: a plexi pushed hard, crunch rather than saturation.
const PLEXI: &[Stage] = &[Stage { low_cut_hz: 30.0, gain_db: 12.0, bias: 0.1 }, Stage { low_cut_hz: 50.0, gain_db: 12.0, bias: 0.3 }];

/// The lead's: boosted into the hot head, the mids forward, a quarter
/// and a dotted eighth either side under it.
pub const LEAD: Rig = Rig {
    name: "lead",
    amp: Some(Amp { boost: Some(SCREAMER), drive_db: 6.0, stages: HOT, bass_db: -2.0, mid_db: 3.0, mid_hz: 800.0, treble_db: 1.0, presence_db: 2.0 }),
    cabinet: Some(Cabinet::V30),
    delay: Some(Delay { left_eighths: 2.0, right_eighths: 3.0, feedback: 0.3, mix: 0.18, tone_hz: 3500.0 }),
};

/// The second lead's: the same head pushed less, through the rounder
/// cabinet, so a twin is two guitarists.
pub const LEAD_WARM: Rig = Rig {
    name: "warm lead",
    amp: Some(Amp { boost: Some(SCREAMER), drive_db: 2.0, stages: HOT, bass_db: 0.0, mid_db: 2.0, mid_hz: 650.0, treble_db: -1.0, presence_db: 0.0 }),
    cabinet: Some(Cabinet::Celestion),
    delay: Some(Delay { left_eighths: 3.0, right_eighths: 2.0, feedback: 0.3, mix: 0.18, tone_hz: 3000.0 }),
};

/// A rhythm wall's: boosted for a tight low end, the mids scooped a
/// little, dry.
pub const RHYTHM: Rig = Rig {
    name: "rhythm",
    amp: Some(Amp { boost: Some(SCREAMER), drive_db: 0.0, stages: HOT, bass_db: 1.0, mid_db: -2.0, mid_hz: 650.0, treble_db: 1.0, presence_db: 1.0 }),
    cabinet: Some(Cabinet::V30),
    delay: None,
};

/// The other rhythm's: a plexi's crunch through the rounder cabinet.
pub const CRUNCH: Rig = Rig {
    name: "crunch",
    amp: Some(Amp { boost: None, drive_db: 6.0, stages: PLEXI, bass_db: 0.0, mid_db: 1.0, mid_hz: 700.0, treble_db: 0.0, presence_db: 1.0 }),
    cabinet: Some(Cabinet::Celestion),
    delay: None,
};

/// The rig a part of `role` on `program` plays through where its score
/// fits none: General MIDI's distortion guitar into the hot head, its
/// overdriven guitar into the warmer one or the plexi; a lead with its
/// delay, a rhythm dry.
pub fn of(role: crate::score::Role, program: u8) -> &'static Rig {
    use crate::score::Role;
    match (role, program) {
        (Role::Melody, 29) => &LEAD_WARM,
        (Role::Melody, _) => &LEAD,
        (_, 29) => &CRUNCH,
        _ => &RHYTHM,
    }
}
