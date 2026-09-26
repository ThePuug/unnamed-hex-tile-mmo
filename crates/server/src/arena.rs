//! # Balance arena
//!
//! `cargo run --bin server -- arena [key=value ...]` sets NPC archetypes
//! against each other on a flat, empty map and reports who wins. Each fight
//! is its own headless app running `CombatPlugin` and `BehaviourPlugin`,
//! the rules the live server runs, stepped on a manual clock so it goes as
//! fast as the CPU allows. No networking, terrain or players.
//!
//! Keys: `level` (10), `size` NPCs per side (1), `b_level` and `b_size` to
//! set the second archetype's side apart (the same by default), `runs` per
//! matchup (20), `cap` seconds after which the side with more health left wins
//! (300), `only` a comma
//! list of archetypes to restrict the matchups to, `mirror=1` to fight each
//! archetype against itself instead of the others, `ordered=1` to fight every
//! ordered pair, mirrors included, `trace` to print every
//! fight (1), with a timeline every 5s (2), or every half second for its
//! first 12s (3).
//!
//! Every pairing fights `runs` times, the two swapping ends each run so
//! neither side's spawn decides it. The report
//! gives each pairing's win split, median fight length, the winners' health
//! left, and where each side's damage came from: auto-attacks, signature
//! abilities, or reflections.

use std::{collections::HashMap, time::Duration};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use qrz::Qrz;

use common_bevy::{
    components::{
        behaviour::Side,
        entity_type::{decorator::Decorator, EntityType},
        resources::{Health, SpawnPoint},
    },
    message::{AbilityType, Event, Try},
    plugins::nntree::NNTreePlugin,
    resources::map::Map,
    spatial_difficulty::EnemyArchetype,
};

use crate::{
    plugins::{behaviour::BehaviourPlugin, combat::CombatPlugin},
    systems::{actor, engagement_spawner::spawn_engagement, renet},
};

/// One simulated frame. FixedUpdate's 125ms tick runs every second frame.
const STEP: Duration = Duration::from_micros(62_500);

/// Flat tiles laid out round the origin; wide enough that a fleeing Kiter
/// reaches its leash before the edge.
const ARENA_RADIUS: i32 = 45;

/// Each side's den stands this far either side of the origin: inside every
/// archetype's acquisition range of the other.
const DEN_OFFSET: i32 = 6;

const ARCHETYPES: [EnemyArchetype; 4] = [
    EnemyArchetype::Berserker,
    EnemyArchetype::Juggernaut,
    EnemyArchetype::Kiter,
    EnemyArchetype::Defender,
];

/// One side of a fight: `size` NPCs of `archetype` at `level`.
#[derive(Clone, Copy)]
struct Team {
    archetype: EnemyArchetype,
    level: u8,
    size: u8,
}

struct Settings {
    level: u8,
    size: u8,
    b_level: Option<u8>,
    b_size: Option<u8>,
    mirror: bool,
    ordered: bool,
    runs: u32,
    cap: Duration,
    only: Vec<EnemyArchetype>,
    trace: u8,
}

impl Settings {
    fn parse(args: &[String]) -> Self {
        let mut settings = Settings { level: 10, size: 1, b_level: None, b_size: None, mirror: false, ordered: false, runs: 20, cap: Duration::from_secs(300), only: ARCHETYPES.to_vec(), trace: 0 };
        for arg in args {
            let (key, value) = arg.split_once('=').unwrap_or_else(|| panic!("arena takes key=value, not {arg}"));
            match key {
                "level" => settings.level = value.parse().expect("level is a whole number"),
                "size" => settings.size = value.parse().expect("size is a whole number"),
                "b_level" => settings.b_level = Some(value.parse().expect("b_level is a whole number")),
                "b_size" => settings.b_size = Some(value.parse().expect("b_size is a whole number")),
                "mirror" => settings.mirror = value == "1",
                "ordered" => settings.ordered = value == "1",
                "runs" => settings.runs = value.parse().expect("runs is a whole number"),
                "cap" => settings.cap = Duration::from_secs(value.parse().expect("cap is whole seconds")),
                "only" => settings.only = value.split(',').map(archetype_named).collect(),
                "trace" => settings.trace = value.parse().expect("trace is 0 to 3"),
                _ => panic!("arena has no key {key}"),
            }
        }
        settings
    }

    fn team_a(&self, archetype: EnemyArchetype) -> Team {
        Team { archetype, level: self.level, size: self.size }
    }

    fn team_b(&self, archetype: EnemyArchetype) -> Team {
        Team { archetype, level: self.b_level.unwrap_or(self.level), size: self.b_size.unwrap_or(self.size) }
    }
}

fn archetype_named(name: &str) -> EnemyArchetype {
    ARCHETYPES.into_iter()
        .find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no archetype {name}"))
}

/// Where a side's damage came from, before mitigation.
#[derive(Clone, Copy, Default)]
struct Sources {
    auto: f32,
    ability: f32,
    reflect: f32,
}

impl Sources {
    fn add(&mut self, ability: Option<AbilityType>, damage: f32) {
        match ability {
            Some(AbilityType::AutoAttack) => self.auto += damage,
            Some(AbilityType::Counter) | Some(AbilityType::Kick) => self.reflect += damage,
            _ => self.ability += damage,
        }
    }

    fn total(&self) -> f32 {
        self.auto + self.ability + self.reflect
    }

    fn merge(&mut self, other: Sources) {
        self.auto += other.auto;
        self.ability += other.ability;
        self.reflect += other.reflect;
    }
}

/// The two sides of a fight, the damage each has dealt and the abilities
/// each has used.
#[derive(Resource, Default)]
struct Tally {
    sides: HashMap<Entity, Side>,
    dealt: HashMap<Side, Sources>,
    used: HashMap<(Side, AbilityType), u32>,
}

/// Counts each ability an actor asks to use, auto-attacks included.
fn tally_used(mut reader: MessageReader<Try>, mut tally: ResMut<Tally>) {
    for message in reader.read() {
        let Try { event: Event::UseAbility { ent, ability, .. } } = message else { continue };
        let Some(&side) = tally.sides.get(ent) else { continue };
        *tally.used.entry((side, *ability)).or_default() += 1;
    }
}

/// Counts each resolved threat against the side of the actor that sent it.
fn tally_resolved(trigger: On<Try>, mut tally: ResMut<Tally>) {
    let Try { event: Event::ResolveThreat { threat, .. } } = trigger.event() else { return };
    let Some(&side) = tally.sides.get(&threat.source) else { return };
    tally.dealt.entry(side).or_default().add(threat.ability, threat.damage);
}

struct Outcome {
    winner: Option<Side>,
    length: Duration,
    /// The winners' remaining health as a fraction of their total
    left: f32,
    /// Decided on health left at the cap, not by a side dying
    timed_out: bool,
    dealt: HashMap<Side, Sources>,
}

const WEST: Side = Side(1);
const EAST: Side = Side(2);

fn flat_map() -> Map {
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

/// Fights `west` against `east` until one side is dead or `cap` passes.
fn fight(west: Team, east: Team, settings: &Settings) -> Outcome {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, NNTreePlugin, BehaviourPlugin, CombatPlugin));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    app.insert_resource(Time::<Fixed>::from_seconds(0.125));
    app.insert_resource(flat_map());
    app.insert_resource(SpawnPoint(Qrz { q: 0, r: 0, z: 1 }));
    app.init_resource::<Tally>();
    app.add_systems(Update, (actor::update, tally_used));
    app.add_systems(PostUpdate, renet::cleanup_despawned);
    app.add_observer(tally_resolved);
    app.finish();
    app.cleanup();

    let world = app.world_mut();
    let time = world.resource::<Time>().clone();
    {
        let mut commands = world.commands();
        for (team, side, q) in [(west, WEST, -DEN_OFFSET), (east, EAST, DEN_OFFSET)] {
            spawn_engagement(Qrz { q, r: 0, z: 1 }, team.archetype, side, team.level, team.size, |_, _| 0, &mut commands, &time);
        }
    }
    world.flush();
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
            // At the cap the side with more of its health left wins on points
            (Some(&(west, _)), Some(&(east, _))) if elapsed >= settings.cap => {
                timed_out = true;
                let (west, east) = (west / full[&WEST], east / full[&EAST]);
                break if west >= east { (Some(WEST), west) } else { (Some(EAST), east) };
            }
            _ => {}
        }
    };
    let tally = std::mem::take(&mut *app.world_mut().resource_mut::<Tally>());
    if settings.trace > 0 {
        let used = |side: Side| {
            let mut used: Vec<_> = tally.used.iter().filter(|((s, _), _)| *s == side).map(|((_, a), n)| format!("{a:?} {n}")).collect();
            used.sort();
            used.join(", ")
        };
        let hp: Vec<_> = health.iter(app.world_mut()).map(|(s, h)| format!("{}:{:.0}/{:.0}", s.0, h.state, h.max)).collect();
        println!("  {}x{:?}@{} (1) v {}x{:?}@{} (2): winner {:?} after {:.1}s, hp [{}]; 1 used [{}] dealt {:.0}; 2 used [{}] dealt {:.0}",
            west.size, west.archetype, west.level, east.size, east.archetype, east.level,
            winner.map(|s| s.0), elapsed.as_secs_f32(), hp.join(" "),
            used(WEST), tally.dealt.get(&WEST).map_or(0.0, Sources::total),
            used(EAST), tally.dealt.get(&EAST).map_or(0.0, Sources::total));
    }
    Outcome { winner, length: elapsed, left, timed_out, dealt: tally.dealt }
}

/// Prints where every actor stands and what it is doing.
fn timeline(world: &mut World, elapsed: Duration) {
    use common_bevy::components::{Loc, hex_assignment::AssignedHex, resources::CombatState, returning::Returning, stagger::Stagger, target::Target};
    let mut actors = world.query::<(Entity, &Side, &Loc, &Health, &CombatState, Option<&Target>, Option<&Returning>, Option<&AssignedHex>, Option<&Stagger>, &common_bevy::components::position::Position)>();
    let lines: Vec<_> = actors.iter(world).map(|(e, side, loc, hp, combat, target, returning, assigned, stagger, pos)| {
        format!("{}#{} {:?} pos {:?}+({:.2},{:.2}) hp {:.0}{}{}{}{} ->{:?}", side.0, e.index(), (loc.q, loc.r, loc.z), (pos.tile.q, pos.tile.r), pos.offset.x, pos.offset.z, hp.state,
            if combat.in_combat { " fighting" } else { "" },
            if returning.is_some() { " RETURNING" } else { "" },
            assigned.map_or(String::new(), |a| format!(" hex {:?}", (a.0.q, a.0.r, a.0.z))),
            if stagger.is_some() { " STAGGERED" } else { "" },
            target.and_then(|t| t.entity).map(|t| t.index()))
    }).collect();
    let mut engagements = world.query::<&common_bevy::components::hex_assignment::HexAssignment>();
    let assigning: Vec<_> = engagements.iter(world).map(|h| format!("{:?}@{:?}", h.target_player.map(|e| e.index()), h.last_player_tile.map(|t| (t.q, t.r)))).collect();
    println!("    t={:>5.1}s {} || engagements {}", elapsed.as_secs_f32(), lines.join(" | "), assigning.join(" "));
}

/// Runs every pairing and prints the report.
pub fn run(args: &[String]) {
    let settings = Settings::parse(args);
    let (a_team, b_team) = (settings.team_a(EnemyArchetype::Berserker), settings.team_b(EnemyArchetype::Berserker));
    println!(
        "arena: a is {} at level {}, b is {} at level {}; {} runs per pairing, decided on health after {}s",
        a_team.size, a_team.level, b_team.size, b_team.level, settings.runs, settings.cap.as_secs(),
    );
    println!();
    println!("{:<22} {:>5} {:>5} {:>5}  {:>6}  {:>5}   {:<30} {:<30}",
        "pairing (a v b)", "a%", "b%", "capped", "median", "left", "a dealt: auto/abil/refl", "b dealt: auto/abil/refl");

    let pairings: Vec<(EnemyArchetype, EnemyArchetype)> = if settings.ordered {
        settings.only.iter().flat_map(|&a| settings.only.iter().map(move |&b| (a, b))).collect()
    } else if settings.mirror {
        settings.only.iter().map(|&a| (a, a)).collect()
    } else {
        settings.only.iter().enumerate()
            .flat_map(|(i, &a)| settings.only[i + 1..].iter().map(move |&b| (a, b)))
            .collect()
    };
    for (a, b) in pairings {
        {
            let (mut a_wins, mut b_wins, mut capped) = (0u32, 0u32, 0u32);
            let mut lengths = Vec::new();
            let mut left = Vec::new();
            let (mut a_dealt, mut b_dealt) = (Sources::default(), Sources::default());
            for run in 0..settings.runs {
                // Swap ends every run so the spawn layout favours neither archetype
                let (team_a, team_b) = (settings.team_a(a), settings.team_b(b));
                let (west, east, a_side) = if run % 2 == 0 { (team_a, team_b, WEST) } else { (team_b, team_a, EAST) };
                let b_side = if a_side == WEST { EAST } else { WEST };
                let outcome = fight(west, east, &settings);
                match outcome.winner {
                    Some(side) if side == a_side => a_wins += 1,
                    Some(_) => b_wins += 1,
                    None => {}
                }
                if outcome.timed_out {
                    capped += 1;
                }
                if outcome.winner.is_some() {
                    lengths.push(outcome.length);
                    left.push(outcome.left);
                }
                a_dealt.merge(outcome.dealt.get(&a_side).copied().unwrap_or_default());
                b_dealt.merge(outcome.dealt.get(&b_side).copied().unwrap_or_default());
            }
            lengths.sort();
            left.sort_by(f32::total_cmp);
            let median = lengths.get(lengths.len() / 2).map_or(0.0, |d| d.as_secs_f32());
            let median_left = left.get(left.len() / 2).copied().unwrap_or(0.0);
            let runs = settings.runs as f32;
            println!("{:<22} {:>5.0} {:>5.0} {:>5.0}  {:>5.0}s  {:>4.0}%   {:<30} {:<30}",
                format!("{a:?} v {b:?}"),
                100.0 * a_wins as f32 / runs,
                100.0 * b_wins as f32 / runs,
                100.0 * capped as f32 / runs,
                median,
                100.0 * median_left,
                split(a_dealt, runs),
                split(b_dealt, runs),
            );
        }
    }
}

/// A side's damage per fight and its split, as `total (auto/ability/reflect %)`
fn split(dealt: Sources, runs: f32) -> String {
    let total = dealt.total();
    if total <= 0.0 {
        return "0".to_owned();
    }
    format!("{:.0} ({:.0}/{:.0}/{:.0}%)",
        total / runs,
        100.0 * dealt.auto / total,
        100.0 * dealt.ability / total,
        100.0 * dealt.reflect / total,
    )
}
