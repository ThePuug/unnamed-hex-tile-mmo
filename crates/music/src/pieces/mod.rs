//! Every track, by name, and the styles and settings they are played in.
//! A track is an improvisational interpretation of a theme, a story or a
//! script — speed metal as Helloween played it, the overworld told on a
//! dance, the teaser's cue cut to picture: every play of it is new, and
//! every play is the track. A track is in one style and declares the
//! settings it is made to be played in; a setting is a purpose the music
//! serves, which bounds the track's choices, and a track is made to
//! support one by being taught it. Every track also plays in `None`, as
//! it is: no constraint, and no expectation either. A play is a band playing a track in a
//! setting (`Params`). Adding a track: a module with
//! `pub fn build(&Params) -> Score`, an entry in `TRACKS`, a row in the
//! README.

mod minor_blues;
mod balkan_horo;
mod metal_ballad;
mod overworld_ambient;
mod speed_metal;
mod teaser;

pub use crate::band::{Band, Style};
use crate::score::Score;

/// The purpose a track is played for: wandering the overworld, a fight,
/// a town; or none, the track as it is, unbounded — what it is, played at
/// the listener's risk, with nothing promised of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Setting {
    Ambient,
    Combat,
    City,
    None,
}

impl Setting {
    pub const ALL: [Setting; 4] = [Setting::Ambient, Setting::Combat, Setting::City, Setting::None];

    pub fn name(self) -> &'static str {
        match self {
            Setting::Ambient => "ambient",
            Setting::Combat => "combat",
            Setting::City => "city",
            Setting::None => "none",
        }
    }

    /// The loudness every track plays at in it, LUFS, so a fight is as
    /// loud whatever style plays it: combat 4 LU over wandering, as games
    /// mix them in play, a town a little over (`proofs/research/
    /// settings-findings.md`, §3). In none, the track's own.
    pub fn lufs(self) -> Option<f32> {
        match self {
            Setting::Ambient => Some(-22.0),
            Setting::Combat => Some(-18.0),
            Setting::City => Some(-21.0),
            Setting::None => None,
        }
    }

    /// The most its loudness may range, LU: a fight holds a plateau, and
    /// wandering and a town breathe a little more. In none, the track's
    /// own.
    pub fn range(self) -> Option<f32> {
        match self {
            Setting::Ambient | Setting::City => Some(6.0),
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
            the plucks playing that dance's own pattern — from its start, or \
            the drum stepping in a quarter of the way through, or the plucks' \
            figure alone with no drum, or no pulse at all, the pad and the lead \
            carrying the time, as field music mostly has none; its tempo \
            holding, or pressing on from a little past halfway. One theme runs \
            the piece through, or two or three in series as a dance plays its \
            tunes: a shape on the strong beats that comes down \
            to the finalis — an arch, a descent, a circling, a leap and its \
            recovery, a wave — filled by steps, said in four-bar phrases as a \
            period or a sentence, open and closed by turns, so eight bars are \
            a question and its answer; it comes back a step up every other \
            pair and a third up where the bed is fullest. The harmony is a \
            four-bar schema of the mode — a shuttle or a walk down to open, a \
            cadence to close — held over a tonic drone. One of six stories the \
            seed chooses — an arc that climbs to one defiant riff and comes \
            down; two waves with a hollow between; a slow burn to a long \
            plateau with no riff; a lament sung through with one swell; a \
            dance that steps in on the plucks and the drum and takes a \
            breath; fragments, the tune sung and played by turns — \
            each told by the bed filling and thinning one layer at a time, a \
            half-phrase a step, never two layers at once, coming back to the \
            texture it opened on, at a loudness that barely moves. Once, \
            somewhere past its first pair, it does one thing of its own: the \
            bed hollows to the drone and the breath for a phrase; the second \
            sings the tune for a pair; a pair lifts onto the relative major's \
            chords or steps onto the fifth; a phrase is sung in free time with \
            the dance stopped; or a bar of the drone alone. It opens from the \
            drone: the bed fading in, a prelude in free time, the dance or the \
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
    name: "minor-blues",
    style: Style::Blues,
    settings: &[Setting::City, Setting::Ambient, Setting::Combat],
    brief: "A minor blues as a small band plays it: the twelve-bar in Dorian or Aeolian on sevenths, in a \
            slow twelve-eight, a rumba, a walking four or a six-eight, the band \
            playing that feel; every chorus the song's twelve-bar, the same each \
            time round, its turn home through v and iv, ♭VI7 to the major V7, \
            iiø to V7 or the like, its last bar turning into the next chorus. \
            The head is the first chorus and the last, and every chorus between \
            is the lead's solo, a blues player's: phrases of six or seven beats \
            with rests between, most entering off the beat or pushed ahead of \
            it, restating an earlier one now and then, denser as it goes. \
            One theme runs the head on the minor pentatonic — the \
            fall from the fifth to the flat seventh under home, the riff round \
            the tonic — sung AAB, a row's line, the line again over the IV and \
            an answer, each in a row's first two bars, the band answering in \
            its last two, or where no band answers, the lead answering \
            itself — a third up where the band is fullest. One of five stories the seed \
            chooses — a stroll that fills to a riff and empties; a late night that \
            thins to the organ; a rush hour that never quite stops; a corner where \
            the lead calls and answers itself; after hours, slow, the lead \
            singing over the organ — each told by the band filling and thinning \
            one player at a time, a half-phrase a step, coming back to where it \
            opened, at a loudness that barely moves; once, two thirds through, \
            it may play a chorus in stop-time, drop to the bass and the kit for \
            one, or vamp on the tonic at a lifted tempo before the end. It \
            opens as blues bands open — a vamp on the tonic the band layers \
            into, the lead soloing a chorus before the head, four bars from the \
            V, the lead alone in free time, or the head at once — and ends as \
            one ends: a held last chord, slowing into it about half the time, \
            the brushes swelling on the ride; a vamp falling away; the band \
            stopping for the lead's break and a stab; or the last two bars three \
            times — on the tonic's seventh or ninth, ♭VI, the major tonic or ♭II. \
            One player tells the story, the lead — a harmonica, a tenor or alto \
            sax or a muted trumpet, each file its own: it sings the theme with a \
            scoop into its notes, plays it detached as a riff, calls with each \
            phrase's first half and answers itself with the riff in its second, \
            holding each row's last tone, a second horn holding a \
            third or a sixth under its riff where the seed brings one; it sits \
            a little forward of the \
            band, never over it. The electric piano answers the lead's call in \
            the row's last bars, an octave under it — in every hole, about half, \
            or never, holding the chord instead, as the song's band answers; the \
            bass in two or walking, with a pickup into every bar, the piano \
            comping a bar's rhythm at a time and the guitar on the beats, the \
            kit on brushes, filling where a blues fills, every part varying \
            by the phrase, the organ as the wash, and the seed's colours — a second horn \
            on the third, a shimmer — joining and leaving as one layer. Every \
            other player sits in the band. No drone; no sung words.",
    lufs: -22.0,
    range: 5.0,
    kin: IMPROVISED,
    build: minor_blues::build,
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
    Track {
    name: "metal-ballad",
    style: Style::Metal,
    settings: &[Setting::City],
    brief: "A metal power ballad in the minor, slow — a four at \
            sixty to seventy-five or a twelve-eight — that opens on a clean \
            guitar's picked arpeggio, or a piano's, its top ringing on one \
            tone of the key as the chords move under it, and builds on it a \
            layer at a time: the bass locked to the kick, the kit's side stick \
            and hats and then its ride and snare, the distorted guitars \
            double-tracked one each side, palm-muted in a verse with the next \
            chord pushed open on the bar's last eighth, ringing open in a \
            chorus, where the arpeggio rests — the two parting the song's one \
            way: in unison, the right sitting out each verse's first half, the \
            right holding the chord where the left plays, on the chord's \
            inversion, or an octave up — the slow strings and a choir. What plays makes \
            the part of the song: a verse holds the tonic and swings slowly to \
            the sixth or the seventh, or falls through the sixth to the major \
            V; a chorus moves a chord a bar, opens on the sixth as often as \
            the tonic, walks down to the major V and comes home by the sixth \
            and seventh or through iv and the major V, its leading tone the \
            harmonic minor's, the tune a third higher; the solo's climax has a \
            progression heard nowhere else. One theme runs the whole piece \
            through, sung by one lead where a voice would — an overdriven or a \
            distortion guitar, or a violin, each file its own — in four-bar \
            phrases that ask and answer, stated over the arpeggio it opens on, \
            played as a riff over the muted guitars, held in long tones in the \
            part before a chorus, the breath a pre-chorus takes, licking in some \
            of its gaps and pushing some tones ahead of the beat; in some songs \
            its last chorus reaches one tone past every chorus before. The song is \
            dense, not long: a layer joins or leaves at every half-phrase. \
            No part plays \
            one bar over and over: each phrase is a bar, its variant — the hat \
            opened, the kick moved, the arpeggio turned over, the bass's \
            octave — and a cadence where the kit fills, the bass walks into \
            the next chord and the arpeggio holds or hammers in, and a bigger \
            fill, the seed's own, rolls up to the top of the kit into every \
            new part of the song. Its crest is a solo of six to twelve bars, the \
            seed's, the arpeggio resting to make room for it: a guitar's \
            balladeer's turn of held and bent tones and runs, peaking in its \
            middle and fastest after, a second guitar over a few bars of it in \
            one solo in four; or a guitar, or the \
            bass stepping forward over the kit and the strings and choir with \
            the guitars resting, that sings the theme, answers itself a third \
            higher with rests between, climbs in sixteenth-note sequences to a \
            peak it has not touched before, the band stopping for the beat \
            before it while the soloist breaks alone, held with a bend or \
            reached by a \
            run, and lands on the theme with a guitar a third over it, the \
            harmony held back until then; or the kit, loud from its first bar and louder every part, \
            the song's lead, \
            over the band's short stop-time stab on every bar, varied by the \
            phrase, the bass's pedal and \
            the strings holding the chord's root and fifth, its motif on every beat, \
            then displaced over doubled kicks, then rolling in triplets, until \
            the fill that opens the choruses brings the band back. One of four \
            stories — a power ballad with a chorus before the solo and a verse \
            between, a slow burn from the arpeggio climbing once, an anthem \
            with the band in from the start, a requiem opened on the strings — \
            each the band joining and leaving one layer a part, but where it drops \
            at once to a quiet verse or comes back in at once. It opens on the \
            arpeggio alone, the lead stating the theme over it. What follows the \
            solo is the seed's, as often as power ballads do it: the final chorus, \
            longer than the first as often as not; a verse dropped to the intro's \
            players and the lead, quiet, before the final chorus; the guitar's solo, long, \
            playing the song out; or a final chorus between two guitar solos. So is \
            the ending: the band leaving a layer a part to its last few, who strike \
            the tonic and let it ring, where a record would fade; one guitar alone \
            walking up into the tonic's chord, rolled and let ring; the intro come \
            back alone, four to eight bars, slowing into its tonic; the final chorus \
            now and then a step up and its last line sung again once or twice, the last time \
            slowing into the band's tonic, held; or, seldom, one hit on the tonic \
            held under a cymbal's swell and cut with a last crash. The chorus \
            heavier than the verse, the band growing as each layer joins, \
            not afraid of its loud parts; no sung words.",
    lufs: -20.0,
    // A ballad's chorus is heavier than its arpeggio, and its loudness
    // says so: the pedal gives back only part of what the band adds.
    range: 8.0,
    kin: IMPROVISED,
    build: metal_ballad::build,
    },
    Track {
    name: "speed-metal",
    style: Style::Metal,
    settings: &[Setting::Combat],
    brief: "Speed metal as Helloween played it on Walls of Jericho and the \
            Keepers, 1985 to 1988, at a hundred and forty-four to a \
            hundred and sixty-four beats in the minor, every seed its own \
            song of one album: its groove's family drawn first — the skank \
            with the kick doubled or single, the gallop, the backbeat or a \
            half-speed feel — then its intro, its chorus's mood, its \
            form's lengths, how its two guitars split, which soloist goes \
            first, how it ends, and one signature event no other seed \
            shares: the guitars out over bass and drums before the last \
            chorus, a dead bar, the second verse in another feel, a \
            half-time chorus, the drums alone into the solos, or the last \
            chorus up a step. Verse, most often a pre-chorus, and chorus \
            twice, a solo block, and the way back to a last chorus, \
            doubled three times in four, every section whole phrases of \
            four bars. The pre-chorus doubles the kick, the chorus moves \
            the cymbal to the ride, the solos do both; fills a beat or two \
            every few bars and into every section, a crash after. The \
            rhythm guitars are two players, two amps, one hard each side: \
            most bars one part, palm-muted in the verse and open in the \
            chorus, but how much they split is the song's — the pre-chorus \
            most — the riff harmonised in thirds, one holding chords while \
            the other chugs, a pedal over the riff, another voicing or an \
            octave up; their riff answers itself in its second bar, turns \
            round in its fourth and harder in its eighth, and a later \
            verse answers its own way; the bass on the root in unison. One \
            theme runs the song through, sung by one lead guitar that \
            dances: every foot of it in the verse, legato, half its tones \
            off the beat; held tones climbing the pre-chorus to its peak; \
            a third higher in the chorus, held bars and moving bars by \
            turns, a twin guitar a diatonic third over it, plain; a \
            half-phrase restated an octave up now and then, a tone pushed \
            ahead of the beat when it sings a phrase again, a lick in a \
            few of its gaps from the second verse on, and at the end of \
            the last chorus a climb to the song's highest tone, held over \
            the ending; bent into, slid into, shaken with vibrato, its \
            phrase ends let go a different way each time. The chorus's \
            hook is the twin melody the song opens on — the twins alone, \
            the band's hits, the drums alone, held chords, the riff, or \
            one guitar alone — and comes back after the first chorus where \
            the seed brings it. The solo block is two players, a shredder \
            of long runs, leaps, repeated cells and odd groupings and a \
            singer of short bent phrases round a pedal, in one of the \
            surveyed layouts — two turns and a twin break of the theme in \
            thirds before, between or after them, or the turns trading \
            eight bars and then four — each turn opening on a bend, a \
            flurry, a held tone or a short lick, its density arching, \
            rising, falling or flat, its register climbing to a peak, \
            closing held or running on, every turn answering the one \
            before by contrast; the rhythm guitars on a palm-muted pedal \
            under it. It ends as Helloween end, on the chorus's home \
            chord: the ritual — the chord held a bar to four under a \
            cymbal's wash and the toms, sometimes slowing, a separate last \
            hit, now and then on the and of four — a figure of the band's \
            hits and one stroke, the riff into one stab, a false ending, \
            the lead alone into the stab, or, seldom, a chord left to \
            ring. A record's stereo: the rhythm pair wide, the kit spread \
            across the stage, the lead left of the middle and the twin \
            right. Loud, driving, alive, bright even in the minor; no sung \
            words.",
    lufs: -20.0,
    range: 7.0,
    kin: IMPROVISED,
    build: speed_metal::build,
    },
];

pub fn find(name: &str) -> Option<&'static Track> {
    TRACKS.iter().find(|p| p.name == name)
}

/// The tracks of `style`.
pub fn of_style(style: Style) -> impl Iterator<Item = &'static Track> {
    TRACKS.iter().filter(move |t| t.style == style)
}
