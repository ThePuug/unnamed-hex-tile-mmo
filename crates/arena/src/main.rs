//! # Balance arena
//!
//! `cargo run --bin arena -- [key=value ...]` sets NPC archetypes
//! against each other on a flat, empty map and reports who wins. Each fight
//! is its own headless app running `CombatPlugin` and `BehaviourPlugin`,
//! the rules the live server runs, stepped on a manual clock so it goes as
//! fast as the CPU allows. No networking, terrain or players.
//!
//! Keys: `level` (10), `size` NPCs per side (1), `skill` its NPCs fight
//! with (`sharp`, `steady`, `sloppy`, or `fastest-slowest/error/misjudge`
//! in milliseconds, a share and milliseconds; sharp), `b_level`, `b_size` and `b_skill` to
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
//! (`bar=parry,feint`); `build` and `b_build` nine comma steps, axis,
//! spectrum and shift of each pair in turn, that each side's fighters hold in
//! place of their archetype's build, at their level (`build=-6,4,4,-3,4,4,-3,4,4`); and any mind
//! setting as `mind.<all|archetype>.<setting>` (`combat::behaviour::mind`), so a
//! search tunes each archetype's fighter the same way.
//!
//! `arena serve` runs one scenario per line of stdin, each line the keys
//! above, and ends each report with a line `end`. `arena tune ...` searches
//! the game's numbers and the NPCs' minds in-process ([`tune`]). `arena
//! curves` prints every archetype's decisions and mind as JSON, the data
//! of the skills reference page ([`curves`]).
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
//! landed, and how many of its uses landed, were blocked, missed or were
//! interrupted; then the share of the fight it spent activating, held, lost
//! to a slow, with its target beyond its reach and standing past its
//! target's forward faces, its mean endurance, and the share it stood
//! priced out: a skill of its bar recharged and costing more than it held,
//! which the gate would refuse for endurance alone.
//!
//! The report ends with its distance ([`Distance`]): how far the matrix
//! stands from a simple, functional fight, as a sum of named parts, each 0
//! at the target. `parity` is the mean over pairings of the edge's size;
//! `length` the mean of the seconds a pairing's median fight falls outside
//! 30 to 45; `capped` the share of fights run to the cap, in points;
//! `skills` the mean over every side of the points by which skills' share
//! of its damage, all but its auto-attacks, falls short of 50; `dead` 10
//! for each skill on an archetype's own bar it uses less than once a fight
//! across its pairings, or a strike landing under a quarter of its uses, a
//! stance judged only over the pairings whose foe can bring about what it
//! takes hold against (`skills::arises`);
//! `answers` the points by which the share of skill strikes blocked, missed
//! or interrupted falls outside 10 to 40.

mod curves;
mod tune;

use std::{collections::HashMap, time::Duration};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use qrz::Qrz;

use common_bevy::{
    components::{
        behaviour::Side,
        entity_type::{decorator::Decorator, EntityType},
        activation::{Activating, Recharges},
        heading::Heading,
        resources::{Endurance, Health, SpawnPoint},
        ActorAttributes,
        status::{Stance, Status},
        target::Target,
        AttackRange, Loc,
    },
    message::{AbilityType, Do, Event, Outcome as Ended, SkillKind},
    plugins::nntree::NNTreePlugin,
    resources::map::Map,
    systems::targeting,
    archetype::EnemyArchetype,
    tuning::Tuning,
};

use combat::{
    actor,
    behaviour::{mind::Minds, perception::Skill, skills::arises, Bar, Decisions},
    dice::Dice,
    engagement::{engaging_at, spawn_engagement, STAGE_GAP},
    BehaviourPlugin, CombatPlugin,
};

/// One simulated frame. FixedUpdate's 125ms tick runs every second frame.
const STEP: Duration = Duration::from_micros(62_500);

/// Flat tiles laid out round the origin; wide enough that a fleeing fighter
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
    /// The steps its fighters hold in place of their archetype's build
    build: Option<[i8; 9]>,
}

struct Settings {
    level: u8,
    size: u8,
    b_level: Option<u8>,
    b_size: Option<u8>,
    skill: Skill,
    b_skill: Option<Skill>,
    build: Option<[i8; 9]>,
    b_build: Option<[i8; 9]>,
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
        let mut settings = Settings { level: 10, size: 1, b_level: None, b_size: None, skill: Skill::SHARP, b_skill: None, build: None, b_build: None, bar: None, mirror: false, ordered: false, runs: 20, cap: Duration::from_secs(120), only: EnemyArchetype::ALL.to_vec(), focus: None, trace: 0, ledger: false, seed: rand::random(), tuning: Tuning::default(), minds: Minds::tuned() };
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
                "build" => settings.build = Some(build_named(value)),
                "b_build" => settings.b_build = Some(build_named(value)),
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
        Team { archetype, level: self.level, size: self.size, skill: self.skill, build: self.build }
    }

    fn team_b(&self, archetype: EnemyArchetype) -> Team {
        Team { archetype, level: self.b_level.unwrap_or(self.level), size: self.b_size.unwrap_or(self.size), skill: self.b_skill.unwrap_or(self.skill), build: self.b_build }
    }
}

/// Nine comma steps, axis, spectrum and shift of each pair in turn
fn build_named(value: &str) -> [i8; 9] {
    let steps: Vec<i8> = value.split(',').map(|step| step.parse().expect("a build is nine whole numbers")).collect();
    steps.try_into().unwrap_or_else(|_| panic!("a build is nine steps, not {value}"))
}

fn ability_named(name: &str) -> AbilityType {
    AbilityType::ALL.into_iter()
        .find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no skill {name}"))
}

fn archetype_named(name: &str) -> EnemyArchetype {
    EnemyArchetype::ALL.into_iter()
        .find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no archetype {name}"))
}

/// What one side did over its fights, keyed by the ability that did it.
#[derive(Clone, Default)]
struct Ledger {
    used: HashMap<AbilityType, u32>,
    /// How each of its attacks and activations ended: landed, blocked,
    /// missed or interrupted, a Counter's return among the landed
    ended: HashMap<(AbilityType, Ended), u32>,
    /// Damage of its that landed, a Counter's return included
    landed: HashMap<AbilityType, f32>,
    /// Seconds alive, and of those activating a skill, held, lost to a slow
    /// (a slow to 85% for a second losing 0.15 s), with its target beyond
    /// its reach and standing past its target's forward faces; and the
    /// share of its endurance it held summed over them
    alive: f32,
    activating: f32,
    held: f32,
    lost_to_slow: f32,
    beyond_reach: f32,
    past_faces: f32,
    endurance: f32,
    /// Seconds a skill of its bar stood recharged and cost more than it held
    priced_out: f32,
}

impl Ledger {
    fn merge(&mut self, other: &Ledger) {
        for (ability, n) in &other.used {
            *self.used.entry(*ability).or_default() += n;
        }
        for (key, n) in &other.ended {
            *self.ended.entry(*key).or_default() += n;
        }
        for (ability, damage) in &other.landed {
            *self.landed.entry(*ability).or_default() += damage;
        }
        self.alive += other.alive;
        self.activating += other.activating;
        self.held += other.held;
        self.lost_to_slow += other.lost_to_slow;
        self.beyond_reach += other.beyond_reach;
        self.past_faces += other.past_faces;
        self.endurance += other.endurance;
        self.priced_out += other.priced_out;
    }

    fn landed_total(&self) -> f32 {
        // Summed in one order: a map iterates in its own, and float sums
        // in two orders round apart
        let mut landed: Vec<f32> = self.landed.values().copied().collect();
        landed.sort_by(f32::total_cmp);
        landed.iter().sum()
    }

    /// Landed damage as auto-attacks, skills and reflections
    fn sources(&self) -> (f32, f32, f32) {
        let of = |ability: AbilityType| self.landed.get(&ability).copied().unwrap_or(0.0);
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

/// Counts each skill an actor uses as its activation starts, each
/// auto-attack as it resolves, how every attack and activation ended, and
/// the damage each landed. A rune's pulses are on no side's ledger.
fn tally_sent(mut reader: MessageReader<Do>, mut tally: ResMut<Tally>) {
    for message in reader.read() {
        match &message.event {
            Event::Activate { ent, activation } => if let Some(ledger) = tally.of(*ent) {
                *ledger.used.entry(activation.ability).or_default() += 1;
            },
            Event::Resolved { ent, ability, damage, outcome, .. } => if let Some(ledger) = tally.of(*ent) {
                if *ability == AbilityType::AutoAttack {
                    *ledger.used.entry(*ability).or_default() += 1;
                }
                *ledger.ended.entry((*ability, *outcome)).or_default() += 1;
                if *outcome == Ended::Landed {
                    *ledger.landed.entry(*ability).or_default() += damage;
                }
            },
            _ => {}
        }
    }
}

/// Adds a frame of `step` seconds to each living actor's side: whether it
/// is activating, held, what its slow takes, has its target beyond its
/// reach or stands past its target's forward faces, and the share of its
/// endurance it holds.
fn tally_states(world: &mut World, step: f32) {
    let mut locs = world.query::<(Entity, &Loc)>();
    let locs: HashMap<Entity, Loc> = locs.iter(world).map(|(ent, loc)| (ent, *loc)).collect();
    let mut headings = world.query::<(Entity, &Heading)>();
    let headings: HashMap<Entity, Heading> = headings.iter(world).map(|(ent, heading)| (ent, *heading)).collect();
    let tuning = *world.resource::<Tuning>();
    let now = world.resource::<combat::RunTime>().now(world.resource::<Time>());
    let mut actors = world.query::<(Entity, &Health, &Loc, &AttackRange, Option<&Target>, Option<&Activating>, Option<&Status>, Option<&Endurance>, Option<(&Bar, &Recharges)>)>();
    let frames: Vec<_> = actors.iter(world).filter(|(_, health, ..)| health.state > 0.0).map(|(ent, _, loc, range, target, activating, status, endurance, skills)| {
        let priced_out = endurance.zip(skills).is_some_and(|(endurance, (bar, recharges))| {
            bar.0.iter().any(|&ability| recharges.is_ready(ability, now) && tuning.cost(ability) > endurance.state)
        });
        let foe = target.and_then(|target| target.entity);
        let beyond = foe.and_then(|foe| locs.get(&foe)).is_some_and(|foe| loc.flat_distance(foe) > range.0);
        let past_faces = foe.and_then(|foe| Some((locs.get(&foe)?, headings.get(&foe))))
            .is_some_and(|(foe_loc, heading)| targeting::flanked(heading, foe_loc, loc));
        let slow = status.and_then(|status| status.slow).filter(|slow| slow.remaining > 0.0).map_or(0.0, |slow| 1.0 - slow.pace);
        let pool = endurance.map_or(1.0, |endurance| endurance.state / endurance.max.max(f32::EPSILON));
        (ent, activating.is_some_and(|activating| activating.0.is_some()), Status::holds(status), slow, beyond, past_faces, pool, priced_out)
    }).collect();
    let mut tally = world.resource_mut::<Tally>();
    for (ent, activating, held, slow, beyond, past_faces, pool, priced_out) in frames {
        let Some(ledger) = tally.of(ent) else { continue };
        let during = |state: bool| if state { step } else { 0.0 };
        ledger.alive += step;
        ledger.activating += during(activating);
        ledger.held += during(held);
        ledger.lost_to_slow += slow * step;
        ledger.beyond_reach += during(beyond);
        ledger.past_faces += during(past_faces);
        ledger.endurance += pool * step;
        ledger.priced_out += during(priced_out);
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
    let map = Map::new(qrz::Map::new(common::grid::HEX_RADIUS, common::grid::RISE));
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
    app.insert_resource(settings.tuning);
    app.insert_resource(settings.minds.clone());
    if settings.trace > 3 {
        app.init_resource::<Decisions>();
    }
    app.add_systems(Update, (actor::update, tally_sent));
    app.add_systems(PostUpdate, actor::cleanup_despawned);
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
            spawn_engagement(&settings.tuning, jitter(at, foes, &dice), team.archetype, side, team.level, combat::behaviour::ACQUISITION_RANGE, team.size, |_, _| 0, &mut commands, &time);
        }
    }
    world.flush();
    let mut skills = world.query::<(&Side, &mut Skill)>();
    for (side, mut skill) in skills.iter_mut(world) {
        *skill = if *side == WEST { west.skill } else { east.skill };
    }
    let mut builds = world.query::<(&Side, &mut ActorAttributes, &mut Health, &mut Endurance)>();
    for (side, mut attrs, mut health, mut endurance) in builds.iter_mut(world) {
        let team = if *side == WEST { west } else { east };
        let Some([a, b, c, d, e, f, g, h, i]) = team.build else { continue };
        *attrs = ActorAttributes::new(a, b, c, d, e, f, g, h, i).at_level(team.level as u32);
        *health = Health::full(attrs.max_health(&settings.tuning));
        *endurance = Endurance::full(attrs.max_endurance(&settings.tuning));
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
    let mut fighters = world.query::<(Entity, &Side, &common_bevy::components::Loc, &mut Heading)>();
    for (ent, side, loc, mut heading) in fighters.iter_mut(world) {
        let foes = if *side == WEST { east_at } else { west_at };
        let toward = Heading::between(**loc, foes).unwrap_or(*heading);
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
    let tally = std::mem::take(&mut *app.world_mut().resource_mut::<Tally>());
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
    use common_bevy::components::{Loc, resources::CombatState, returning::Returning, status::Status, target::Target};
    use combat::engagement::{AssignedHex, HexAssignment};
    let mut facings = world.query::<(Entity, &Loc, &Heading)>();
    let facing: HashMap<Entity, (Loc, Heading)> = facings.iter(world).map(|(e, loc, heading)| (e, (*loc, *heading))).collect();
    let mut actors = world.query::<(Entity, &Side, &Loc, &Health, &CombatState, Option<&Target>, Option<&Returning>, Option<&AssignedHex>, Option<&Status>, &common_bevy::components::position::Position)>();
    let lines: Vec<_> = actors.iter(world).map(|(e, side, loc, hp, combat, target, returning, assigned, status, pos)| {
        // Whether it stands past its target's forward faces, as a strike it lands would count a flank
        let flanking = target.and_then(|t| t.entity).and_then(|t| facing.get(&t)).is_some_and(|(at, heading)| targeting::flanked(Some(heading), at, loc));
        format!("{}#{} {:?} face {:?}{} pos {:?}+({:.2},{:.2}) hp {:.0}{}{}{}{} ->{:?}", side.0, e.index(), (loc.q, loc.r, loc.z),
            facing.get(&e).map(|(_, heading)| *heading), if flanking { " FLANKING" } else { "" },
            (pos.tile.q, pos.tile.r), pos.offset.x, pos.offset.z, hp.state,
            if combat.in_combat { " fighting" } else { "" },
            if returning.is_some() { " RETURNING" } else { "" },
            assigned.map_or(String::new(), |a| format!(" hex {:?}", (a.0.q, a.0.r, a.0.z))),
            if Status::holds(status) { " HELD" } else { "" },
            target.and_then(|t| t.entity).map(|t| t.index()))
    }).collect();
    let mut engagements = world.query::<&HexAssignment>();
    let assigning: Vec<_> = engagements.iter(world).map(|h| format!("{:?}@{:?}", h.target_player.map(|e| e.index()), h.last_player_tile.map(|t| (t.q, t.r)))).collect();
    println!("    t={:>5.1}s {} || engagements {}", elapsed.as_secs_f32(), lines.join(" | "), assigning.join(" "));
}

/// Runs the arena: `serve` answers scenarios from stdin, `tune` searches,
/// `curves` prints the minds, anything else is one scenario's keys.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(),
        Some("tune") => tune::run(&args[1..]),
        Some("curves") => curves::print(),
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
    println!("{}", Distance::of(rows));
}

/// How far a matrix stands from the target fight, part by part (the module
/// doc says what each measures): every part 0 or more, 0 at the target.
struct Distance([(&'static str, f32); 6]);

impl Distance {
    fn of(rows: &[Pairing]) -> Self {
        let mean = |values: Vec<f32>| values.iter().sum::<f32>() / values.len().max(1) as f32;
        let outside = |value: f32, low: f32, high: f32| (low - value).max(value - high).max(0.0);
        let sides = || rows.iter().flat_map(|row| [(row.a, &row.a_ledger, row.runs), (row.b, &row.b_ledger, row.runs)]);

        let skills = mean(sides().map(|(_, ledger, _)| {
            let (auto, total) = (ledger.sources().0, ledger.landed_total());
            if total > 0.0 { (50.0 - 100.0 * (total - auto) / total).max(0.0) } else { 50.0 }
        }).collect());

        // Each skill of each archetype's bar across the matrix: its uses,
        // how many landed, and the fights it was judged over, a stance's
        // only those against a foe that can bring its condition about
        let ended = |ledger: &Ledger, ability: AbilityType, outcome: Ended| ledger.ended.get(&(ability, outcome)).copied().unwrap_or(0) as f32;
        let mut whole: HashMap<(EnemyArchetype, AbilityType), (f32, f32, f32)> = HashMap::new();
        for row in rows {
            for (archetype, foe, ledger) in [(row.a, row.b, &row.a_ledger), (row.b, row.a, &row.b_ledger)] {
                for ability in archetype.bar() {
                    if Stance::of(ability).is_some_and(|stance| !arises(stance, &foe.bar())) {
                        continue;
                    }
                    let (uses, landed, fights) = whole.entry((archetype, ability)).or_default();
                    *uses += ledger.used.get(&ability).copied().unwrap_or(0) as f32;
                    *landed += ended(ledger, ability, Ended::Landed);
                    *fights += row.runs;
                }
            }
        }
        let dead = whole.iter()
            .filter(|&(&(_, ability), &(uses, landed, fights))| {
                let strike = ability.kind() == SkillKind::Strike;
                uses < fights || strike && landed < 0.25 * uses
            })
            .count();

        let (mut landed, mut answered) = (0.0, 0.0);
        for (_, ledger, _) in sides() {
            for ability in AbilityType::ALL.into_iter().filter(|&ability| ability != AbilityType::AutoAttack && ability.kind() == SkillKind::Strike) {
                landed += ended(ledger, ability, Ended::Landed);
                answered += [Ended::Blocked, Ended::Missed, Ended::Interrupted].into_iter().map(|outcome| ended(ledger, ability, outcome)).sum::<f32>();
            }
        }
        let answers = if landed + answered > 0.0 { outside(100.0 * answered / (landed + answered), 10.0, 40.0) } else { 10.0 };

        Distance([
            ("parity", mean(rows.iter().map(|row| row.edge.abs()).collect())),
            ("length", mean(rows.iter().map(|row| outside(row.median, 30.0, 45.0)).collect())),
            ("capped", mean(rows.iter().map(|row| row.capped).collect())),
            ("skills", skills),
            ("dead", 10.0 * dead as f32),
            ("answers", answers),
        ])
    }

    fn total(&self) -> f32 {
        self.0.iter().map(|&(_, part)| part).sum()
    }
}

impl std::fmt::Display for Distance {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let parts: Vec<String> = self.0.iter().map(|(name, part)| format!("{name} {part:.1}")).collect();
        write!(f, "distance {:.1} = {}", self.total(), parts.join(" + "))
    }
}

/// Fights every pairing of the scenario `runs` times under its tuning and
/// minds.
fn matrix(settings: &Settings) -> Vec<Pairing> {
    matrices(std::slice::from_ref(settings)).remove(0)
}

/// Each scenario's `matrix`, their fights all in one pool of workers, so
/// none waits on another's slowest; each fight plays its scenario's tuning
/// and minds.
fn matrices(scenarios: &[Settings]) -> Vec<Vec<Pairing>> {
    // Each scenario's pairings, and its fights' place among all of them
    let pairings: Vec<Vec<(EnemyArchetype, EnemyArchetype)>> = scenarios.iter().map(pairings).collect();
    let jobs: Vec<(usize, (EnemyArchetype, EnemyArchetype), u32)> = scenarios.iter().zip(&pairings).enumerate()
        .flat_map(|(i, (settings, pairings))| pairings.iter().flat_map(move |&pairing| (0..settings.runs).map(move |run| (i, pairing, run))))
        .collect();
    let workers = if scenarios.iter().any(|settings| settings.trace > 0) { 1 } else { std::thread::available_parallelism().map_or(1, |n| n.get()) };
    let mut outcomes = in_parallel(jobs.len() as u32, workers, |job| {
        let (i, (a, b), run) = jobs[job as usize];
        let settings = &scenarios[i];
        // Swap ends every run so the spawn layout favours neither archetype
        let (team_a, team_b) = (settings.team_a(a), settings.team_b(b));
        let (west, east, a_side) = if run % 2 == 0 { (team_a, team_b, WEST) } else { (team_b, team_a, EAST) };
        (a_side, fight(west, east, settings, fight_seed(settings.seed, a, b, run)))
    }).into_iter();
    scenarios.iter().zip(&pairings).map(|(settings, pairings)| {
        let outcomes: Vec<_> = outcomes.by_ref().take(pairings.len() * settings.runs as usize).collect();
        rows(pairings, &outcomes, settings.runs)
    }).collect()
}

/// The pairings a scenario fights
fn pairings(settings: &Settings) -> Vec<(EnemyArchetype, EnemyArchetype)> {
    let pairings: Vec<(EnemyArchetype, EnemyArchetype)> = if settings.ordered {
        settings.only.iter().flat_map(|&a| settings.only.iter().map(move |&b| (a, b))).collect()
    } else if settings.mirror {
        settings.only.iter().map(|&a| (a, a)).collect()
    } else {
        settings.only.iter().enumerate()
            .flat_map(|(i, &a)| settings.only[i + 1..].iter().map(move |&b| (a, b)))
            .collect()
    };
    pairings.into_iter().filter(|&(a, b)| settings.focus.is_none_or(|focus| a == focus || b == focus)).collect()
}

/// Each pairing's `runs` outcomes, in order, summed into its row
fn rows(pairings: &[(EnemyArchetype, EnemyArchetype)], outcomes: &[(Side, Outcome)], runs: u32) -> Vec<Pairing> {
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

/// A side's ledger per fight: each ability's uses, the damage it landed,
/// and how many of its uses landed, were blocked, missed and were
/// interrupted; then its states as shares of the time it was alive
fn ledger_line(ledger: &Ledger, runs: f32) -> String {
    let mut used: Vec<(AbilityType, u32)> = ledger.used.iter().map(|(&ability, &uses)| (ability, uses)).collect();
    used.extend(ledger.ended.keys().map(|&(ability, _)| ability).filter(|ability| !ledger.used.contains_key(ability)).map(|ability| (ability, 0)));
    used.sort_by_key(|&(ability, _)| format!("{ability:?}"));
    used.dedup();
    let abilities: Vec<String> = used.into_iter().map(|(ability, uses)| {
        let ended = |outcome: Ended| ledger.ended.get(&(ability, outcome)).copied().unwrap_or(0) as f32 / runs;
        let counts: Vec<String> = [("landed", Ended::Landed), ("blocked", Ended::Blocked), ("missed", Ended::Missed), ("interrupted", Ended::Interrupted)]
            .into_iter()
            .filter(|&(_, outcome)| ended(outcome) > 0.0)
            .map(|(name, outcome)| format!("{name} {:.1}", ended(outcome)))
            .collect();
        let dealt = ledger.landed.get(&ability).copied().unwrap_or(0.0);
        let damage = if dealt > 0.0 { format!(" dealt {:.0}", dealt / runs) } else { String::new() };
        let counts = if counts.is_empty() { String::new() } else { format!(" ({})", counts.join(", ")) };
        format!("{ability:?} {:.1}x{damage}{counts}", uses as f32 / runs)
    }).collect();
    let alive = ledger.alive.max(f32::EPSILON);
    let share = |seconds: f32| 100.0 * seconds / alive;
    format!("{} || activating {:.0}% held {:.0}% lost to slow {:.0}% beyond reach {:.0}% past faces {:.0}% endurance {:.0}% priced out {:.0}%",
        abilities.join(" | "),
        share(ledger.activating), share(ledger.held), share(ledger.lost_to_slow), share(ledger.beyond_reach), share(ledger.past_faces), share(ledger.endurance), share(ledger.priced_out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pairing's outcome, every number of it, as bits
    fn fingerprint(rows: &[Pairing]) -> Vec<u32> {
        rows.iter().flat_map(|row| [row.a_wins, row.b_wins, row.capped, row.median, row.left, row.edge,
            row.a_ledger.landed_total(), row.b_ledger.landed_total(), row.a_ledger.alive, row.b_ledger.alive]).map(f32::to_bits).collect()
    }

    /// A Berserker v Defender pairing at the target on every part: even,
    /// 40 s, every skill of both bars used and landed, a fifth of the
    /// strikes answered, half the damage from skills
    fn on_target() -> Pairing {
        let side = |archetype: EnemyArchetype| {
            let mut ledger = Ledger::default();
            ledger.landed.insert(AbilityType::AutoAttack, 500.0);
            for ability in archetype.bar() {
                ledger.used.insert(ability, 100);
                ledger.ended.insert((ability, Ended::Landed), 80);
                ledger.ended.insert((ability, Ended::Blocked), 20);
                ledger.landed.insert(ability, 512.0 / archetype.bar().len() as f32);
            }
            ledger
        };
        let (a, b) = (EnemyArchetype::Berserker, EnemyArchetype::Defender);
        Pairing { a, b, a_wins: 50.0, b_wins: 50.0, capped: 0.0, median: 40.0, left: 20.0, edge: 0.0, a_ledger: side(a), b_ledger: side(b), runs: 10.0 }
    }

    #[test]
    fn a_matrix_worse_on_one_part_stands_further_by_that_part() {
        assert_eq!(Distance::of(&[on_target()]).total(), 0.0, "the target stands nowhere from itself");
        let worse: [(&str, fn(&mut Pairing)); 8] = [
            ("parity", |row| row.edge = -30.0),
            ("length", |row| row.median = 80.0),
            ("length", |row| row.median = 12.0),
            ("capped", |row| row.capped = 25.0),
            ("skills", |row| { row.a_ledger.landed.insert(AbilityType::AutoAttack, 5000.0); }),
            ("dead", |row| { row.a_ledger.used.insert(AbilityType::Clash, 3); }),
            ("dead", |row| { row.a_ledger.ended.insert((AbilityType::Frenzy, Ended::Landed), 10); }),
            ("answers", |row| { row.b_ledger.ended.insert((AbilityType::Defy, Ended::Missed), 400); }),
        ];
        for (name, worsen) in worse {
            let mut row = on_target();
            worsen(&mut row);
            let distance = Distance::of(&[row]);
            let moved: Vec<&str> = distance.0.iter().filter(|(_, part)| *part > 0.0).map(|&(part, _)| part).collect();
            assert_eq!(moved, [name], "{distance}");
        }
    }

    #[test]
    fn a_seed_fights_the_same_fights() {
        let args = ["only=berserker,ambusher", "runs=2", "seed=7"].map(str::to_owned);
        let settings = Settings::parse(&args);
        assert_eq!(fingerprint(&matrix(&settings)), fingerprint(&matrix(&settings)));
    }
}
