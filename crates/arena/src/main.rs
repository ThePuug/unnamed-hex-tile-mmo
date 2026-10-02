//! # Balance arena
//!
//! `cargo run --bin arena -- [key=value ...]` sets NPC archetypes
//! against each other on a flat, empty map and reports who wins. Each fight
//! is its own headless app running `CombatPlugin` and `BehaviourPlugin`,
//! the rules the live server runs, stepped on a manual clock so it goes as
//! fast as the CPU allows. No networking, terrain or players.
//!
//! Keys: `level` (10), `size` NPCs per side (1), `skill` its NPCs fight
//! with (`sharp`, `steady`, `sloppy`, or `fastest-slowest/error` in
//! milliseconds and a share; sharp), `b_level`, `b_size` and `b_skill` to
//! set the second archetype's side apart (the same by default), `runs` per
//! matchup (20), `cap` seconds after which a fight with both sides standing
//! is lost on both (120), `only` a comma
//! list of archetypes to restrict the matchups to, `mirror=1` to fight each
//! archetype against itself instead of the others, `ordered=1` to fight every
//! ordered pair, mirrors included, `trace` to print every
//! fight (1), with a timeline every 5s (2), or every half second for its
//! first 12s (3), with every NPC decision beside it, the three best and
//! each consideration's response (4). `ledger=1` prints each side's ledger
//! under its pairing's row (below). `seed` sets where every fight's rolls
//! come from (random, and printed): a scenario run again with its seed
//! fights the same fights. Any `Tuning` knob may be set by name too
//! (`frenzy_damage=2`), so a value is tried without a rebuild; `bar` a comma
//! list of the skills every fighter weighs in place of its archetype's own
//! (`bar=parry,feint`); and any mind
//! setting as `mind.<all|archetype>.<setting>` (`combat::behaviour::mind`), so a
//! search tunes each archetype's fighter the same way.
//!
//! `arena serve` runs one scenario per line of stdin, each line the keys
//! above, and ends each report with a line `end`. `arena tune ...` searches
//! the game's numbers and the NPCs' minds in-process ([`tune`]).
//!
//! Every pairing fights `runs` times, the two swapping ends each run, each
//! team starting on its end's tile or a neighbour and each fighter facing
//! its foes give or take 30°, so neither end's start decides a close fight. All of a scenario's fights share one
//! pool of workers, so no pairing waits on another's slowest fight, and
//! each fight runs single-threaded on its worker. The report
//! gives each pairing's win split, median fight length, the winners' health
//! left, a's edge (the share of its health a has left at the end less b's,
//! in points, averaged over the runs: how far a won or lost by; a fight run
//! to the cap adds nothing, as it is never won on health), and where
//! each side's damage came from: auto-attacks, skills, or reflections.
//!
//! A side's ledger says why, per fight: each ability's uses, the damage it
//! sent into its foes' queues and the share of what settled that landed, the
//! rest answered (a reaction's is what it reflected); then the share of the
//! fight it spent recovering, held, slowed, with its target beyond its
//! reach and circling it, and its mean fatigue.

mod tune;

use std::{collections::HashMap, time::Duration};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use qrz::Qrz;

use common_bevy::{
    components::{
        behaviour::Side,
        entity_type::{decorator::Decorator, EntityType},
        heading::Heading,
        reaction_queue::{QueuedThreat, ReactionQueue},
        recovery::GlobalRecovery,
        resources::{Endurance, Health, SpawnPoint},
        status::Status,
        target::Target,
        AttackRange, Loc,
    },
    message::{AbilityType, Do, Event, Try},
    plugins::nntree::NNTreePlugin,
    resources::map::Map,
    archetype::EnemyArchetype,
    tuning::{set_tuning, Tuning},
};

use combat::{
    actor,
    behaviour::{mind::{set_minds, Minds}, moves::Move, perception::Skill, Bar, Decisions},
    dice::Dice,
    engagement::{engaging_at, spawn_engagement, STAGE_GAP},
    BehaviourPlugin, CombatPlugin,
};

/// One simulated frame. FixedUpdate's 125ms tick runs every second frame.
const STEP: Duration = Duration::from_micros(62_500);

/// Flat tiles laid out round the origin; wide enough that a fleeing Kiter
/// reaches its leash before the edge.
const ARENA_RADIUS: i32 = 80;

/// One side of a fight: `size` NPCs of `archetype` at `level`, fighting
/// with `skill`.
#[derive(Clone, Copy)]
struct Team {
    archetype: EnemyArchetype,
    level: u8,
    size: u8,
    skill: Skill,
}

struct Settings {
    level: u8,
    size: u8,
    b_level: Option<u8>,
    b_size: Option<u8>,
    skill: Skill,
    b_skill: Option<Skill>,
    bar: Option<Vec<AbilityType>>,
    mirror: bool,
    ordered: bool,
    runs: u32,
    cap: Duration,
    only: Vec<EnemyArchetype>,
    /// Only the pairings this archetype fights in
    focus: Option<EnemyArchetype>,
    trace: u8,
    ledger: bool,
    /// Where every fight's rolls come from: a pairing's run fights the same
    /// fight under any scenario that shares it
    seed: u64,
    tuning: Tuning,
    minds: Minds,
}

impl Settings {
    fn parse(args: &[String]) -> Self {
        let mut settings = Settings { level: 10, size: 1, b_level: None, b_size: None, skill: Skill::SHARP, b_skill: None, bar: None, mirror: false, ordered: false, runs: 20, cap: Duration::from_secs(120), only: EnemyArchetype::ALL.to_vec(), focus: None, trace: 0, ledger: false, seed: rand::random(), tuning: Tuning::default(), minds: Minds::tuned() };
        for arg in args {
            let (key, value) = arg.split_once('=').unwrap_or_else(|| panic!("arena takes key=value, not {arg}"));
            match key {
                "level" => settings.level = value.parse().expect("level is a whole number"),
                "size" => settings.size = value.parse().expect("size is a whole number"),
                "b_level" => settings.b_level = Some(value.parse().expect("b_level is a whole number")),
                "b_size" => settings.b_size = Some(value.parse().expect("b_size is a whole number")),
                "skill" => settings.skill = Skill::named(value).unwrap_or_else(|error| panic!("arena: {error}")),
                "b_skill" => settings.b_skill = Some(Skill::named(value).unwrap_or_else(|error| panic!("arena: {error}"))),
                "bar" => settings.bar = Some(value.split(',').map(ability_named).collect()),
                "mirror" => settings.mirror = value == "1",
                "ordered" => settings.ordered = value == "1",
                "runs" => settings.runs = value.parse().expect("runs is a whole number"),
                "cap" => settings.cap = Duration::from_secs(value.parse().expect("cap is whole seconds")),
                "only" => settings.only = value.split(',').map(archetype_named).collect(),
                "trace" => settings.trace = value.parse().expect("trace is 0 to 4"),
                "ledger" => settings.ledger = value == "1",
                "seed" => settings.seed = value.parse().expect("seed is a whole number"),
                _ if key.starts_with("mind.") => settings.minds.set(&key["mind.".len()..], value).unwrap_or_else(|error| panic!("arena: {error}")),
                _ => settings.tuning.set(key, value).unwrap_or_else(|error| panic!("arena: {error}")),
            }
        }
        settings
    }

    fn team_a(&self, archetype: EnemyArchetype) -> Team {
        Team { archetype, level: self.level, size: self.size, skill: self.skill }
    }

    fn team_b(&self, archetype: EnemyArchetype) -> Team {
        Team { archetype, level: self.b_level.unwrap_or(self.level), size: self.b_size.unwrap_or(self.size), skill: self.b_skill.unwrap_or(self.skill) }
    }
}

fn ability_named(name: &str) -> AbilityType {
    use AbilityType::*;
    [Frenzy, Feint, Overpower, Punish, Parry, Counter, Leap, PerfectStride].into_iter()
        .find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no skill {name}"))
}

fn archetype_named(name: &str) -> EnemyArchetype {
    EnemyArchetype::ALL.into_iter()
        .find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no archetype {name}"))
}

/// What one side did over its fights. Damage is keyed by the ability that
/// sent it.
#[derive(Clone, Default)]
struct Ledger {
    used: HashMap<AbilityType, u32>,
    /// Damage it put in its foes' queues, damage over time whole
    sent: HashMap<Option<AbilityType>, f32>,
    /// Damage of its that landed, a reflection's included
    landed: HashMap<Option<AbilityType>, f32>,
    /// Damage of its still queued when the fight ended, neither landed nor
    /// answered
    pending: HashMap<Option<AbilityType>, f32>,
    /// Seconds alive, and of those recovering, held, slowed and with its
    /// target beyond its reach; and its fatigue summed over them
    alive: f32,
    recovering: f32,
    held: f32,
    slowed: f32,
    beyond_reach: f32,
    circling: f32,
    fatigue: f32,
}

impl Ledger {
    fn merge(&mut self, other: &Ledger) {
        for (ability, n) in &other.used {
            *self.used.entry(*ability).or_default() += n;
        }
        for (mine, theirs) in [(&mut self.sent, &other.sent), (&mut self.landed, &other.landed), (&mut self.pending, &other.pending)] {
            for (ability, damage) in theirs {
                *mine.entry(*ability).or_default() += damage;
            }
        }
        self.alive += other.alive;
        self.recovering += other.recovering;
        self.held += other.held;
        self.slowed += other.slowed;
        self.beyond_reach += other.beyond_reach;
        self.circling += other.circling;
        self.fatigue += other.fatigue;
    }

    fn landed_total(&self) -> f32 {
        self.landed.values().sum()
    }

    /// Landed damage as auto-attacks, skills and reflections
    fn sources(&self) -> (f32, f32, f32) {
        let of = |ability: AbilityType| self.landed.get(&Some(ability)).copied().unwrap_or(0.0);
        let (auto, reflect) = (of(AbilityType::AutoAttack), of(AbilityType::Counter));
        (auto, self.landed_total() - auto - reflect, reflect)
    }
}

/// The two sides of a fight and each side's ledger.
#[derive(Resource, Default)]
struct Tally {
    sides: HashMap<Entity, Side>,
    ledgers: HashMap<Side, Ledger>,
}

impl Tally {
    fn of(&mut self, ent: Entity) -> Option<&mut Ledger> {
        let side = *self.sides.get(&ent)?;
        Some(self.ledgers.entry(side).or_default())
    }
}

/// A threat's whole damage, what of its DoT has not ticked included.
fn whole(threat: &QueuedThreat) -> f32 {
    threat.damage + threat.dot_left()
}

/// Counts each ability an actor uses, auto-attacks included, and each
/// threat it sends.
fn tally_sent(mut reader: MessageReader<Do>, mut tally: ResMut<Tally>) {
    for message in reader.read() {
        match &message.event {
            Event::UseAbility { ent, ability, .. } => if let Some(ledger) = tally.of(*ent) {
                *ledger.used.entry(*ability).or_default() += 1;
            },
            Event::InsertThreat { threat, .. } => if let Some(ledger) = tally.of(threat.source) {
                *ledger.sent.entry(threat.ability).or_default() += whole(threat);
            },
            _ => {}
        }
    }
}

/// Counts each resolved threat and DoT tick against the side of the actor
/// that sent it.
fn tally_resolved(trigger: On<Try>, mut tally: ResMut<Tally>) {
    let (source, ability, damage) = match trigger.event() {
        Try { event: Event::ResolveThreat { threat, .. } } => (threat.source, threat.ability, whole(threat)),
        Try { event: Event::DotTick { source, ability, damage, .. } } => (*source, *ability, *damage),
        _ => return,
    };
    let Some(ledger) = tally.of(source) else { return };
    *ledger.landed.entry(ability).or_default() += damage;
}

/// Adds a frame of `step` seconds to each living actor's side: whether it
/// is recovering, held, slowed, has its target beyond its reach or circles
/// it, and its fatigue.
fn tally_states(world: &mut World, step: f32) {
    let mut locs = world.query::<(Entity, &Loc)>();
    let locs: HashMap<Entity, Loc> = locs.iter(world).map(|(ent, loc)| (ent, *loc)).collect();
    let mut actors = world.query::<(Entity, &Health, &Loc, &AttackRange, Option<&Target>, Option<&GlobalRecovery>, Option<&Status>, Option<&Endurance>, Option<&Move>)>();
    let frames: Vec<_> = actors.iter(world).filter(|(_, health, ..)| health.state > 0.0).map(|(ent, _, loc, range, target, recovery, status, endurance, under_way)| {
        let beyond = target.and_then(|target| target.entity).and_then(|foe| locs.get(&foe)).is_some_and(|foe| loc.flat_distance(foe) > range.0);
        (ent, recovery.is_some(), Status::holds(status), status.is_some_and(|status| status.slow.is_some()), beyond, under_way == Some(&Move::Circle), Endurance::fatigue_of(endurance))
    }).collect();
    let mut tally = world.resource_mut::<Tally>();
    for (ent, recovering, held, slowed, beyond, circling, fatigue) in frames {
        let Some(ledger) = tally.of(ent) else { continue };
        let during = |state: bool| if state { step } else { 0.0 };
        ledger.alive += step;
        ledger.recovering += during(recovering);
        ledger.held += during(held);
        ledger.slowed += during(slowed);
        ledger.beyond_reach += during(beyond);
        ledger.circling += during(circling);
        ledger.fatigue += fatigue * step;
    }
}

struct Outcome {
    winner: Option<Side>,
    length: Duration,
    /// The winners' remaining health as a fraction of their total
    left: f32,
    /// Run to the cap with both sides standing: a loss for both, never won
    /// on health
    timed_out: bool,
    ledgers: HashMap<Side, Ledger>,
    /// Each side's health left at the end as a fraction of its total
    shares: HashMap<Side, f32>,
}

const WEST: Side = Side(1);
const EAST: Side = Side(2);

/// `at` or one of its neighbours no further from `foes`, at random: a
/// team's start is not fixed to its tile, and never leaves the range the
/// stage set it at to be spotted.
fn jitter(at: Qrz, foes: Qrz, dice: &Dice) -> Qrz {
    let near = at.flat_distance(&foes);
    let starts: Vec<Qrz> = std::iter::once(at)
        .chain(qrz::DIRECTIONS.iter().map(|&d| at + d))
        .filter(|tile| tile.flat_distance(&foes) <= near)
        .collect();
    starts[dice.roll(("start", at)).pick(starts.len())]
}

/// The flat arena every fight stands on, laid once: a `Map` shares its
/// tiles between clones, and no fight changes them.
fn flat_map() -> Map {
    static ARENA: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
    ARENA.get_or_init(lay_flat_map).clone()
}

fn lay_flat_map() -> Map {
    let map = Map::new(qrz::Map::<EntityType>::new(
        common::camera::HEX_RADIUS,
        common::camera::RISE,
        qrz::HexOrientation::FlatTop,
    ));
    for q in -ARENA_RADIUS..=ARENA_RADIUS {
        for r in -ARENA_RADIUS..=ARENA_RADIUS {
            if (q + r).abs() <= ARENA_RADIUS {
                map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator::default()));
            }
        }
    }
    map
}

/// Fights `west` against `east` until one side is dead or `cap` passes,
/// every roll drawn from `seed`: the same seed and settings fight the same
/// fight.
fn fight(west: Team, east: Team, settings: &Settings, seed: u64) -> Outcome {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, NNTreePlugin, BehaviourPlugin, CombatPlugin));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(flat_map());
    app.insert_resource(SpawnPoint(Qrz { q: 0, r: 0, z: 1 }));
    app.init_resource::<Tally>();
    app.insert_resource(Dice::seeded(seed));
    if settings.trace > 3 {
        app.init_resource::<Decisions>();
    }
    app.add_systems(Update, (actor::update, tally_sent));
    app.add_systems(PostUpdate, actor::cleanup_despawned);
    app.add_observer(tally_resolved);
    app.finish();
    app.cleanup();
    // Fights already fill every core, one to a worker thread: a fight's
    // systems run on its own thread rather than queue for Bevy's shared
    // pool, several times faster. Systems whose order is unpinned then run
    // in one order rather than varying frame to frame as on the server.
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    }

    let world = app.world_mut();
    let time = world.resource::<Time>().clone();
    // The east team engages the west as a staged party engages the fighter
    // it is spawned on, the stage's gap apart about the origin
    let west_at = Qrz { q: -STAGE_GAP / 2, r: 0, z: 1 };
    let east_at = engaging_at(west_at, Qrz { q: 1, r: 0, z: 0 }, |_, _| 0);
    let dice = *world.resource::<Dice>();
    {
        let mut commands = world.commands();
        for (team, side, at, foes) in [(west, WEST, west_at, east_at), (east, EAST, east_at, west_at)] {
            spawn_engagement(jitter(at, foes, &dice), team.archetype, side, team.level, team.size, |_, _| 0, &mut commands, &time);
        }
    }
    world.flush();
    let mut skills = world.query::<(&Side, &mut Skill)>();
    for (side, mut skill) in skills.iter_mut(world) {
        *skill = if *side == WEST { west.skill } else { east.skill };
    }
    if let Some(bar) = &settings.bar {
        let mut bars = world.query::<&mut Bar>();
        for mut held in bars.iter_mut(world) {
            held.0 = bar.clone();
        }
    }
    // Nothing but the damage roll varies a fight, so a start the two ends
    // do not share would decide a close one the same way every run: each
    // fighter faces its foes' end give or take two slots, near enough to
    // spot them as a staged party would
    let map = world.resource::<Map>().clone();
    let mut fighters = world.query::<(Entity, &Side, &common_bevy::components::Loc, &mut Heading)>();
    for (ent, side, loc, mut heading) in fighters.iter_mut(world) {
        let foes = if *side == WEST { east_at } else { west_at };
        let toward = Heading::between(&map, **loc, foes).unwrap_or(*heading);
        *heading = toward.turned(dice.roll(("facing", ent)).pick(5) as i32 - 2);
    }
    let mut sides = world.query::<(Entity, &Side)>();
    let roster: HashMap<Entity, Side> = sides.iter(world).map(|(e, s)| (e, *s)).collect();
    world.resource_mut::<Tally>().sides = roster;

    let mut health = app.world_mut().query::<(&Side, &Health)>();
    let mut full: HashMap<Side, f32> = HashMap::new();
    for (side, hp) in health.iter(app.world_mut()) {
        *full.entry(*side).or_default() += hp.max;
    }
    let mut timed_out = false;
    let mut elapsed = Duration::ZERO;
    let (winner, left) = loop {
        app.update();
        elapsed += STEP;
        let world = app.world_mut();
        tally_states(world, STEP.as_secs_f32());
        if let Some(mut decisions) = world.get_resource_mut::<Decisions>() {
            for line in decisions.0.drain(..) {
                println!("    t={:5.2}s {line}", elapsed.as_secs_f32());
            }
        }
        let mut alive: HashMap<Side, (f32, f32)> = HashMap::new();
        for (side, hp) in health.iter(world) {
            if hp.state > 0.0 {
                let entry = alive.entry(*side).or_default();
                entry.0 += hp.state;
                entry.1 += hp.max;
            }
        }
        if settings.trace > 1 && elapsed.as_millis() % (if settings.trace > 2 { 500 } else { 5000 }) == 0 && (settings.trace < 3 || elapsed.as_secs() < 12) {
            timeline(world, elapsed);
        }
        match (alive.get(&WEST), alive.get(&EAST)) {
            (Some(&(state, max)), None) => break (Some(WEST), state / max),
            (None, Some(&(state, max))) => break (Some(EAST), state / max),
            (None, None) => break (None, 0.0),
            (Some(_), Some(_)) if elapsed >= settings.cap => {
                timed_out = true;
                break (None, 0.0);
            }
            _ => {}
        }
    };
    let mut queues = app.world_mut().query::<&ReactionQueue>();
    let pending: Vec<QueuedThreat> = queues.iter(app.world()).flat_map(|queue| queue.threats.iter().copied()).collect();
    let mut tally = std::mem::take(&mut *app.world_mut().resource_mut::<Tally>());
    for threat in pending {
        if let Some(ledger) = tally.of(threat.source) {
            *ledger.pending.entry(threat.ability).or_default() += whole(&threat);
        }
    }
    if settings.trace > 0 {
        let used = |side: Side| {
            let used: Vec<_> = tally.ledgers.get(&side).map_or(Vec::new(), |ledger| ledger.used.iter().map(|(a, n)| format!("{a:?} {n}")).collect());
            used.join(", ")
        };
        let hp: Vec<_> = health.iter(app.world_mut()).map(|(s, h)| format!("{}:{:.0}/{:.0}", s.0, h.state, h.max)).collect();
        println!("  {}x{:?}@{} (1) v {}x{:?}@{} (2): winner {:?} after {:.1}s, hp [{}]; 1 used [{}] dealt {:.0}; 2 used [{}] dealt {:.0}",
            west.size, west.archetype, west.level, east.size, east.archetype, east.level,
            winner.map(|s| s.0), elapsed.as_secs_f32(), hp.join(" "),
            used(WEST), tally.ledgers.get(&WEST).map_or(0.0, Ledger::landed_total),
            used(EAST), tally.ledgers.get(&EAST).map_or(0.0, Ledger::landed_total));
    }
    let mut shares: HashMap<Side, f32> = HashMap::new();
    for (side, hp) in health.iter(app.world_mut()) {
        *shares.entry(*side).or_default() += hp.state.max(0.0) / full[side];
    }
    Outcome { winner, length: elapsed, left, timed_out, ledgers: tally.ledgers, shares }
}

/// Prints where every actor stands and what it is doing.
fn timeline(world: &mut World, elapsed: Duration) {
    use common_bevy::components::{Loc, hex_assignment::AssignedHex, resources::CombatState, returning::Returning, status::Status, target::Target};
    let mut actors = world.query::<(Entity, &Side, &Loc, &Health, &CombatState, Option<&Target>, Option<&Returning>, Option<&AssignedHex>, Option<&Status>, &common_bevy::components::position::Position)>();
    let lines: Vec<_> = actors.iter(world).map(|(e, side, loc, hp, combat, target, returning, assigned, status, pos)| {
        format!("{}#{} {:?} pos {:?}+({:.2},{:.2}) hp {:.0}{}{}{}{} ->{:?}", side.0, e.index(), (loc.q, loc.r, loc.z), (pos.tile.q, pos.tile.r), pos.offset.x, pos.offset.z, hp.state,
            if combat.in_combat { " fighting" } else { "" },
            if returning.is_some() { " RETURNING" } else { "" },
            assigned.map_or(String::new(), |a| format!(" hex {:?}", (a.0.q, a.0.r, a.0.z))),
            if Status::holds(status) { " HELD" } else { "" },
            target.and_then(|t| t.entity).map(|t| t.index()))
    }).collect();
    let mut engagements = world.query::<&common_bevy::components::hex_assignment::HexAssignment>();
    let assigning: Vec<_> = engagements.iter(world).map(|h| format!("{:?}@{:?}", h.target_player.map(|e| e.index()), h.last_player_tile.map(|t| (t.q, t.r)))).collect();
    println!("    t={:>5.1}s {} || engagements {}", elapsed.as_secs_f32(), lines.join(" | "), assigning.join(" "));
}

/// Runs the arena: `serve` answers scenarios from stdin, `tune` searches,
/// anything else is one scenario's keys.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(),
        Some("tune") => tune::run(&args[1..]),
        _ => report(&Settings::parse(&args)),
    }
}

/// Answers each line of stdin as a scenario, closing each report with `end`.
/// A line that fails reports `error` in its place and the next still runs.
fn serve() {
    use std::io::{BufRead, Write};
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        if args.is_empty() {
            continue;
        }
        if std::panic::catch_unwind(|| report(&Settings::parse(&args))).is_err() {
            println!("error");
        }
        println!("end");
        std::io::stdout().flush().expect("arena: stdout closed");
    }
}

/// Runs every pairing under the scenario's tuning and prints the report.
fn report(settings: &Settings) {
    let (a_team, b_team) = (settings.team_a(EnemyArchetype::Berserker), settings.team_b(EnemyArchetype::Berserker));
    println!(
        "arena: a is {} at level {}, b is {} at level {}; {} runs per pairing, a loss for both sides standing at {}s; seed={}",
        a_team.size, a_team.level, b_team.size, b_team.level, settings.runs, settings.cap.as_secs(), settings.seed,
    );
    println!();
    print_matrix(&matrix(settings), settings.ledger);
}

/// One pairing's fights, summed: each side's share of wins, of fights run
/// to the cap and the median length, the winners' median health left, and
/// a's edge, all as percentages but the length; and each side's ledger.
pub struct Pairing {
    pub a: EnemyArchetype,
    pub b: EnemyArchetype,
    pub a_wins: f32,
    pub b_wins: f32,
    pub capped: f32,
    pub median: f32,
    pub left: f32,
    pub edge: f32,
    a_ledger: Ledger,
    b_ledger: Ledger,
    runs: f32,
}

/// Prints `rows` as the report does, with each side's ledger when `ledger`.
fn print_matrix(rows: &[Pairing], ledger: bool) {
    println!("{:<22} {:>5} {:>5} {:>5}  {:>6}  {:>5}  {:>5}   {:<30} {:<30}",
        "pairing (a v b)", "a%", "b%", "capped", "median", "left", "edge", "a dealt: auto/abil/refl", "b dealt: auto/abil/refl");
    for row in rows {
        println!("{:<22} {:>5.0} {:>5.0} {:>5.0}  {:>5.0}s  {:>4.0}%  {:>5.0}   {:<30} {:<30}",
            format!("{:?} v {:?}", row.a, row.b), row.a_wins, row.b_wins, row.capped, row.median, row.left, row.edge,
            split(&row.a_ledger, row.runs), split(&row.b_ledger, row.runs));
        if ledger {
            println!("    {:?}: {}", row.a, ledger_line(&row.a_ledger, row.runs));
            println!("    {:?}: {}", row.b, ledger_line(&row.b_ledger, row.runs));
        }
    }
}

/// Fights every pairing of the scenario `runs` times under its tuning and
/// minds, which it makes the process's own for the while.
fn matrix(settings: &Settings) -> Vec<Pairing> {
    set_tuning(settings.tuning);
    set_minds(settings.minds.clone());
    let pairings: Vec<(EnemyArchetype, EnemyArchetype)> = if settings.ordered {
        settings.only.iter().flat_map(|&a| settings.only.iter().map(move |&b| (a, b))).collect()
    } else if settings.mirror {
        settings.only.iter().map(|&a| (a, a)).collect()
    } else {
        settings.only.iter().enumerate()
            .flat_map(|(i, &a)| settings.only[i + 1..].iter().map(move |&b| (a, b)))
            .collect()
    };
    let pairings: Vec<_> = pairings.into_iter().filter(|&(a, b)| settings.focus.is_none_or(|focus| a == focus || b == focus)).collect();
    let workers = if settings.trace > 0 { 1 } else { std::thread::available_parallelism().map_or(1, |n| n.get()) };
    let runs = settings.runs;
    let outcomes = in_parallel(pairings.len() as u32 * runs, workers, |job| {
        let ((a, b), run) = (pairings[(job / runs) as usize], job % runs);
        // Swap ends every run so the spawn layout favours neither archetype
        let (team_a, team_b) = (settings.team_a(a), settings.team_b(b));
        let (west, east, a_side) = if run % 2 == 0 { (team_a, team_b, WEST) } else { (team_b, team_a, EAST) };
        (a_side, fight(west, east, settings, fight_seed(settings.seed, a, b, run)))
    });
    let mut rows = Vec::new();
    for (&(a, b), outcomes) in pairings.iter().zip(outcomes.chunks(runs as usize)) {
        let (mut a_wins, mut b_wins, mut capped) = (0u32, 0u32, 0u32);
        let mut lengths = Vec::new();
        let mut left = Vec::new();
        let mut edge = 0.0;
        let (mut a_ledger, mut b_ledger) = (Ledger::default(), Ledger::default());
        for (a_side, outcome) in outcomes {
            let a_side = *a_side;
            let b_side = if a_side == WEST { EAST } else { WEST };
            match outcome.winner {
                Some(side) if side == a_side => a_wins += 1,
                Some(_) => b_wins += 1,
                None => {}
            }
            if outcome.timed_out {
                capped += 1;
            }
            lengths.push(outcome.length);
            if outcome.winner.is_some() {
                left.push(outcome.left);
            }
            if !outcome.timed_out {
                let share = |side: Side| outcome.shares.get(&side).copied().unwrap_or(0.0);
                edge += share(a_side) - share(b_side);
            }
            let none = Ledger::default();
            a_ledger.merge(outcome.ledgers.get(&a_side).unwrap_or(&none));
            b_ledger.merge(outcome.ledgers.get(&b_side).unwrap_or(&none));
        }
        lengths.sort();
        left.sort_by(f32::total_cmp);
        let median = lengths.get(lengths.len() / 2).map_or(0.0, |d| d.as_secs_f32());
        let median_left = left.get(left.len() / 2).copied().unwrap_or(0.0);
        let runs = runs as f32;
        rows.push(Pairing {
            a, b,
            a_wins: 100.0 * a_wins as f32 / runs,
            b_wins: 100.0 * b_wins as f32 / runs,
            capped: 100.0 * capped as f32 / runs,
            median,
            left: 100.0 * median_left,
            edge: 100.0 * edge / runs,
            a_ledger, b_ledger, runs,
        });
    }
    rows
}

/// The seed of `run` of the pairing `a` v `b` under `seed`, the same
/// whichever pairings the scenario fights beside it.
fn fight_seed(seed: u64, a: EnemyArchetype, b: EnemyArchetype, run: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    (seed, a, b, run).hash(&mut hasher);
    hasher.finish()
}

/// Runs `job` for each of `0..count` on up to `workers` threads, results in
/// order. Each fight is its own app, so fights share nothing but Bevy's
/// global task pools.
fn in_parallel<T: Send>(count: u32, workers: usize, job: impl Fn(u32) -> T + Sync) -> Vec<T> {
    use std::sync::atomic::{AtomicU32, Ordering};
    let next = AtomicU32::new(0);
    let mut results: Vec<(u32, T)> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..workers.min(count as usize)).map(|_| scope.spawn(|| {
            let mut done = Vec::new();
            loop {
                let run = next.fetch_add(1, Ordering::Relaxed);
                if run >= count {
                    break done;
                }
                done.push((run, job(run)));
            }
        })).collect();
        threads.into_iter().flat_map(|thread| thread.join().expect("arena fight panicked")).collect()
    });
    results.sort_by_key(|(run, _)| *run);
    results.into_iter().map(|(_, result)| result).collect()
}

/// A side's damage per fight and its split, as `total (auto/ability/reflect %)`
fn split(ledger: &Ledger, runs: f32) -> String {
    let total = ledger.landed_total();
    if total <= 0.0 {
        return "0".to_owned();
    }
    let (auto, ability, reflect) = ledger.sources();
    format!("{:.0} ({:.0}/{:.0}/{:.0}%)", total / runs, 100.0 * auto / total, 100.0 * ability / total, 100.0 * reflect / total)
}

/// A side's ledger per fight: each ability's uses, the damage it sent and
/// the share of what settled that landed, or a reaction's reflected; then
/// its states as shares of the time it was alive
fn ledger_line(ledger: &Ledger, runs: f32) -> String {
    let mut used: Vec<(AbilityType, u32)> = ledger.used.iter().map(|(&ability, &uses)| (ability, uses)).collect();
    used.sort_by_key(|&(ability, _)| format!("{ability:?}"));
    let abilities: Vec<String> = used.into_iter().map(|(ability, uses)| {
        let of = |damage: &HashMap<Option<AbilityType>, f32>| damage.get(&Some(ability)).copied().unwrap_or(0.0);
        let (sent, landed, settled) = (of(&ledger.sent), of(&ledger.landed), of(&ledger.sent) - of(&ledger.pending));
        let damage = if settled > 0.0 {
            format!(" sent {:.0} landed {:.0}%", sent / runs, 100.0 * landed / settled)
        } else if landed > 0.0 {
            format!(" reflected {:.0}", landed / runs)
        } else {
            String::new()
        };
        format!("{ability:?} {:.1}x{damage}", uses as f32 / runs)
    }).collect();
    let alive = ledger.alive.max(f32::EPSILON);
    let share = |seconds: f32| 100.0 * seconds / alive;
    format!("{} || recovering {:.0}% held {:.0}% slowed {:.0}% beyond reach {:.0}% circling {:.0}% fatigue {:.0}%",
        abilities.join(" | "),
        share(ledger.recovering), share(ledger.held), share(ledger.slowed), share(ledger.beyond_reach), share(ledger.circling), share(ledger.fatigue))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pairing's outcome, every number of it, as bits
    fn fingerprint(rows: &[Pairing]) -> Vec<u32> {
        rows.iter().flat_map(|row| [row.a_wins, row.b_wins, row.capped, row.median, row.left, row.edge,
            row.a_ledger.landed_total(), row.b_ledger.landed_total(), row.a_ledger.alive, row.b_ledger.alive]).map(f32::to_bits).collect()
    }

    #[test]
    fn a_seed_fights_the_same_fights() {
        let args = ["only=berserker,ambusher", "runs=2", "seed=7"].map(str::to_owned);
        let settings = Settings::parse(&args);
        assert_eq!(fingerprint(&matrix(&settings)), fingerprint(&matrix(&settings)));
    }
}
