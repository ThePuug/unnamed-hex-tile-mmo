//! Every piece, by name. Adding one: a module with
//! `pub fn build(&Params) -> Score`, an entry in `PIECES`, a row in the
//! README.

mod city_ambient;
mod balkan_horo;
mod metal_ballad;
mod overworld_ambient;
mod teaser;

use crate::score::Score;

pub struct Params {
    pub seed: u64,
}

pub struct Piece {
    pub name: &'static str,
    /// What the piece must read as. The comparison critic judges against it.
    pub brief: &'static str,
    /// The pool the client draws it from at intervals, tagged on the file.
    pub pool: &'static str,
    /// Files in the asset, one seed each — the first seeds from 0 whose
    /// stories differ, so a pool needs several and no two of them tell
    /// one story; a one-shot's differ by their leads.
    pub variants: u64,
    /// Integrated loudness the file is set to, LUFS. A bed sits low so the
    /// game's sounds ride over it; a cue sits where a trailer mixes it.
    pub lufs: f32,
    /// How far the loudness may range through the piece, LU, between the
    /// 10th and 95th percentiles of its short-term loudness. A bed holds
    /// still and tells its story in what plays; a one-shot's arc is its
    /// sections' declared levels, and its range is that arc's span.
    pub range: f32,
    /// Whether every seed tells the same story — a cue's movements, cut
    /// to its picture — so its files differ by their leads alone.
    pub one_story: bool,
    pub build: fn(&Params) -> Score,
}

pub const PIECES: &[Piece] = &[
    Piece {
    name: "overworld-ambient",
    brief: "A piece for a Balkan medieval fantasy overworld under Ottoman \
            rule, for wandering, set on one of the Balkan dances — lesnoto, \
            râčenica, pajduško, dajčovo, kopanica — with the frame drum and \
            the plucks playing that dance's own pattern. One theme runs the \
            whole piece through: a shape on the strong beats that comes down \
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
            texture it opened on, at a loudness that barely moves. It opens \
            short, so the dance comes soon: the drone alone and the lead's \
            call down from the fifth to the tonic; and it ends on the frame \
            drum's one low stroke and the plucks' tonic, the lead holding \
            home over the drone and the drone ringing on alone. One player tells the story, the lead — a \
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
    pool: "overworld",
    variants: 3,
    lufs: -22.0,
    range: 5.0,
    one_story: false,
    build: overworld_ambient::build,
    },
    Piece {
    name: "city-ambient",
    brief: "A piece for a bustling city at night, for wandering its streets, on a \
            minor blues: the twelve-bar in Dorian or Aeolian on sevenths, in the \
            shuffle or a walking four, the band playing that feel; every chorus \
            the twelve-bar, its rows drawn afresh between the head and the last. \
            One theme runs the whole piece through on the minor pentatonic — the \
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
            opened, at a loudness that barely moves. It opens as a blues band \
            opens, four bars from the V — the dominant seventh, the iv, the i \
            and the dominant again into the head — and ends as one ends, \
            slowing through the last chorus's last two bars into one held \
            minor ninth on the tonic, the brushes swelling on the ride. \
            One player tells the story, the lead — a harmonica, a tenor or alto \
            sax or a muted trumpet, each file its own: it sings the theme with a \
            scoop into its notes, plays it detached as a riff, calls with each \
            phrase's first half and answers itself with the riff in its second, \
            holding each row's last tone, a second horn holding a \
            third or a sixth under its riff where the seed brings one; it sits \
            a little forward of the \
            band, never over it. The electric piano answers the lead's call in \
            the row's last bars, an octave under it; the \
            bass in two or walking, with a pickup into every bar, the piano \
            comping a bar's rhythm at a time and the guitar on the beats, the \
            kit on brushes, filling where a blues fills, every part varying \
            by the phrase, the organ as the wash, and the seed's colours — a second horn \
            on the third, a shimmer — joining and leaving as one layer. Every \
            other player sits in the band. No drone; no sung words.",
    pool: "city",
    variants: 3,
    lufs: -22.0,
    range: 5.0,
    one_story: false,
    build: city_ambient::build,
    },
    Piece {
    name: "balkan-horo",
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
            guitar echoes the tune a bar behind. No drone: the harmony \
            moves, the minor to its flat sixth or Hijaz to its flat second. \
            One player tells the tune, the lead — a zurna, a fiddle, a \
            clarinet or a trumpet, each file its own — mostly as a riff, \
            breathing at every half-phrase, calling and answering itself, \
            singing or holding long tones at the crest, a horn holding under \
            its riff. One of four stories the seed chooses — a gathering \
            that builds to all of it; in full swing from the first bar; the \
            drum first and the band around it; a breather down to the engine \
            alone and up again — each told by layers joining and leaving a \
            half-phrase at a time. It opens on the tapan alone for a bar, \
            presses on a phrase at a time through its last quarter as a band \
            pushes its dancers, and ends at full tilt: the band in one \
            unison run down to the tonic's neighbour and one hit on the \
            tonic together. Driving, joyful, never chaotic; no sung words.",
    pool: "horo",
    variants: 3,
    // A decibel over the beds, not two: the dance's low end — the tapan,
    // the bass on every eighth — reads quiet to a loudness meter for how
    // hard it peaks.
    lufs: -21.0,
    // A LU over the beds': the bare drum of a story that starts on it, and
    // the hit's ring at the end.
    range: 8.0,
    one_story: false,
    build: balkan_horo::build,
    },
    Piece {
    name: "teaser",
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
    pool: "teaser",
    variants: 3,
    // A cue under a trailer's picture and effects, not a bed under play;
    // its swell's strokes peak some fifteen dB over its loudness.
    lufs: -18.0,
    // Dawn to the swell: the arc a one-shot is for.
    range: 19.0,
    one_story: true,
    build: teaser::build,
    },
    Piece {
    name: "metal-ballad",
    brief: "A metal power ballad in the minor, slow — a four at \
            sixty to seventy-five or a twelve-eight — that opens on a clean \
            guitar's picked arpeggio, or a piano's, its top ringing on one \
            tone of the key as the chords move under it, and builds on it a \
            layer at a time: the bass locked to the kick, the kit's side stick \
            and hats and then its ride and snare, the distorted guitars \
            double-tracked one each side, palm-muted in a verse with the next \
            chord pushed open on the bar's last eighth — in half the verses \
            the right side playing the accents alone — ringing open in a \
            chorus, where the arpeggio rests and the right side takes the \
            chord's inversion, the slow strings and a choir. What plays makes \
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
            part before a chorus, the breath a pre-chorus takes. The song is \
            dense, not long: a layer joins or leaves at every half-phrase. \
            No part plays \
            one bar over and over: each phrase is a bar, its variant — the hat \
            opened, the kick moved, the arpeggio turned over, the bass's \
            octave — and a cadence where the kit fills, the bass walks into \
            the next chord and the arpeggio holds or hammers in, and a bigger \
            fill, the seed's own, rolls up to the top of the kit into every \
            new part of the song. Its crest is a solo of eight bars, the \
            seed's, the arpeggio resting to make room for it: a guitar, or the \
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
            each the band joining and leaving one layer a part. It opens on \
            the arpeggio alone, four bars, the lead stating the theme over \
            it; after the solo comes the final chorus, and out of it the \
            ending: the band slowing into one hit on the tonic held under a \
            cymbal's swell and cut with a last crash, or the arpeggio alone \
            over the tonic, slowing, its last chord left to ring. The chorus \
            heavier than the verse, the band growing as each layer joins, \
            not afraid of its loud parts; no sung words.",
    pool: "ballad",
    variants: 3,
    lufs: -20.0,
    // A ballad's chorus is heavier than its arpeggio, and its loudness
    // says so: the pedal gives back only part of what the band adds.
    range: 8.0,
    one_story: false,
    build: metal_ballad::build,
    },
];

pub fn find(name: &str) -> Option<&'static Piece> {
    PIECES.iter().find(|p| p.name == name)
}

/// How far past its first candidate a pool looks for a seed that brings
/// more of the facets the piece spreads its files across.
const SEARCH: u64 = 64;

impl Piece {
    /// The seeds of the pool, `variants` of them, each one whose story no
    /// seed already taken tells and whose lead no seed already taken leads with, so
    /// each file has its own storyteller; of those, the first from 0
    /// that brings the most of the score's facets the pool lacks, so a
    /// piece's files spread across what its seeds draw. A piece naming
    /// no facets takes the first that qualifies. A piece that tells one
    /// story, a cue, has files that differ by their leads alone.
    pub fn pool(&self) -> Vec<u64> {
        let mut seeds: Vec<u64> = Vec::new();
        let mut taken: Vec<(&'static str, Option<u8>)> = Vec::new();
        let mut spread: Vec<&'static str> = Vec::new();
        while seeds.len() < self.variants as usize {
            let mut best: Option<(u64, usize, (&'static str, Option<u8>), Vec<&'static str>)> = None;
            let mut first: Option<u64> = None;
            let mut seed = 0;
            while first.is_none_or(|f| seed <= f + SEARCH) {
                if !seeds.contains(&seed) {
                    let score = (self.build)(&Params { seed });
                    let story = score.story;
                    let lead = score.lead.map(|ch| score.instrument(ch).program);
                    if !taken.iter().any(|(s, l)| (!self.one_story && *s == story) || (lead.is_some() && *l == lead)) {
                        first.get_or_insert(seed);
                        let new = score.facets.iter().filter(|f| !spread.contains(f)).count();
                        if best.as_ref().is_none_or(|b| new > b.1) {
                            best = Some((seed, new, (story, lead), score.facets.clone()));
                        }
                        if new == score.facets.len() {
                            break;
                        }
                    }
                }
                seed += 1;
            }
            let (seed, _, kind, facets) = best.expect("a seed the pool can take");
            seeds.push(seed);
            taken.push(kind);
            spread.extend(facets);
        }
        seeds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A piece naming no facets takes the first seeds that qualify, one
    /// story and one lead a file, as the pool always has; one that names
    /// them spreads its files across every value it can.
    #[test]
    fn a_pool_spreads_only_what_a_piece_names() {
        for piece in PIECES {
            let pool = piece.pool();
            let facets: Vec<Vec<&str>> = pool.iter().map(|s| (piece.build)(&Params { seed: *s }).facets).collect();
            if facets.iter().all(|f| f.is_empty()) {
                let mut first = Vec::new();
                let mut taken: Vec<(&str, Option<u8>)> = Vec::new();
                let mut seed = 0;
                while first.len() < pool.len() {
                    let score = (piece.build)(&Params { seed });
                    let story = score.story;
                    let lead = score.lead.map(|ch| score.instrument(ch).program);
                    if !taken.iter().any(|(s, l)| (!piece.one_story && *s == story) || (lead.is_some() && *l == lead)) {
                        taken.push((story, lead));
                        first.push(seed);
                    }
                    seed += 1;
                }
                assert_eq!(pool, first, "{}: the pool moved", piece.name);
            } else {
                let spread: std::collections::HashSet<&str> = facets.iter().flatten().copied().collect();
                assert!(spread.len() > facets[0].len(), "{}: its files share every facet", piece.name);
            }
        }
    }
}
