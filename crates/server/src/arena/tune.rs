//! # Tuning search
//!
//! `cargo run --bin server --features tune -- arena tune <command>` searches
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
//!   imbalance
//! - `minds [evals] [archetype ...]`: CMA-ES over each archetype's mind, for
//!   its own score against the rest as they stand
//! - `loop <count> [evals]`: minds, then balance, `count` times
//! - `show [ledger]`: the matrix as the state stands
//! - `apply`: writes the state into the source
//!
//! Each search runs in a space where every setting's bounds are 0 and 1,
//! starts from the state, and keeps what it found only where a second,
//! longer evaluation says it beats the start: a search never makes the
//! state worse.

use std::{collections::BTreeMap, path::PathBuf};

use cmaes::{CMAESOptions, DVector};
use serde::{Deserialize, Serialize};

use common_bevy::{archetype::EnemyArchetype, tuning::Tuning};

use super::{matrix, print_matrix, Pairing, Settings};
use crate::systems::behaviour::mind::{Minds, TUNED};

/// The step a search starts with, in the space where each bound is 0 and 1
const SIGMA: f64 = 0.2;

#[derive(Deserialize)]
struct Config {
    runs: u32,
    skill: String,
    fixed: BTreeMap<String, f32>,
    score: Score,
    screen: Screen,
    knobs: Vec<Range>,
    minds: MindRanges,
}

#[derive(Deserialize)]
struct Score {
    band: f32,
    edge_band: f32,
    edge: f32,
    short: f32,
    capped: f32,
    mind_edge: f32,
    draw: f32,
    mind_capped: f32,
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
    just_acted: Bounds,
    common: Vec<Range>,
    strike: Vec<Range>,
    reaction: Vec<Range>,
    own: BTreeMap<String, Vec<Range>>,
}

/// The numbers and minds found so far, by the names the arena sets them by
#[derive(Default, Deserialize, Serialize)]
struct State {
    knobs: BTreeMap<String, f32>,
    minds: BTreeMap<String, f32>,
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn results() -> PathBuf {
    manifest().join("../../proofs/arena/tune")
}

fn config() -> Config {
    let path = manifest().join("src/arena/tune.toml");
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
/// `minds` laid over it, and `focus`'s pairings alone where given
fn settings(config: &Config, state: &State, knobs: &BTreeMap<String, f32>, minds: &BTreeMap<String, f32>, focus: Option<EnemyArchetype>, runs: u32) -> Settings {
    let mut settings = Settings::parse(&[format!("runs={runs}"), format!("skill={}", config.skill)]);
    settings.focus = focus;
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

/// How far `rows` lie from fair: what each pairing's split lies past
/// `band` from even (a fight both sides die in is a draw, a win for
/// neither), its edge past `edge_band`, its median fight short of `short`
/// seconds, and its fights run to the cap; the mean over pairings.
fn imbalance(score: &Score, rows: &[Pairing]) -> f32 {
    let each = rows.iter().map(|row| {
        ((row.a_wins - row.b_wins).abs() / 2.0 - score.band).max(0.0)
            + score.edge * (row.edge.abs() - score.edge_band).max(0.0)
            + (score.short - row.median).max(0.0)
            + score.capped * row.capped
    });
    each.sum::<f32>() / rows.len().max(1) as f32
}

/// How well `archetype` does in `rows`: its share of wins, more by
/// `mind_edge` of its edge and `draw` of the fights both sides die in, less
/// by `mind_capped` of those run to the cap; the mean over its pairings.
fn standing(score: &Score, rows: &[Pairing], archetype: EnemyArchetype) -> f32 {
    let mine: Vec<f32> = rows.iter().filter(|row| row.a == archetype || row.b == archetype).map(|row| {
        let (own, edge) = if row.a == archetype { (row.a_wins, row.edge) } else { (row.b_wins, -row.edge) };
        let draw = (100.0 - row.a_wins - row.b_wins).max(0.0);
        own + score.mind_edge * edge + score.draw * draw - score.mind_capped * row.capped
    }).collect();
    mine.iter().sum::<f32>() / mine.len().max(1) as f32
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

/// Minimizes `objective` over `ranges` by CMA-ES from `start`, for no more
/// than `evals` evaluations, and returns the search's mean, its estimate of
/// the best, in each setting's own units. A point outside the bounds is
/// scored at the nearest inside and penalized by how far out it lies.
fn search(label: &str, ranges: &[Range], start: &[f32], evals: usize, mut objective: impl FnMut(&[f32]) -> f32) -> Vec<f32> {
    let unit: Vec<f64> = ranges.iter().zip(start).map(|(range, &value)| range.to_unit(value)).collect();
    let mut evaluated = 0;
    let penalized = |x: &DVector<f64>| -> f64 {
        let values: Vec<f32> = ranges.iter().zip(x.iter()).map(|(range, &u)| range.from_unit(u)).collect();
        let outside: f64 = x.iter().map(|&u| (u - u.clamp(0.0, 1.0)).powi(2)).sum();
        evaluated += 1;
        objective(&values) as f64 + 100.0 * outside
    };
    let mut cma = CMAESOptions::new(unit, SIGMA).max_function_evals(evals).build(penalized)
        .unwrap_or_else(|error| panic!("tune: {error:?}"));
    loop {
        let done = cma.next();
        let best = cma.overall_best_individual().map_or(f64::NAN, |best| best.value);
        println!("  {label} generation {}: best {best:.2}, step {:.3}", cma.generation(), cma.sigma());
        if done.is_some() {
            break;
        }
    }
    ranges.iter().zip(cma.mean().iter()).map(|(range, &u)| range.from_unit(u)).collect()
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
            minds(&config, number(1, 150), &only)
        }
        Some("loop") => {
            let evals = number(2, 150);
            for round in 0..number(1, 1) {
                println!("loop {round}: minds");
                minds(&config, evals, &[]);
                println!("loop {round}: balance");
                balance(&config, evals);
            }
        }
        Some("show") => {
            let state = load();
            let rows = matrix(&settings(&config, &state, &BTreeMap::new(), &BTreeMap::new(), None, config.runs));
            print_matrix(&rows, args.get(1).is_some_and(|arg| arg == "ledger"));
            println!("imbalance {:.1}; {}", imbalance(&config.score, &rows), standings(&rows));
        }
        Some("apply") => apply(&load()),
        _ => panic!("arena tune takes screen, balance [evals], minds [evals] [archetype ...], loop <count> [evals], show [ledger] or apply"),
    }
}

/// Sweeps each knob alone to its bounds and reports how far it moves the
/// matrix: the most any pairing's edge moves, and the imbalance at each end.
/// What it found is kept for the balance search to choose its knobs by.
fn screen(config: &Config) {
    let state = load();
    let none = BTreeMap::new();
    let base = matrix(&settings(config, &state, &none, &none, None, config.runs));
    println!("baseline imbalance {:.1}", imbalance(&config.score, &base));
    let mut moved = BTreeMap::new();
    for range in &config.knobs {
        let at = |value: f32| matrix(&settings(config, &state, &BTreeMap::from([(range.name.clone(), value)]), &none, None, config.runs));
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
    let found = search("balance", &ranges, &start, evals, |values| {
        imbalance(&config.score, &matrix(&settings(config, &state, &named(&ranges, values), &none, None, config.runs)))
    });
    // Kept only where a longer look says it beats where it started
    let check = |values: &[f32]| matrix(&settings(config, &state, &named(&ranges, values), &none, None, config.runs * 2));
    let (before, after) = (check(&start), check(&found));
    let (was, now) = (imbalance(&config.score, &before), imbalance(&config.score, &after));
    if now < was {
        state.knobs.extend(named(&ranges, &found));
        save(&state);
        println!("balance kept: imbalance {was:.1} -> {now:.1}; {}", standings(&after));
        print_matrix(&after, false);
    } else {
        println!("balance found nothing better: {was:.1} against {now:.1}");
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

fn minds(config: &Config, evals: usize, only: &[EnemyArchetype]) {
    let mut state = load();
    let none = BTreeMap::new();
    for &archetype in EnemyArchetype::ALL.iter().filter(|archetype| only.is_empty() || only.contains(archetype)) {
        let ranges = mind_ranges(config, archetype);
        let start: Vec<f32> = ranges.iter().map(|range| state.minds.get(&range.name).copied()
            .or(range.start).unwrap_or((range.min + range.max) / 2.0).clamp(range.min, range.max)).collect();
        let label = format!("{archetype:?}");
        let found = search(&label, &ranges, &start, evals, |values| {
            -standing(&config.score, &matrix(&settings(config, &state, &none, &named(&ranges, values), Some(archetype), config.runs)), archetype)
        });
        let check = |values: &[f32]| standing(&config.score, &matrix(&settings(config, &state, &none, &named(&ranges, values), Some(archetype), config.runs * 2)), archetype);
        let (was, now) = (check(&start), check(&found));
        if now > was {
            state.minds.extend(named(&ranges, &found));
            save(&state);
            println!("{label} kept: {was:.1} -> {now:.1}");
        } else {
            println!("{label} found nothing better: {was:.1} against {now:.1}");
        }
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
            Some(&value) => format!("        {name}: {},", (value * 1000.0).round() / 1000.0),
            None => line.to_owned(),
        }
    }).collect();
    std::fs::write(&path, lines.join(nl)).expect("tune: tuning.rs");

    let path = manifest().join("src/systems/behaviour/mind.rs");
    let text = std::fs::read_to_string(&path).expect("tune: mind.rs");
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let open = "pub const TUNED: &[(&str, f32)] = &[";
    let from = text.find(open).expect("tune: mind.rs holds no TUNED") + open.len() + nl.len();
    let to = from + text[from..].find("];").expect("tune: TUNED unclosed");
    let entries: Vec<String> = state.minds.iter().map(|(key, value)| format!("(\"{key}\", {})", (value * 1000.0).round() / 1000.0)).collect();
    let body: String = entries.chunks(3).map(|chunk| format!("    {},{nl}", chunk.join(", "))).collect();
    std::fs::write(&path, format!("{}{body}{}", &text[..from], &text[to..])).expect("tune: mind.rs");
    println!("applied {} knobs and {} mind settings", state.knobs.len(), state.minds.len());
}
