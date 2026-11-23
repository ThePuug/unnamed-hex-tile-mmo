//! Every track, by name, and the styles and settings they are played in.
//! A track is an improvisational interpretation of a theme, a story or a
//! script — the overworld told on a dance, a horo as a wedding band
//! plays it, the teaser's cue cut to picture: every play of it is new, and
//! every play is the track. A track is in one style and declares the
//! settings it is made to be played in; a setting is a purpose the music
//! serves, which bounds the track's choices, and a track is made to
//! support one by being taught it. Every track also plays in `None`, as
//! it is: no constraint, and no expectation either. A play is a band playing a track in a
//! setting (`Params`). Adding a track: a module with
//! `pub fn build(&Params) -> Score`, an entry in `TRACKS`, a row in the
//! README.

mod balkan_horo;
mod overworld_ambient;
mod teaser;

pub use crate::band::{Band, Style};
use crate::score::Score;

/// The purpose a track is played for: wandering the overworld, a fight;
/// or none, the track as it is, unbounded — what it is, played at
/// the listener's risk, with nothing promised of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Setting {
    Ambient,
    Combat,
    None,
}

impl Setting {
    pub const ALL: [Setting; 3] = [Setting::Ambient, Setting::Combat, Setting::None];

    pub fn name(self) -> &'static str {
        match self {
            Setting::Ambient => "ambient",
            Setting::Combat => "combat",
            Setting::None => "none",
        }
    }

    /// The loudness every track plays at in it, LUFS, so a fight is as
    /// loud whatever style plays it: combat 4 LU over wandering, as games
    /// mix them in play (`proofs/research/settings-findings.md`, §3). In
    /// none, the track's own.
    pub fn lufs(self) -> Option<f32> {
        match self {
            Setting::Ambient => Some(-22.0),
            Setting::Combat => Some(-18.0),
            Setting::None => None,
        }
    }

    /// The most its loudness may range, LU: a fight holds a plateau, and
    /// wandering breathes a little more. In none, the track's own.
    pub fn range(self) -> Option<f32> {
        match self {
            Setting::Ambient => Some(6.0),
            Setting::Combat => Some(4.0),
            Setting::None => None,
        }
    }
}

/// One play: `band` playing a track in `setting`, the play's own
/// `seed`. A band outside the track's style, or a setting the track does
/// not support, plays, and is undefined.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub seed: u64,
    pub band: &'static Band,
    pub setting: Setting,
}

impl Params {
    /// A play of `track` by a band of its own style whose seed is the
    /// play's, in the first setting the track supports.
    pub fn of(track: &Track, seed: u64) -> Params {
        Params { seed, band: Band::first(track.style), setting: track.settings.first().copied().unwrap_or(Setting::None) }
    }
}

pub struct Track {
    pub name: &'static str,
    pub style: Style,
    /// The settings it is made to be played in and checked in; it plays
    /// in `Setting::None` besides, as it is.
    pub settings: &'static [Setting],
    /// What the track must read as. The comparison critic judges against it.
    pub brief: &'static str,
    /// Integrated loudness a take in none is set to, LUFS; a setting
    /// sets its own. A bed sits low so the game's sounds ride over it; a
    /// cue sits where a trailer mixes it.
    pub lufs: f32,
    /// How far the loudness may range through the track, LU, between the
    /// 10th and 95th percentiles of its short-term loudness; a setting may
    /// hold it tighter. A bed holds still and tells its story in what
    /// plays; a one-shot's arc is its sections' declared levels, and its
    /// range is that arc's span.
    pub range: f32,
    /// The most of their figures two plays may share: an improvisation's
    /// plays are each their own, a script's follow one story cut to the
    /// same seconds and share most of it.
    pub kin: f32,
    pub build: fn(&Params) -> Score,
}

impl Track {
    /// Whether it plays in `setting`: one it is made for, or none.
    pub fn plays_in(&self, setting: Setting) -> bool {
        setting == Setting::None || self.settings.contains(&setting)
    }

    /// The loudness a take in `setting` is set to, LUFS.
    pub fn lufs_in(&self, setting: Setting) -> f32 {
        setting.lufs().unwrap_or(self.lufs)
    }

    /// The most a take's loudness in `setting` may range, LU.
    pub fn range_in(&self, setting: Setting) -> f32 {
        setting.range().map_or(self.range, |r| r.min(self.range))
    }
}

/// The most two plays of an improvised track share: past it, plays are
/// one arrangement re-rolled.
const IMPROVISED: f32 = 0.7;
/// The most two plays of a script share: every play cuts to the same
/// picture.
const SCRIPTED: f32 = 0.85;

pub const TRACKS: &[Track] = &[
    Track {
    name: "overworld-ambient",
    style: Style::Bulgarian,
    settings: &[Setting::Ambient],
    brief: "A piece for a Balkan medieval fantasy overworld under Ottoman \
            rule, for wandering, set on one of the Balkan dances — lesnoto, \
            râčenica, pajduško, dajčovo, kopanica — with the frame drum and \
            the plucks playing that dance, the plucks' chords and the drum's \
            fingers on the eighths off the beat the lead moves on, bar by bar, \
            the chords a third under the lead's tones, and where the lead holds, \
            on its figure's eighths, answering — from its start, or \
            the drum stepping in a quarter of the way through, or the plucks' \
            figure alone with no drum, or no pulse at all, the pad and the lead \
            carrying the time, as field music mostly has none; its tempo \
            holding, or pressing on from a little past halfway. The piece is about \
            one motif, the lead's: a bar of the dance's rhythm with a turn of its \
            own, opening on a tone of the tonic's chord and ending off home, said \
            plainly twice at the start and until it is known, and after that as \
            often a little changed as not — a tone passed through, a neighbour \
            taken, a group's long and short the other way about, or said broad, a \
            tone a beat — its beat tones and its opening the same every time. \
            From there the lead \
            plays its distance from it, a half-phrase at a time, improvised: it \
            states it, a bar of rest after; says it a third higher, detached, the \
            strings on it; answers it in its rhythm with other tones; leaves it \
            for another rhythm in another place; begins it and breaks off, its \
            last tone held and a bar of silence after; or runs back to it, ending \
            a step under where it begins. The longer it has been away the likelier \
            it comes back, and it ends on the motif, come home. Every tone not the \
            motif's is by step mostly, every leap answered, falling to the \
            finalis; four bars are a phrase, open and closed by turns, so eight \
            are a question and its answer. The harmony is the lead's too, a \
            four-bar schema of the mode to each phrase — a shuttle, a walk down or \
            a turn through the relative major to open, a cadence to close — the \
            play settling on a few rows it keeps coming home to, all of it held \
            over a tonic drone. The band follows the lead and is never ahead of \
            it: a few players under the opening statements, more with each; \
            thinner where the lead answers and thinnest where it leaves; coming in \
            a layer a bar behind its run back, the drum rolling and the bass \
            stepping up into the bar the motif lands on, where the drum comes \
            down harder and the band is fuller than at the landing before; out, \
            to the drone and the breath, in the silence after the lead breaks \
            off; all of it where the motif is said higher; and the loudness goes \
            with it, a few LU from its barest to its fullest. The layers join in \
            one of six orders the seed chooses — the strings first, or the drum \
            early, or the plucks, the weave of the tune and the colours last or \
            nearly. It opens from the \
            drone: the bed fading in, a prelude of the lead alone at the \
            dance's tempo, the dance or the \
            plucks' figure first, or the lead's short call down from the fifth \
            to the tonic; and it ends with the bed thinning to the drone \
            ringing on, the lead alone over it holding home, the pad and the \
            choir holding the chord, the frame drum's one low stroke and the \
            plucks' tonic, or cut on its last bar. One player tells the story, the lead — a \
            flute, a pan flute, a fiddle, a clarinet or an English horn, each \
            file its own: it sings the theme, breathing at each cadence; plays \
            it detached as a riff where the story asks, the strings doubling \
            it in unison and a second player holding a third or a sixth under \
            it; or holds one chord tone every other bar between the tellings. \
            It sits a little forward of the band, never over it. A weave of \
            the theme runs under the \
            bed where the story thickens — a dulcimer a bar behind, then a \
            harp a foot late and a third above — in the air before the riff \
            takes the tune and behind it after; strings in slow chords; the \
            seed's colours — a low choir, a horn, a shimmer, whichever it \
            brings — joining and leaving together as one soft layer; and \
            under it all, the whole piece through, a breathy choir holding the \
            root and fifth of the chord that stands, far back in the room. \
            Every other voice sits in the bed. Consonant, no drum \
            kit, no sung words.",
    lufs: -22.0,
    range: 5.0,
    kin: IMPROVISED,
    build: overworld_ambient::build,
    },
    Track {
    name: "balkan-horo",
    style: Style::Bulgarian,
    settings: &[Setting::Combat],
    brief: "A horo, a Bulgarian circle dance, as a village wedding band \
            plays it, in the same Balkan land under Ottoman rule: a \
            râčenica, a pajduško, a kopanica or a dajčovo at a hundred and \
            fifty to a hundred and eighty beats, with bite. The bass and the \
            tambura's strum drive every eighth; the tapan's beater falls on \
            the bar and the dance's long group — the limp — with its deep \
            head under it and its thin stick on the other groups, a fill \
            into every part; trombone and tuba stab the strokes and push on \
            the off-beats; the accordion and a held clarinet hold the chords \
            where the dance thickens, a saxophone doubles the riff, and a \
            guitar echoes the tune a bar behind. No drone: the harmony moves, the \
            minor to its flat sixth or Hijaz to its flat second. Its tune is three \
            or four sections, its kolena, each its own theme on its own question and \
            answer and each played twice running, as a horo plays its figures, the \
            round coming again from the second, the bass and the tambura stepping \
            into each new section and the tapan's fill running longer; where the \
            seed gives one, after the first round the lead takes a turn over a \
            vamp of two chords as a wedding soloist does, sixteenths turning about \
            a tone, phrases of two or four bars each closed on a held tone, faster \
            as it goes. One player tells the \
            tune, the lead — a zurna, a fiddle, a clarinet or a trumpet, each file \
            its own — mostly as a riff, breathing at every half-phrase, calling and \
            answering itself, singing or holding long tones at the crest, a horn \
            holding under its riff. One of four stories the seed chooses — a \
            gathering that builds to all of it; in full swing from the first bar; \
            the drum first and the band around it; a breather down to the engine \
            alone and up again — each told by layers joining and leaving a \
            half-phrase at a time. It opens as most dances do, straight in, or after \
            a taksim — the lead alone in free time over the accordion's held chord, \
            falling to home — or on the tapan alone for a bar. Its tempo moves a \
            phrase pair at a time: pressing on through the last quarter, a step at \
            every new section, building from the first pair to the last, or up past \
            the middle and back. It ends as most dances end, the band stopping on \
            the cadence of a section the listener knows — the first come back, or \
            the last played twice — with one stroke on the tonic together; or plays \
            the last four bars three times before that stroke, or holds the last \
            tone, or, seldom, runs down together in unison to the tonic's neighbour \
            and hits the tonic. Driving, joyful, never chaotic; no sung words.",
    // A decibel over the beds, not two: the dance's low end — the tapan,
    // the bass on every eighth — reads quiet to a loudness meter for how
    // hard it peaks.
    lufs: -21.0,
    // A LU over the beds': the bare drum of a story that starts on it, and
    // the hit's ring at the end.
    range: 8.0,
    kin: IMPROVISED,
    build: balkan_horo::build,
    },
    Track {
    name: "teaser",
    style: Style::Bulgarian,
    settings: &[],
    brief: "A two-minute cue for the game's teaser trailer, cut to \
            picture, in the overworld's voice: the lesnoto, a tonic drone, \
            the Aeolian's modal schemata, one storyteller — a flute, a pan \
            flute, a fiddle, a clarinet or an English horn, each file its \
            own — a breathy choir, a stone hall. It has a head and an end, \
            and a loudness and density arc across them. Dawn, \
            near silence to the drone, the breath and then the lone voice \
            singing the theme; vistas, the strings, the echo and the bass \
            joining a layer a part under the tune, the storyteller \
            breathing while the echo carries it and coming back a step \
            higher over the whole bed; day into night, the tune a step \
            lower on the lament as a soft pulse enters and the bed thins \
            under its answer, then a whole phrase of night: one string, \
            the drum's heartbeat, a high dulcimer for the moon, the \
            storyteller holding long tones, quiet; work, on the dance, the \
            frame drum, the plucks, a taiko striking each bar and the lead \
            playing the tune detached with the strings on it, all on the \
            cut, a second joining under it, a crescendo, and then a drop \
            to the drum's fill alone; the swell, all of it, the tune sung \
            a third up with the horns holding its skeleton under it, the \
            timpani joining for its second half, the loudest part; the bed \
            dropping out for the last bar under the tune, the choir and a \
            growing timpani roll as the harmony rises to the flat seventh; \
            and the title, \
            the tonic struck by everyone and let ring out, falling away. \
            Each movement starts on a cadence within a couple of seconds \
            of its cut — 0:00, 0:15, 0:40, 1:00, 1:35, 1:55 — its level \
            declared against the swell. Consonant, no drum kit but for the \
            one crash, no sung words.",
    // A cue under a trailer's picture and effects, not a bed under play;
    // its swell's strokes peak some fifteen dB over its loudness.
    lufs: -18.0,
    // Dawn to the swell: the arc a one-shot is for.
    range: 19.0,
    kin: SCRIPTED,
    build: teaser::build,
    },
];

pub fn find(name: &str) -> Option<&'static Track> {
    TRACKS.iter().find(|p| p.name == name)
}

/// The tracks of `style`.
pub fn of_style(style: Style) -> impl Iterator<Item = &'static Track> {
    TRACKS.iter().filter(move |t| t.style == style)
}
