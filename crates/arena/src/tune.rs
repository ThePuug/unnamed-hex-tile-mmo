//! # Tuning search
//!
//! `cargo run --bin arena -- tune <command>` searches
//! the game's numbers and the NPCs' minds against the balance arena, in
//! process. What it searches, within what bounds, what every scenario holds
//! fixed and how a matrix is scored are `tune.toml` beside this file, read
//! as a command starts. What it finds carries from command to command in
//! `proofs/arena/tune/state.json`, which starts from the game's own numbers
//! and minds; `apply` writes it into `Tuning`'s defaults and `mind::TUNED`.
//!
//! - `screen`: sweeps each knob alone across its bounds and reports how far
//!   it moves each pairing's edge; the balance search leaves out the knobs
//!   that move none past `screen.min_moved`
//! - `balance [evals]`: CMA-ES over the knobs that matter, for the least
//!   imbalance and shortfall of the minds' styles together, keeping
//!   nothing that widens either the split of wins or that shortfall
//! - `minds [evals] [archetype ...]`: CMA-ES over each archetype's mind, for
//!   its own score against the field: one pass
//! - `restart [evals] archetype ...`: a fresh search for each archetype's
//!   mind from anywhere in its bounds, scored as `minds` scores, keeping
//!   what it found where it beats the mind held: for a mind stuck where no
//!   small change pays
//! - `exploit [evals] [archetype ...]`: a fresh search for each archetype's
//!   best answer to the minds as they stand, on winning alone, and how far
//!   it beats the mind it has: reported, never joining the field
//! - `settle [evals]`: passes of `minds`, an `exploit` pass every
//!   `minds.exploit_every`, until no archetype gains more than
//!   `minds.settle` in one, or `minds.rounds` have run
//! - `loop <count> [evals]`: settle the minds, then balance, `count` times
//! - `show [ledger]`: the matrix as the state stands
//! - `apply`: writes the state into the source
//!
//! Each search runs in a space where every setting's bounds are 0 and 1,
//! starts from the state, and keeps what it found only where a second,
//! longer evaluation says it beats the start: a search never makes the
//! state worse. Every candidate of a generation fights the same seeded
//! fights, and the check fights the start and the find on the same seeds,
//! so a comparison weighs the settings rather than the dice
//! (`combat::dice`); each generation draws a seed of its own, so no search
//! fits one set of fights.
//!
//! A mind is searched against a field: the minds as they stand and those of
//! the last `minds.pool` - 1 passes. A
//! mind that answers only the latest of its foes' minds would chase them
//! round in circles, each pass undoing the last; one that answers the field
//! holds up against all of them. A mind's score also holds its archetype
//! to its style: what it loses when the commitment its build invests in is
//! used less than `style.floor`, so a mind keeps its style in play at the
//! cost of some fights, and a style the numbers make too dear shows as
//! lost fights rather than as fighters that stopped playing it. An
//! exploiter, searched afresh from anywhere in the bounds, scores on
//! winning alone, so how far it beats a held mind is the style's price.

use std::{collections::BTreeMap, path::PathBuf};

use cmaes::{CMAESOptions, DVector};
use serde::{Deserialize, Serialize};

use common_bevy::{archetype::EnemyArchetype, components::{ActorAttributes, Attribute}, message::AbilityType, tuning::Tuning};

use super::{matrices, matrix, print_matrix, Ledger, Pairing, Settings};
use combat::behaviour::mind::{Minds, TUNED};

/// The step a search starts with, in the space where each bound is 0 and 1
const SIGMA: f64 = 0.2;

/// The step an exploiter starts with, from anywhere in the bounds
const EXPLORE: f64 = 0.3;

/// Generations between looks at where a search's mean stands
const LOOK_EVERY: usize = 5;

#[derive(Deserialize)]
struct Config {
    runs: u32,
    check_runs: u32,
    population: f64,
    skill: String,
    fixed: BTreeMap<String, f32>,
    score: Score,
    screen: Screen,
    knobs: Vec<Range>,
    minds: MindRanges,
    style: Style,
}

/// What holds a mind to its archetype's style (`style_use`)
#[derive(Deserialize)]
struct Style {
    /// Points a mind's score loses, of a hundred, for each share of its
    /// floor its commitment's use falls short of
    weight: f32,
    /// Each archetype's floor, by name, in its commitment's own measure;
    /// none where it has none
    #[serde(default)]
    floor: BTreeMap<String, f32>,
}

#[derive(Deserialize)]
struct Score {
    band: f32,
    edge: f32,
    short: f32,
}

#[derive(Deserialize)]
struct Screen {
    min_moved: f32,
}

#[derive(Clone, Deserialize)]
struct Range {
    name: String,
    min: f32,
    max: f32,
    start: Option<f32>,
}

impl Range {
    fn to_unit(&self, value: f32) -> f64 {
        (((value - self.min) / (self.max - self.min)) as f64).clamp(0.0, 1.0)
    }

    fn from_unit(&self, unit: f64) -> f32 {
        self.min + unit.clamp(0.0, 1.0) as f32 * (self.max - self.min)
    }
}

#[derive(Deserialize)]
struct Bounds {
    min: f32,
    max: f32,
}

#[derive(Deserialize)]
struct MindRanges {
    settle: f32,
    rounds: usize,
    pool: usize,
    exploit_every: usize,
    just_acted: Bounds,
    common: Vec<Range>,
    strike: Vec<Range>,
    reaction: Vec<Range>,
    own: BTreeMap<String, Vec<Range>>,
}

/// The numbers and minds found so far, by the names the arena sets them by,
/// and the field's minds of passes past, oldest first
#[derive(Default, Deserialize, Serialize)]
struct State {
    knobs: BTreeMap<String, f32>,
    minds: BTreeMap<String, f32>,
    #[serde(default)]
    history: Vec<BTreeMap<String, f32>>,
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn results() -> PathBuf {
    manifest().join("../../proofs/arena/tune")
}

fn config() -> Config {
    let path = manifest().join("src/tune.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    toml::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn load() -> State {
    let path = results().join("state.json");
    let mut state: State = std::fs::read_to_string(&path).ok()
        .map(|text| serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display())))
        .unwrap_or_default();
    // Every mind setting the game holds, under what was found
    for &(key, value) in TUNED {
        state.minds.entry(key.to_owned()).or_insert(value);
    }
    state
}

fn save(state: &State) {
    std::fs::create_dir_all(results()).expect("tune: results directory");
    let text = serde_json::to_string_pretty(state).expect("tune: state");
    std::fs::write(results().join("state.json"), text).expect("tune: state.json");
}

/// The game's value of `name` where the state holds none
fn knob(state: &State, name: &str) -> f32 {
    state.knobs.get(name).copied().unwrap_or_else(|| Tuning::default().get(name).unwrap_or_else(|error| panic!("tune.toml: {error}")))
}

/// A scenario of `runs` fights a pairing under the state, with `knobs` and
/// `minds` laid over it, and `focus`'s pairings alone where given, its
/// fights rolled from `seed`
fn settings(config: &Config, state: &State, knobs: &BTreeMap<String, f32>, minds: &BTreeMap<String, f32>, focus: Option<EnemyArchetype>, runs: u32, seed: u64) -> Settings {
    let mut settings = Settings::parse(&[format!("runs={runs}"), format!("skill={}", config.skill)]);
    settings.focus = focus;
    settings.seed = seed;
    let mut tuning = Tuning::default();
    for (name, value) in state.knobs.iter().chain(knobs) {
        tuning.set(name, &value.to_string()).unwrap_or_else(|error| panic!("tune: {error}"));
    }
    for (name, value) in &config.fixed {
        tuning.set(name, &value.to_string()).unwrap_or_else(|error| panic!("tune.toml: {error}"));
    }
    settings.tuning = tuning;
    let mut held = Minds::tuned();
    for (key, value) in state.minds.iter().chain(minds) {
        held.set(key, &value.to_string()).unwrap_or_else(|error| panic!("tune: {error}"));
    }
    settings.minds = held;
    settings
}

/// How far `rows` lie from fair, what the balance search lessens: each
/// pairing's edge, and each of its fights run to the cap as a hundred
/// points off fair, the furthest a fight lies, both by `edge` a point; and
/// its median fight short of `short` seconds. The mean over pairings.
fn imbalance(score: &Score, rows: &[Pairing]) -> f32 {
    let each = rows.iter().map(|row| {
        score.edge * (row.edge.abs() + row.capped)
            + (score.short - row.median).max(0.0)
    });
    each.sum::<f32>() / rows.len().max(1) as f32
}

/// How far each pairing's split of wins lies past `band` from even, a
/// fight both sides die in a win for neither; the mean over pairings. A
/// balance search keeps nothing that widens it.
fn split(score: &Score, rows: &[Pairing]) -> f32 {
    let each = rows.iter().map(|row| ((row.a_wins - row.b_wins).abs() / 2.0 - score.band).max(0.0));
    each.sum::<f32>() / rows.len().max(1) as f32
}

/// How well `archetype` does in `rows`, what its mind search raises: its
/// edge, a fight both sides die in nothing and each fight run to the cap a
/// loss at full health, a hundred points; the mean over its pairings, less
/// its `shortfall` where held to `style`.
fn standing(rows: &[Pairing], archetype: EnemyArchetype, style: Option<&Style>) -> f32 {
    let mine: Vec<f32> = rows.iter().filter(|row| row.a == archetype || row.b == archetype).map(|row| {
        let edge = if row.a == archetype { row.edge } else { -row.edge };
        edge - row.capped
    }).collect();
    mine.iter().sum::<f32>() / mine.len().max(1) as f32 - style.map_or(0.0, |style| shortfall(style, rows, archetype))
}

/// How far `archetype` falls short of its style in `rows`: `weight` of a
/// hundred points for each share of its floor its commitment's use falls
/// short of, nothing above it or with no floor
fn shortfall(style: &Style, rows: &[Pairing], archetype: EnemyArchetype) -> f32 {
    style.floor.get(&name(archetype)).filter(|&&floor| floor > 0.0)
        .map_or(0.0, |&floor| style.weight * 100.0 * (1.0 - use_of(rows, archetype) / floor).max(0.0))
}

/// Every archetype's `shortfall` in `rows`, the mean over them. A balance
/// search keeps nothing that widens it.
fn short_of_style(style: &Style, rows: &[Pairing]) -> f32 {
    EnemyArchetype::ALL.iter().map(|&archetype| shortfall(style, rows, archetype)).sum::<f32>() / EnemyArchetype::ALL.len() as f32
}

/// The attribute `archetype`'s build invests in, whose commitment is its
/// style: the line of its own skill
fn commitment(archetype: EnemyArchetype) -> Attribute {
    ActorAttributes::line(archetype.profile().ability).expect("tune: an archetype's own skill has a line")
}

/// How much `ledger`'s side worked `attribute`'s commitment against the
/// side `foe`'s ledger keeps, each a share in points but Awareness's:
/// Ferocity's, of its skills the combos fired early; Grit's, of its foe's
/// time alive the time its bind held it; Grace's, of its strikes those
/// struck across its line; Preparation's, of its reactions those fired
/// early; Patience's, of the time its stamina refilled the time at
/// Patience's faster rate; Awareness's, the damage each clear answered in
/// threats' worth, a threat's worth the mean damage of those queued on it.
fn style_use(ledger: &Ledger, foe: &Ledger, attribute: Attribute) -> f32 {
    match attribute {
        Attribute::Might => {
            let skills: u32 = ledger.used.iter().filter(|&(&ability, _)| ability != AbilityType::AutoAttack).map(|(_, &uses)| uses).sum();
            100.0 * ledger.early_combos as f32 / skills.max(1) as f32
        }
        Attribute::Vitality => 100.0 * ledger.bind / foe.alive.max(f32::EPSILON),
        Attribute::Agility => 100.0 * ledger.across as f32 / ledger.strikes.max(1) as f32,
        Attribute::Discipline => {
            let reactions: u32 = ledger.used.iter().filter(|&(ability, _)| ability.is_reaction()).map(|(_, &uses)| uses).sum();
            100.0 * ledger.through as f32 / reactions.max(1) as f32
        }
        Attribute::Instinct => 100.0 * ledger.refilling_fast / ledger.refilling.max(f32::EPSILON),
        Attribute::Resolve => {
            let answered = (ledger.queued_damage_on - ledger.landed_damage_on - ledger.pending_damage_on).max(0.0);
            let threat = ledger.queued_damage_on / ledger.queued_on.max(1) as f32;
            if threat > 0.0 { answered / ledger.clears.max(1) as f32 / threat } else { 0.0 }
        }
    }
}

/// `archetype`'s use of its commitment, the mean over its pairings
fn use_of(rows: &[Pairing], archetype: EnemyArchetype) -> f32 {
    let uses: Vec<f32> = rows.iter().filter_map(|row| match (row.a == archetype, row.b == archetype) {
        (true, _) => Some((&row.a_ledger, &row.b_ledger)),
        (_, true) => Some((&row.b_ledger, &row.a_ledger)),
        _ => None,
    }).map(|(own, foe)| style_use(own, foe, commitment(archetype))).collect();
    uses.iter().sum::<f32>() / uses.len().max(1) as f32
}

/// An archetype's name as `tune.toml` keys it
fn name(archetype: EnemyArchetype) -> String {
    format!("{archetype:?}").to_lowercase()
}

/// Each archetype's use of its commitment in `rows`, and its floor
fn style_uses(style: &Style, rows: &[Pairing]) -> String {
    EnemyArchetype::ALL.iter().map(|&archetype| {
        let floor = style.floor.get(&name(archetype)).map_or(String::new(), |floor| format!(" (floor {floor:.2})"));
        format!("{archetype:?} {:?} {:.2}{floor}", commitment(archetype), use_of(rows, archetype))
    }).collect::<Vec<_>>().join(", ")
}

/// Each archetype's mean share of wins over its pairings in `rows`
fn standings(rows: &[Pairing]) -> String {
    EnemyArchetype::ALL.iter().map(|&archetype| {
        let wins: Vec<f32> = rows.iter().filter_map(|row| match (row.a == archetype, row.b == archetype) {
            (true, _) => Some(row.a_wins),
            (_, true) => Some(row.b_wins),
            _ => None,
        }).collect();
        format!("{archetype:?} {:.0}", wins.iter().sum::<f32>() / wins.len().max(1) as f32)
    }).collect::<Vec<_>>().join(", ")
}

/// Minimizes `objective` over `ranges` by CMA-ES from `start` with a first
/// step of `sigma`, for no more than `evals` evaluations, and returns the
/// search's mean, its estimate of the best, in each setting's own units.
/// Each generation holds
/// `population` times CMA-ES's own count of candidates, every one scored on
/// the generation's seed. A point outside the bounds is scored at the
/// nearest inside and penalized by how far out it lies. Every `LOOK_EVERY`
/// generations it prints `look` of the mean as it stands.
fn search(label: &str, ranges: &[Range], start: &[f32], sigma: f64, evals: usize, population: f64, mut objective: impl FnMut(&[f32], u64) -> f32, look: impl Fn(&[f32]) -> String) -> Vec<f32> {
    let unit: Vec<f64> = ranges.iter().zip(start).map(|(range, &value)| range.to_unit(value)).collect();
    let candidates = (population * (4.0 + (3.0 * (ranges.len() as f64).ln()).floor())).round().max(2.0) as usize;
    let seeds: u64 = rand::random();
    let mut evaluated = 0;
    let penalized = |x: &DVector<f64>| -> f64 {
        let values: Vec<f32> = ranges.iter().zip(x.iter()).map(|(range, &u)| range.from_unit(u)).collect();
        let outside: f64 = x.iter().map(|&u| (u - u.clamp(0.0, 1.0)).powi(2)).sum();
        let seed = seeds.wrapping_add((evaluated / candidates) as u64);
        evaluated += 1;
        objective(&values, seed) as f64 + 100.0 * outside
    };
    let mut cma = CMAESOptions::new(unit, sigma).population_size(candidates).max_function_evals(evals).build(penalized)
        .unwrap_or_else(|error| panic!("tune: {error:?}"));
    loop {
        let done = cma.next();
        let best = cma.overall_best_individual().map_or(f64::NAN, |best| best.value);
        println!("  {label} generation {}: best {best:.2}, step {:.3}", cma.generation(), cma.sigma());
        if done.is_some() {
            break;
        }
        if cma.generation() % LOOK_EVERY == 0 {
            println!("  {label} mean at generation {}: {}", cma.generation(), look(&from_unit(ranges, cma.mean())));
        }
    }
    from_unit(ranges, cma.mean())
}

/// A point of the unit space in each setting's own units
fn from_unit(ranges: &[Range], unit: &DVector<f64>) -> Vec<f32> {
    ranges.iter().zip(unit.iter()).map(|(range, &u)| range.from_unit(u)).collect()
}

/// `values` for `ranges`, by name
fn named(ranges: &[Range], values: &[f32]) -> BTreeMap<String, f32> {
    ranges.iter().zip(values).map(|(range, &value)| (range.name.clone(), value)).collect()
}

pub fn run(args: &[String]) {
    let config = config();
    let number = |i: usize, default: usize| args.get(i).map_or(default, |arg| arg.parse().unwrap_or_else(|_| panic!("tune: {arg} is not a number")));
    match args.first().map(String::as_str) {
        Some("screen") => screen(&config),
        Some("balance") => balance(&config, number(1, 200)),
        Some("minds") => {
            let only: Vec<EnemyArchetype> = args.iter().skip(2).map(|name| super::archetype_named(name)).collect();
            minds(&config, number(1, 150), &only);
        }
        Some("exploit") => {
            let only: Vec<EnemyArchetype> = args.iter().skip(2).map(|name| super::archetype_named(name)).collect();
            exploit(&config, number(1, 150), &only);
        }
        Some("restart") => {
            let only: Vec<EnemyArchetype> = args.iter().skip(2).map(|name| super::archetype_named(name)).collect();
            assert!(!only.is_empty(), "arena tune restart names the archetypes to restart");
            restart(&config, number(1, 150), &only);
        }
        Some("settle") => settle(&config, number(1, 150)),
        Some("loop") => {
            let evals = number(2, 150);
            for round in 0..number(1, 1) {
                println!("loop {round}: minds");
                settle(&config, evals);
                println!("loop {round}: balance");
                balance(&config, evals);
            }
        }
        Some("show") => {
            let state = load();
            let rows = matrix(&settings(&config, &state, &BTreeMap::new(), &BTreeMap::new(), None, config.check_runs, rand::random()));
            print_matrix(&rows, args.get(1).is_some_and(|arg| arg == "ledger"));
            println!("imbalance {:.1}, split {:.1}; {}", imbalance(&config.score, &rows), split(&config.score, &rows), standings(&rows));
            println!("style use: {}; short of style {:.1}", style_uses(&config.style, &rows), short_of_style(&config.style, &rows));
        }
        Some("apply") => apply(&load()),
        _ => panic!("arena tune takes screen, balance [evals], minds [evals] [archetype ...], restart [evals] archetype ..., exploit [evals] [archetype ...], settle [evals], loop <count> [evals], show [ledger] or apply"),
    }
}

/// Sweeps each knob alone to its bounds and reports how far it moves the
/// matrix: the most any pairing's edge moves, and the imbalance at each end.
/// What it found is kept for the balance search to choose its knobs by.
fn screen(config: &Config) {
    let state = load();
    let none = BTreeMap::new();
    let seed = rand::random();
    let base = matrix(&settings(config, &state, &none, &none, None, config.runs, seed));
    println!("baseline imbalance {:.1}", imbalance(&config.score, &base));
    let mut moved = BTreeMap::new();
    for range in &config.knobs {
        let at = |value: f32| matrix(&settings(config, &state, &BTreeMap::from([(range.name.clone(), value)]), &none, None, config.runs, seed));
        let (low, high) = (at(range.min), at(range.max));
        let most = low.iter().zip(&high).map(|(l, h)| (l.edge - h.edge).abs()).fold(0.0, f32::max);
        println!("{:<22} moves an edge {:>5.1}  imbalance {:>5.1} .. {:>5.1}", range.name, most, imbalance(&config.score, &low), imbalance(&config.score, &high));
        moved.insert(range.name.clone(), most);
    }
    std::fs::create_dir_all(results()).expect("tune: results directory");
    std::fs::write(results().join("screen.json"), serde_json::to_string_pretty(&moved).expect("tune: screen")).expect("tune: screen.json");
}

/// The knobs screening found to move the matrix, or every knob before any
/// screen
fn screened(config: &Config) -> Vec<Range> {
    let moved: Option<BTreeMap<String, f32>> = std::fs::read_to_string(results().join("screen.json")).ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    config.knobs.iter()
        .filter(|range| moved.as_ref().is_none_or(|moved| moved.get(&range.name).is_none_or(|&most| most >= config.screen.min_moved)))
        .cloned().collect()
}

fn balance(config: &Config, evals: usize) {
    let mut state = load();
    let ranges = screened(config);
    println!("balance over {} knobs: {}", ranges.len(), ranges.iter().map(|range| range.name.as_str()).collect::<Vec<_>>().join(", "));
    let start: Vec<f32> = ranges.iter().map(|range| knob(&state, &range.name).clamp(range.min, range.max)).collect();
    let none = BTreeMap::new();
    // Fair and every style in play, in one measure: a search that weighed
    // fairness alone would price a style's skills out of use
    let unfair = |rows: &[Pairing]| imbalance(&config.score, rows) + short_of_style(&config.style, rows);
    let found = search("balance", &ranges, &start, SIGMA, evals, config.population, |values, seed| {
        unfair(&matrix(&settings(config, &state, &named(&ranges, values), &none, None, config.runs, seed)))
    }, |values| {
        let rows = matrix(&settings(config, &state, &named(&ranges, values), &none, None, config.runs, rand::random()));
        format!("imbalance {:.1}, split {:.1}, short of style {:.1}; {}", imbalance(&config.score, &rows), split(&config.score, &rows), short_of_style(&config.style, &rows), standings(&rows))
    });
    // Kept only where a longer look on the same fights says it beats where
    // it started, its split of wins and the minds' shortfall of their
    // styles no wider
    let seed = rand::random();
    let check = |values: &[f32]| matrix(&settings(config, &state, &named(&ranges, values), &none, None, config.check_runs, seed));
    let (before, after) = (check(&start), check(&found));
    let (was, now) = (unfair(&before), unfair(&after));
    let (split_was, split_now) = (split(&config.score, &before), split(&config.score, &after));
    let (short_was, short_now) = (short_of_style(&config.style, &before), short_of_style(&config.style, &after));
    let looks = format!("imbalance and short of style {was:.1} -> {now:.1}, split {split_was:.1} -> {split_now:.1}, short of style {short_was:.1} -> {short_now:.1}");
    if now < was && split_now <= split_was && short_now <= short_was {
        state.knobs.extend(named(&ranges, &found));
        save(&state);
        println!("balance kept: {looks}; {}", standings(&after));
        print_matrix(&after, false);
    } else {
        println!("balance found nothing better: {looks}");
    }
}

/// Each archetype's mind settings: the ranges every one tunes, how much
/// each other archetype's Approach makes its last skill count, and its own
fn mind_ranges(config: &Config, archetype: EnemyArchetype) -> Vec<Range> {
    let key = format!("{archetype:?}").to_lowercase();
    let approaches = EnemyArchetype::ALL.iter().filter(|&&other| other != archetype).map(|other| Range {
        name: format!("just_acted.{}", format!("{:?}", other.profile().approach).to_lowercase()),
        min: config.minds.just_acted.min,
        max: config.minds.just_acted.max,
        start: Some(1.0),
    });
    config.minds.common.iter().chain(&config.minds.strike).chain(&config.minds.reaction).cloned()
        .chain(approaches)
        .chain(config.minds.own.get(&key).into_iter().flatten().cloned())
        .map(|range| Range { name: format!("{key}.{}", range.name), ..range })
        .collect()
}

/// Passes of `minds`, an `exploit` pass before every `minds.exploit_every`th,
/// until no archetype's gain in one passes `minds.settle`, or
/// `minds.rounds` have run
fn settle(config: &Config, evals: usize) {
    for round in 0..config.minds.rounds {
        if config.minds.exploit_every > 0 && round > 0 && round % config.minds.exploit_every == 0 {
            exploit(config, evals, &[]);
        }
        let gains = minds(config, evals, &[]);
        let most = gains.iter().map(|&(_, gain)| gain).fold(0.0, f32::max);
        println!("minds round {round}: {}; most gained {most:.1}",
            gains.iter().map(|(archetype, gain)| format!("{archetype:?} {gain:+.1}")).collect::<Vec<_>>().join(", "));
        if most <= config.minds.settle {
            println!("minds settled after {} rounds", round + 1);
            return;
        }
    }
    println!("minds still moving after {} rounds", config.minds.rounds);
}

/// The minds a mind is searched against: those of the last `minds.pool` - 1
/// passes, and the minds as they stand
fn field(config: &Config, state: &State) -> Vec<BTreeMap<String, f32>> {
    let past = state.history.len().saturating_sub(config.minds.pool.saturating_sub(1));
    state.history[past..].iter().cloned().chain(std::iter::once(state.minds.clone())).collect()
}

/// How `archetype` stands with `values` for its `ranges`, held to `style`
/// where given, against each mind of `field` in turn, the pairings' `runs`
/// fights shared among them; the mean over the field
#[allow(clippy::too_many_arguments)]
fn standing_in(config: &Config, state: &State, field: &[BTreeMap<String, f32>], archetype: EnemyArchetype, ranges: &[Range], values: &[f32], runs: u32, seed: u64, style: Option<&Style>) -> f32 {
    let each = (runs / field.len() as u32 / 2).max(1) * 2;
    let none = BTreeMap::new();
    let scenarios: Vec<Settings> = field.iter().enumerate().map(|(i, minds)| {
        let mut minds = minds.clone();
        minds.extend(named(ranges, values));
        settings(config, state, &none, &minds, Some(archetype), each, seed.wrapping_add(i as u64))
    }).collect();
    let standings: Vec<f32> = matrices(&scenarios).iter().map(|rows| standing(rows, archetype, style)).collect();
    standings.iter().sum::<f32>() / standings.len() as f32
}

/// One pass over the archetypes (all, or `only`), each mind searched
/// against the field; what each gained, nothing where it kept nothing. The
/// minds the pass leaves join the field's history.
fn minds(config: &Config, evals: usize, only: &[EnemyArchetype]) -> Vec<(EnemyArchetype, f32)> {
    let mut state = load();
    let mut gains = Vec::new();
    for &archetype in EnemyArchetype::ALL.iter().filter(|archetype| only.is_empty() || only.contains(archetype)) {
        let held = held_mind(config, &state, archetype);
        gains.push((archetype, improve(config, &mut state, archetype, &held, SIGMA, evals, &format!("{archetype:?}"))));
    }
    state.history.push(state.minds.clone());
    let keep = state.history.len().saturating_sub(config.minds.pool);
    state.history.drain(..keep);
    save(&state);
    gains
}

/// `archetype`'s mind as the state holds it, a setting it holds none of at
/// its range's start or middle
fn held_mind(config: &Config, state: &State, archetype: EnemyArchetype) -> Vec<f32> {
    mind_ranges(config, archetype).iter().map(|range| state.minds.get(&range.name).copied()
        .or(range.start).unwrap_or((range.min + range.max) / 2.0).clamp(range.min, range.max)).collect()
}

/// Searches `archetype`'s mind from `start` with a first step of `sigma`,
/// against the field and held to its style, and keeps what it found where a
/// longer look on the same fights says it beats the mind `state` holds;
/// what it gained, nothing where it kept nothing
fn improve(config: &Config, state: &mut State, archetype: EnemyArchetype, start: &[f32], sigma: f64, evals: usize, label: &str) -> f32 {
    let ranges = mind_ranges(config, archetype);
    let held = held_mind(config, state, archetype);
    let field = field(config, state);
    let style = Some(&config.style);
    let found = search(label, &ranges, start, sigma, evals, config.population, |values, seed| {
        -standing_in(config, state, &field, archetype, &ranges, values, config.runs, seed, style)
    }, |values| {
        format!("standing {:.1}", standing_in(config, state, &field, archetype, &ranges, values, config.runs, rand::random(), style))
    });
    let seed = rand::random();
    let check = |values: &[f32]| standing_in(config, state, &field, archetype, &ranges, values, config.check_runs, seed, style);
    let (was, now) = (check(&held), check(&found));
    if now > was {
        state.minds.extend(named(&ranges, &found));
        save(state);
        println!("{label} kept: {was:.1} -> {now:.1} against a field of {}", field.len());
        now - was
    } else {
        println!("{label} found nothing better: {was:.1} against {now:.1}");
        0.0
    }
}

/// For each archetype in `only`, a fresh search for its mind from anywhere
/// in its bounds, as `minds` scores it, kept where it beats the mind held:
/// a mind stuck where no small change pays starts over
fn restart(config: &Config, evals: usize, only: &[EnemyArchetype]) {
    let mut state = load();
    for &archetype in only {
        let anywhere: Vec<f32> = mind_ranges(config, archetype).iter().map(|range| range.from_unit(rand::random())).collect();
        improve(config, &mut state, archetype, &anywhere, EXPLORE, evals, &format!("{archetype:?} restart"));
    }
}

/// For each archetype (all, or `only`), a fresh search from anywhere in the
/// bounds for its best answer to the minds as they stand, scored on winning
/// alone, and how far that beats the mind it has on the same fights, also
/// scored on winning alone: the price of its style. Reported only; no
/// exploiter joins the field.
fn exploit(config: &Config, evals: usize, only: &[EnemyArchetype]) {
    let state = load();
    for &archetype in EnemyArchetype::ALL.iter().filter(|archetype| only.is_empty() || only.contains(archetype)) {
        let ranges = mind_ranges(config, archetype);
        let held = held_mind(config, &state, archetype);
        let anywhere: Vec<f32> = ranges.iter().map(|range| range.from_unit(rand::random())).collect();
        let label = format!("{archetype:?} exploiter");
        let as_they_stand = std::slice::from_ref(&state.minds);
        let found = search(&label, &ranges, &anywhere, EXPLORE, evals, config.population, |values, seed| {
            -standing_in(config, &state, as_they_stand, archetype, &ranges, values, config.runs, seed, None)
        }, |values| {
            format!("standing {:.1}", standing_in(config, &state, as_they_stand, archetype, &ranges, values, config.runs, rand::random(), None))
        });
        let seed = rand::random();
        let check = |values: &[f32]| standing_in(config, &state, as_they_stand, archetype, &ranges, values, config.check_runs, seed, None);
        let (mind, exploiter) = (check(&held), check(&found));
        println!("{label}: {exploiter:.1} against its mind's {mind:.1} on winning alone, {:+.1}", exploiter - mind);
    }
}

/// Writes the state's knobs into `Tuning`'s defaults and its minds into
/// `mind::TUNED`
fn apply(state: &State) {
    let path = manifest().join("../common-bevy/src/tuning.rs");
    let text = std::fs::read_to_string(&path).expect("tune: tuning.rs");
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<String> = text.split(nl).map(|line| {
        let Some((name, _)) = line.strip_prefix("        ").and_then(|rest| rest.split_once(": ")) else { return line.to_owned() };
        match state.knobs.get(name) {
            // A whole-tile knob is written whole
            Some(&value) if name == "leap_distance" => format!("        {name}: {},", value.round().max(1.0) as usize),
            Some(&value) => format!("        {name}: {:?},", (value * 1000.0).round() / 1000.0),
            None => line.to_owned(),
        }
    }).collect();
    std::fs::write(&path, lines.join(nl)).expect("tune: tuning.rs");

    let path = manifest().join("../combat/src/behaviour/mind.rs");
    let text = std::fs::read_to_string(&path).expect("tune: mind.rs");
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let open = "pub const TUNED: &[(&str, f32)] = &[";
    let from = text.find(open).expect("tune: mind.rs holds no TUNED") + open.len() + nl.len();
    let to = from + text[from..].find("];").expect("tune: TUNED unclosed");
    let entries: Vec<String> = state.minds.iter().map(|(key, value)| format!("(\"{key}\", {:?})", (value * 1000.0).round() / 1000.0)).collect();
    let body: String = entries.chunks(3).map(|chunk| format!("    {},{nl}", chunk.join(", "))).collect();
    std::fs::write(&path, format!("{}{body}{}", &text[..from], &text[to..])).expect("tune: mind.rs");
    println!("applied {} knobs and {} mind settings", state.knobs.len(), state.minds.len());
}
