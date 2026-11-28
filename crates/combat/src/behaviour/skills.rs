//! An NPC's skills channel: which of its skills it uses now, if any.
//!
//! Every skill on its bar brings a decision for what it does now (a strike,
//! a block, a leap clear or in as its range has it, a stance it takes),
//! and each decision brings considerations from what it does. Every
//! skill's worth is weighed against its cost, the endurance it spends,
//! through curves no mind sets, or a search would shape them to even out
//! how often skills are used; and no decision is weighed that the gate
//! would refuse ([`admits`]). The channel's do-nothing decision, waiting,
//! scores [`WAIT`], and a skill is used only where it scores higher.
//!
//! Every input is a ratio in the game's own terms, so a curve means the
//! same at any level and holds when a skill's numbers change.

use common_bevy::{
    components::{activation::{Activation, Recharges}, heading::Heading, status::{Stance, Status}, ActorAttributes, Loc},
    message::AbilityType,
    moment::Moment,
};

use super::{mind::Mind, utility::{score, Consideration, Curve, Shape}};
use crate::abilities::{admits, stance, strike, Standing};
use common_bevy::tuning::Tuning;

/// What waiting scores unless a mind sets it: the threshold every skill's
/// decision must beat.
pub const WAIT: f32 = 0.35;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &[
    "worth_answering", "leash_left", "effect_added",
];

/// The blow, as a share of its own health, a block or a leap clear counts
/// as fully worth answering.
const WORTH: f32 = 0.3;

/// What an NPC perceives as it weighs its skills, and the skill it weighs.
#[derive(Clone, Debug)]
pub struct View {
    /// The numbers it plays by
    pub tuning: Tuning,
    pub ability: AbilityType,
    pub attrs: ActorAttributes,
    pub health: f32,
    pub health_max: f32,
    pub endurance: f32,
    pub endurance_max: f32,
    /// The game's clock as it weighs
    pub now: Moment,
    /// It is activating a skill
    pub activating: bool,
    /// Its skills' recharges
    pub recharges: Recharges,
    /// Its own timed effects
    pub status: Status,
    /// It is held up by the water (`movement::swimming`)
    pub swimming: bool,
    /// Its own reach, in tiles
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    /// Share of its leash it would have left where a Leap lands, clear of
    /// its target in reach or onto it out of reach: 1 with no leash, 0 with
    /// nowhere to land
    pub leap_room: f32,
    /// Its pack's attack capacity on its target is taken: as many others of
    /// its engagement are activating a skill at the target
    pub capacity_taken: bool,
    /// Seconds until the skill its foe is activating lands, as it judges
    /// it (`Skill::judged`); None while it sees none
    pub lands_in: Option<f32>,
    pub foe: Option<Foe>,
    /// The kinds of its own runes standing
    pub runes: Vec<common_bevy::components::rune::RuneKind>,
}

/// Its target, as it sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Foe {
    /// Where it stands and which way it faces, which its movement steps by
    pub at: Loc,
    pub heading: Option<Heading>,
    pub distance: i32,
    /// The health it has left, as its target frame shows it
    pub health: f32,
    /// Within the arc it strikes within
    pub in_arc: bool,
    /// It stands past the foe's forward faces, flanking it
    pub flanked: bool,
    /// The skill the foe is activating, as the bar over it shows
    pub activation: Option<Activation>,
    /// What that skill would land on it: none for one that is no strike,
    /// or is aimed elsewhere
    pub strike: f32,
    /// When the foe's next auto-attack comes due, on the game clock; None
    /// for one with no swing coming
    pub swing: Option<Moment>,
    /// What an auto-attack of the foe's would land on it
    pub auto: f32,
    /// Its timed effects, as its target frame shows them
    pub status: Status,
}

/// One reason to use a skill, scored.
#[derive(Clone, Debug)]
pub struct Decision {
    pub ability: AbilityType,
    pub reason: &'static str,
    pub score: f32,
    /// Each consideration's name and response, for the trace
    pub responses: Vec<(&'static str, f32)>,
}

/// The best of `bar`'s decisions for what `view` perceives, as `mind`
/// shapes them, where it beats waiting; None to wait. `view.ability` is
/// set to each skill in turn. `stray` gives each score's error, a share of
/// it.
pub fn choose(view: &mut View, bar: &[AbilityType], mind: &Mind, mut stray: impl FnMut(&Decision) -> f32) -> Option<Decision> {
    weigh(view, bar, mind).into_iter()
        .map(|decision| Decision { score: decision.score * (1.0 + stray(&decision)), ..decision })
        .filter(|decision| decision.score > mind.wait)
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

/// Every decision `bar` brings, scored as `mind` shapes it.
pub fn weigh(view: &mut View, bar: &[AbilityType], mind: &Mind) -> Vec<Decision> {
    let mut decisions = Vec::new();
    for &ability in bar {
        view.ability = ability;
        let Some((reason, considerations)) = reason(ability, view) else { continue };
        let responses: Vec<(&'static str, f32)> = considerations.iter()
            .map(|consideration| (consideration.name, mind.shape(consideration).answer(view)))
            .collect();
        let score = score(1.0, responses.iter().map(|&(_, response)| response));
        decisions.push(Decision { ability, reason, score, responses });
    }
    decisions
}

type Considered = Consideration<View>;

/// What a skill does now, each bringing its considerations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Part {
    Strike,
    /// A strike that interrupts
    Interrupt,
    Block,
    /// A leap clear of a foe in reach
    Clear,
    /// A leap onto a foe out of reach
    Dive,
    /// A step round a foe in reach, toward its back
    Step,
    /// A lunge or a drag that closes on a foe out of reach
    Close,
    /// A stance it takes
    Stance,
    /// A rune cut ahead
    Rune,
}

/// What `ability` may do, each beside its reason: one part, or for a skill
/// that reads the range, its part with a foe in reach and then its part
/// with one beyond (a Leap clears the one, dives onto the other). None for
/// a skill that is no decision.
fn parts(ability: AbilityType) -> &'static [(&'static str, Part)] {
    match ability {
        AbilityType::AutoAttack => &[],
        AbilityType::Frenzy | AbilityType::Overpower | AbilityType::Backstab | AbilityType::Kick | AbilityType::Defy => &[("strike", Part::Strike)],
        AbilityType::Punish => &[("punish", Part::Strike)],
        AbilityType::Feint => &[("interrupt", Part::Interrupt)],
        AbilityType::Parry | AbilityType::Counter | AbilityType::Clash => &[("block", Part::Block)],
        AbilityType::Leap => &[("dodge", Part::Clear), ("dive", Part::Dive)],
        AbilityType::Sidestep => &[("flank", Part::Step)],
        AbilityType::Pounce => &[("close", Part::Close)],
        AbilityType::Yank => &[("pull", Part::Close)],
        AbilityType::PerfectStride => &[("stride", Part::Stance)],
        AbilityType::Bloodlust => &[("bloodlust", Part::Stance)],
        AbilityType::Brace => &[("brace", Part::Stance)],
        AbilityType::Surge => &[("surge", Part::Stance)],
        AbilityType::Vigil => &[("vigil", Part::Stance)],
        AbilityType::Lurk => &[("lurk", Part::Stance)],
        AbilityType::Drag => &[("rune", Part::Rune)],
    }
}

/// What `ability` does for what `view` perceives, and its considerations
/// ([`considerations`]). None where it does nothing.
fn reason(ability: AbilityType, view: &View) -> Option<(&'static str, Vec<Considered>)> {
    let (reason, part) = match parts(ability) {
        [] => return None,
        [one] => *one,
        [near, far, ..] => if view.foe?.distance <= view.reach { *near } else { *far },
    };
    Some((reason, considerations(part)))
}

/// Every decision `ability` may bring, at any range, beside its reason:
/// what the balance arena's skills reference lists.
pub fn decisions(ability: AbilityType) -> Vec<(&'static str, Vec<Consideration<View>>)> {
    parts(ability).iter().map(|&(reason, part)| (reason, considerations(part))).collect()
}

/// What a skill asks doing `part`: what any skill asks, then what its part
/// asks.
fn considerations(part: Part) -> Vec<Considered> {
    let mut considerations = vec![USABLE, ENDURANCE_LEFT];
    considerations.extend(match part {
        Part::Strike => vec![CAPACITY, LANDS, STRIKE_WORTH],
        Part::Interrupt => vec![CAPACITY, INTERRUPTS],
        Part::Block => vec![THREATENED, WORTH_ANSWERING],
        Part::Clear => vec![THREATENED, WORTH_ANSWERING, LEASH_LEFT],
        Part::Dive => vec![CAPACITY, LANDS, STRIKE_WORTH, LEASH_LEFT],
        Part::Step => vec![FACED],
        Part::Close => vec![FOE_BEYOND],
        Part::Stance => vec![TAKES_HOLD, EFFECT_ADDED],
        Part::Rune => vec![RUNE_CATCHES],
    });
    considerations
}

/// Whether taking `stance` would do anything now, by its own rule: a
/// stride or a bloodlust with a foe in reach to strike; a lurk or a surge
/// with its foe beyond reach, unseen or ground to cover; a vigil with its
/// foe outside the arc it strikes within; a brace with a skill that drags
/// aimed at it
fn takes_hold(stance: Stance, view: &View) -> bool {
    let in_reach = view.foe.is_some_and(|foe| foe.distance <= view.reach);
    match stance {
        Stance::PerfectStride | Stance::Bloodlust => in_reach,
        Stance::Lurk | Stance::Surge => view.foe.is_some() && !in_reach,
        Stance::Vigil => view.foe.is_some_and(|foe| !foe.in_arc),
        Stance::Brace => view.foe.is_some_and(|foe| foe.activation.is_some_and(|activation| {
            matches!(activation.ability, AbilityType::Kick | AbilityType::Yank) && view.lands_in.is_some()
        })),
    }
}

/// Whether a foe whose bar is `foe` can bring about what `stance` takes
/// hold against ([`takes_hold`]): a brace wants a skill that drags, a vigil
/// one that takes its user round or lands only from a flank. Any foe may
/// stand in reach or beyond it, which every other stance asks.
pub fn arises(stance: Stance, foe: &[AbilityType]) -> bool {
    match stance {
        Stance::Brace => foe.iter().any(|ability| matches!(ability, AbilityType::Kick | AbilityType::Yank)),
        Stance::Vigil => foe.iter().any(|ability| ability.flanks() || *ability == AbilityType::Sidestep),
        Stance::PerfectStride | Stance::Bloodlust | Stance::Lurk | Stance::Surge => true,
    }
}

const fn step(name: &'static str, read: fn(&View) -> f32) -> Considered {
    Consideration { name, read, bounds: (1.0, 1.0), curve: Curve::RISING }
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// Its health, never so low a ratio over it runs away
fn health(view: &View) -> f32 {
    view.health.max(1.0)
}

/// What a block put up now, or a leap clear, would stop: the blow of the
/// strike its foe is activating at it, where that lands within a block's
/// seconds as it judges it; else its foe's auto-attack, where one is due
/// within them and its foe stands to swing, in its reach and facing it
fn incoming(view: &View) -> f32 {
    let Some(foe) = view.foe else { return 0.0 };
    let within = view.tuning.block_secs;
    if foe.strike > 0.0 && view.lands_in.is_some_and(|lands_in| lands_in <= within) {
        return foe.strike;
    }
    let swings = foe.activation.is_none() && !foe.flanked && foe.distance <= view.reach
        && foe.swing.is_some_and(|due| view.now.until(due).as_secs_f32() <= within);
    if swings { foe.auto } else { 0.0 }
}

// --- Any skill ---

/// The gate would let the skill through on its foe now: it unheld, not
/// swimming and activating nothing, the skill recharged and paid for, its foe within
/// its reach and arc for one with a reach
const USABLE: Considered = step("usable", |view| {
    let standing = Standing {
        held: view.status.is_held(),
        swimming: view.swimming,
        pinned: view.status.is_pinned(),
        activating: view.activating,
        recharged: view.recharges.is_ready(view.ability, view.now),
        endurance: view.endurance,
    };
    let foe = view.foe.map(|foe| (foe.distance, foe.in_arc));
    flag(admits(&view.tuning, view.ability, &standing, &view.attrs, view.reach, foe).is_ok())
});

/// The share of its pool it would have left once the skill is paid for:
/// what spending it now leaves the rest of the fight, a dearer skill and an
/// emptier pool weighing more. Authored and no mind's to set, or a search
/// would shape it to even out how often skills are used
const ENDURANCE_LEFT: Considered = Consideration {
    name: "endurance_left",
    read: |view| (view.endurance - view.tuning.cost(view.ability)).max(0.0) / view.endurance_max.max(f32::EPSILON),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.2),
};

// --- Strike, and a leap onto a foe ---

/// Room in its pack's attack capacity on its target
const CAPACITY: Considered = step("capacity", |view| flag(!view.capacity_taken));

/// The strike could land from where it stands, by the strike's own rule:
/// a Leap reaches its foe, a Backstab is struck from past its foe's
/// forward faces. The gate lets both through to miss
const LANDS: Considered = step("lands", |view| flag(view.foe.is_some_and(|foe| {
    let reaches = view.ability != AbilityType::Leap || foe.distance <= view.leap + view.reach;
    reaches && (!view.ability.flanks() || foe.flanked)
})));

/// What a strike would deal, with every modifier it carries, as a share of
/// the health its foe has left: how much nearer it brings the kill, a
/// finishing blow most. Authored and no mind's to set, or a search would
/// flatten it until a heavy blow counted no more than a light one
const STRIKE_WORTH: Considered = Consideration {
    name: "strike_worth",
    read: |view| {
        let Some(foe) = view.foe else { return 0.0 };
        let tuning = &view.tuning;
        let lost = 1.0 - view.health / view.health_max.max(f32::EPSILON);
        let share = match view.ability {
            AbilityType::Leap => tuning.damage(AbilityType::Leap) * view.attrs.line_power(tuning, AbilityType::Leap),
            ability => strike::share(tuning, &view.attrs, ability, foe.activation.is_some(), lost),
        };
        let ambush = view.status.stance(Stance::Lurk).map_or(1.0, |lurk| 1.0 + lurk.pace);
        view.attrs.base_potency(tuning) * share * ambush / foe.health.max(1.0)
    },
    bounds: (0.0, 0.25),
    curve: Curve::RISING.floored(0.3),
};

/// Its foe is activating a skill, and the interrupt would land first, as
/// it judges when that skill lands
const INTERRUPTS: Considered = step("interrupts", |view| {
    flag(view.lands_in.is_some_and(|lands_in| view.tuning.activation(view.ability) < lands_in))
});

// --- Block, and a leap clear to dodge ---

/// Something is coming that a block would stop ([`incoming`])
const THREATENED: Considered = step("threatened", |view| flag(incoming(view) > 0.0));

/// What it would stop, and what a Counter would strike back of that as its
/// line has it, over its health
const WORTH_ANSWERING: Considered = Consideration {
    name: "worth_answering",
    read: |view| {
        let stopped = incoming(view);
        let returned = match view.ability {
            AbilityType::Counter => stopped * view.tuning.counter_reflect * view.attrs.line_power(&view.tuning, AbilityType::Counter),
            _ => 0.0,
        };
        (stopped + returned) / health(view)
    },
    bounds: (0.0, WORTH),
    curve: Curve { shape: Shape::Logistic { mid: 0.4, steep: 8.0 }, falling: false, floor: 0.0 },
};

// --- Leap ---

/// The share of its leash it would have left where it lands
const LEASH_LEFT: Considered = Consideration {
    name: "leash_left",
    read: |view| view.leap_room,
    bounds: (0.0, 0.3),
    curve: Curve::RISING,
};

// --- Moves ---

/// Its foe stands in its reach and it stands within the foe's forward
/// faces: a step round takes it out of them
const FACED: Considered = step("faced", |view| flag(view.foe.is_some_and(|foe| foe.distance <= view.reach && !foe.flanked)));

/// Its foe stands beyond its reach, ground to close
const FOE_BEYOND: Considered = step("foe_beyond", |view| flag(view.foe.is_some_and(|foe| foe.distance > view.reach)));

// --- Stance ---

/// The stance it would take would do something now, by the stance's own
/// rule ([`takes_hold`])
const TAKES_HOLD: Considered = step("takes_hold", |view| flag(Stance::of(view.ability).is_some_and(|stance| takes_hold(stance, view))));

/// How much of the stance it would take is new: what a fresh one lasts
/// less what is left of the same on it, over what a fresh one lasts.
/// Nothing new vetoes it
const EFFECT_ADDED: Considered = Consideration {
    name: "effect_added",
    read: |view| Stance::of(view.ability).map_or(1.0, |taken| {
        let lasts = stance::timed(&view.tuning, &view.attrs, taken).remaining.max(f32::EPSILON);
        let left = view.status.stance(taken).map_or(0.0, |timed| timed.remaining.max(0.0));
        (lasts - left) / lasts
    }),
    bounds: (0.0, 1.0),
    curve: Curve::RISING,
};

// --- Rune ---

/// Whether the rune `view.ability` cuts would catch its foe: the foe beyond
/// its own reach, in the arc it faces, within the rune's reach of where the
/// rune would land; and none of its own runes of that kind standing, which
/// cutting another would lift.
const RUNE_CATCHES: Considered = step("rune_catches", |view| {
    let Some(rune) = view.ability.rune() else { return 0.0 };
    let Some(foe) = view.foe else { return 0.0 };
    let lands = crate::runes::distance(&view.tuning, rune) as i32;
    let reach = crate::runes::reach_of(&view.tuning, &view.attrs, rune);
    flag(foe.in_arc && foe.distance > view.reach && (foe.distance - lands).abs() <= reach && !view.runes.contains(&rune.kind()))
});

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use common_bevy::components::{activation::Recharge, status::Timed};

    const NOW: Moment = Moment::from_millis(10_000);

    fn view(tuning: &Tuning, ability: AbilityType, attrs: ActorAttributes) -> View {
        View {
            tuning: *tuning,
            ability,
            attrs,
            health: 600.0,
            health_max: 600.0,
            endurance: attrs.max_endurance(tuning),
            endurance_max: attrs.max_endurance(tuning),
            now: NOW,
            activating: false,
            recharges: Recharges::default(),
            status: Status::default(),
            swimming: false,
            reach: 2,
            leap: 9,
            leap_room: 1.0,
            capacity_taken: false,
            lands_in: None,
            foe: Some(Foe { distance: 1, health: 600.0, in_arc: true, ..Foe::default() }),
            runes: Vec::new(),
        }
    }

    /// `view` with its foe `secs` into activating a strike at it that
    /// would land `damage`, `left` seconds from landing
    fn struck_at(mut view: View, damage: f32, left: f32) -> View {
        let activation = Activation { ability: AbilityType::Overpower, target: None, began: NOW, length: Duration::from_secs_f32(left) };
        view.foe = Some(Foe { activation: Some(activation), strike: damage, ..view.foe.unwrap() });
        view.lands_in = Some(left);
        view
    }

    fn scored_by(view: &mut View, reason: &str, mind: &Mind) -> f32 {
        let bar = [view.ability];
        weigh(view, &bar, mind).into_iter().find(|decision| decision.reason == reason).map_or(0.0, |decision| decision.score)
    }

    fn scored(view: &mut View, reason: &str) -> f32 {
        scored_by(view, reason, &Mind::default())
    }

    fn response(view: &mut View, name: &str) -> f32 {
        let bar = [view.ability];
        weigh(view, &bar, &Mind::default())[0].responses.iter().find(|(named, _)| *named == name).unwrap().1
    }

    #[test]
    fn a_rune_is_cut_where_it_would_catch_a_foe_beyond_reach_and_not_again_while_it_stands() {
        let tuning = Tuning::DEFAULT;
        let mut view = view(&tuning, AbilityType::Drag, ActorAttributes::default());
        let lands = crate::runes::distance(&tuning, common_bevy::components::rune::Rune::Drag) as i32;
        let catches = |view: &View| RUNE_CATCHES.answer(view) > 0.0;
        view.foe.as_mut().unwrap().distance = lands;
        assert!(catches(&view), "a foe where the rune lands");
        view.foe.as_mut().unwrap().distance = 1;
        assert!(!catches(&view), "a foe in its own reach it strikes instead");
        view.foe.as_mut().unwrap().distance = lands;
        view.foe.as_mut().unwrap().in_arc = false;
        assert!(!catches(&view), "a rune lands ahead, so a foe off its arc is missed");
        view.foe.as_mut().unwrap().in_arc = true;
        view.runes.push(common_bevy::components::rune::RuneKind::Control);
        assert!(!catches(&view), "its control rune stands: another would lift it");
    }

    #[test]
    fn nothing_coming_nothing_to_block() {
        let tuning = Tuning::DEFAULT;
        let mut quiet = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        assert!(choose(&mut quiet, &[AbilityType::Counter], &Mind::default(), |_| 0.0).is_none());
    }

    #[test]
    fn a_heavy_blow_is_blocked_and_a_light_one_let_land() {
        let tuning = Tuning::DEFAULT;
        let mut heavy = struck_at(view(&tuning, AbilityType::Parry, ActorAttributes::default()), 150.0, 1.0);
        let mut light = struck_at(view(&tuning, AbilityType::Parry, ActorAttributes::default()), 20.0, 1.0);
        assert!(scored(&mut heavy, "block") > WAIT, "a blow of a quarter of its health is blocked");
        assert!(scored(&mut light, "block") < WAIT, "one light blow is let land");
        let mut swung = view(&tuning, AbilityType::Parry, ActorAttributes::default());
        swung.foe = Some(Foe { swing: Some(NOW + Duration::from_millis(500)), auto: 150.0, ..swung.foe.unwrap() });
        assert!(scored(&mut swung, "block") > WAIT, "a heavy swing coming due is blocked as a strike is");
        swung.foe = Some(Foe { flanked: true, ..swung.foe.unwrap() });
        assert_eq!(scored(&mut swung, "block"), 0.0, "but not one from a foe it stands behind");
    }

    #[test]
    fn a_block_waits_until_the_blow_would_land_within_it() {
        let tuning = Tuning::DEFAULT;
        let mut soon = struck_at(view(&tuning, AbilityType::Parry, ActorAttributes::default()), 150.0, tuning.block_secs * 0.5);
        let mut far = struck_at(view(&tuning, AbilityType::Parry, ActorAttributes::default()), 150.0, tuning.block_secs * 2.0);
        assert!(scored(&mut soon, "block") > WAIT);
        assert_eq!(scored(&mut far, "block"), 0.0, "a block up now would run out before it lands");
        let mut dodge = struck_at(view(&tuning, AbilityType::Leap, ActorAttributes::default()), 150.0, tuning.block_secs * 0.5);
        assert!(scored(&mut dodge, "dodge") > WAIT, "and a leap clear answers the same blow");
    }

    #[test]
    fn an_interrupt_is_held_for_a_foe_mid_activation_it_would_land_before() {
        let tuning = Tuning::DEFAULT;
        let feint = tuning.activation(AbilityType::Feint);
        let mut idle = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        assert_eq!(scored(&mut idle, "interrupt"), 0.0, "a foe activating nothing has nothing to interrupt");
        let mut caught = struck_at(view(&tuning, AbilityType::Feint, ActorAttributes::default()), 100.0, feint * 2.0);
        assert!(scored(&mut caught, "interrupt") > WAIT, "one it would land before is interrupted");
        let mut late = struck_at(view(&tuning, AbilityType::Feint, ActorAttributes::default()), 100.0, feint * 0.5);
        assert_eq!(scored(&mut late, "interrupt"), 0.0, "one that lands first is not");
    }

    #[test]
    fn what_the_gate_refuses_is_never_weighed() {
        let tuning = Tuning::DEFAULT;
        let plain = || view(&tuning, AbilityType::Frenzy, ActorAttributes::default());
        assert!(scored(&mut plain(), "strike") > 0.0);
        let mut far = plain();
        far.foe = Some(Foe { distance: 3, ..far.foe.unwrap() });
        assert_eq!(scored(&mut far, "strike"), 0.0, "out of reach");
        let mut behind = plain();
        behind.foe = Some(Foe { in_arc: false, ..behind.foe.unwrap() });
        assert_eq!(scored(&mut behind, "strike"), 0.0, "outside its arc");
        let mut recharging = plain();
        recharging.recharges.start(AbilityType::Frenzy, Recharge { began: NOW, length: Duration::from_secs(3) });
        assert_eq!(scored(&mut recharging, "strike"), 0.0, "recharging");
        let mut committed = plain();
        committed.activating = true;
        assert_eq!(scored(&mut committed, "strike"), 0.0, "mid-activation");
        let mut spent = plain();
        spent.endurance = 0.0;
        assert_eq!(scored(&mut spent, "strike"), 0.0, "without its endurance");
    }

    #[test]
    fn a_strike_waits_while_its_pack_holds_the_capacity() {
        let tuning = Tuning::DEFAULT;
        let mut full = view(&tuning, AbilityType::Frenzy, ActorAttributes::default());
        full.capacity_taken = true;
        assert_eq!(scored(&mut full, "strike"), 0.0);
        let mut blocking = struck_at(view(&tuning, AbilityType::Counter, ActorAttributes::default()), 150.0, 1.0);
        blocking.capacity_taken = true;
        assert!(scored(&mut blocking, "block") > WAIT, "a block takes no slot");
    }

    #[test]
    fn a_strike_is_worth_what_it_would_land() {
        let tuning = Tuning::DEFAULT;
        let worth = |view: &mut View| response(view, "strike_worth");
        let mut front = view(&tuning, AbilityType::Backstab, ActorAttributes::default());
        let mut behind = front.clone();
        behind.foe = Some(Foe { flanked: true, ..behind.foe.unwrap() });
        assert_eq!(scored(&mut front, "strike"), 0.0, "a Backstab from in front lands nothing, and is not struck");
        assert!(scored(&mut behind, "strike") > WAIT, "from past its foe's forward faces it is");

        let instinctive = ActorAttributes::new(0, 0, 0, 0, 0, 0, -25, 0, 0);
        let mut resting = view(&tuning, AbilityType::Punish, instinctive);
        let mut caught = struck_at(view(&tuning, AbilityType::Punish, instinctive), 0.0, 1.0);
        assert!(worth(&mut caught) > worth(&mut resting), "a Punish is worth more on a foe mid-activation");

        let resolute = ActorAttributes::new(0, 0, 0, 0, 0, 0, 25, 0, 0);
        let mut hale = view(&tuning, AbilityType::Defy, resolute);
        let mut hurt = hale.clone();
        hurt.health = 100.0;
        assert!(worth(&mut hurt) > worth(&mut hale), "a Defy is worth more the more health it has lost");

        let mut finishing = view(&tuning, AbilityType::Frenzy, ActorAttributes::default());
        finishing.foe = Some(Foe { health: 20.0, ..finishing.foe.unwrap() });
        assert!(worth(&mut finishing) > worth(&mut view(&tuning, AbilityType::Frenzy, ActorAttributes::default())), "a finishing blow most");
    }

    #[test]
    fn a_stance_is_taken_only_where_it_takes_hold_and_not_over_itself() {
        let tuning = Tuning::DEFAULT;
        let taken = |view: &mut View| choose(view, &[view.ability], &Mind::default(), |_| 0.0).is_some();

        let mut beside = view(&tuning, AbilityType::Lurk, ActorAttributes::default());
        assert!(!taken(&mut beside), "no lurking beside its foe");
        let mut apart = view(&tuning, AbilityType::Lurk, ActorAttributes::default());
        apart.foe.as_mut().unwrap().distance = 5;
        assert!(taken(&mut apart), "but beyond its reach");

        let mut calm = view(&tuning, AbilityType::Brace, ActorAttributes::default());
        assert!(!taken(&mut calm), "no bracing with nothing coming");
        let mut coming = struck_at(view(&tuning, AbilityType::Brace, ActorAttributes::default()), 30.0, 0.4);
        coming.foe.as_mut().unwrap().activation.as_mut().unwrap().ability = AbilityType::Kick;
        assert!(taken(&mut coming), "but against a kick mid-activation");

        let mut faced = view(&tuning, AbilityType::Vigil, ActorAttributes::default());
        assert!(!taken(&mut faced), "no vigil with its foe in front of it");
        let mut flanked = view(&tuning, AbilityType::Vigil, ActorAttributes::default());
        flanked.foe.as_mut().unwrap().in_arc = false;
        assert!(taken(&mut flanked), "but with its foe outside its arc");

        let mut fresh = view(&tuning, AbilityType::Bloodlust, ActorAttributes::default());
        assert!(taken(&mut fresh));
        fresh.status.take(Stance::Bloodlust, Timed { pace: 0.1, remaining: tuning.bloodlust_secs });
        assert!(!taken(&mut fresh), "one just taken adds nothing");
    }

    #[test]
    fn a_counter_is_worth_more_than_a_parry_by_what_it_returns() {
        let tuning = Tuning::DEFAULT;
        let mut parry = struck_at(view(&tuning, AbilityType::Parry, ActorAttributes::default()), 60.0, 1.0);
        let mut counter = struck_at(view(&tuning, AbilityType::Counter, ActorAttributes::new(0, 0, 0, 0, 0, 0, 25, 0, 0)), 60.0, 1.0);
        counter.health = parry.health;
        assert!(response(&mut counter, "worth_answering") > response(&mut parry, "worth_answering"));
    }

    #[test]
    fn a_dearer_skill_and_an_emptier_pool_weigh_more() {
        let tuning = Tuning::DEFAULT;
        let left = |ability: AbilityType, endurance: f32| {
            let mut view = view(&tuning, ability, ActorAttributes::default());
            view.endurance = endurance;
            response(&mut view, "endurance_left")
        };
        assert!(left(AbilityType::Frenzy, 100.0) > left(AbilityType::Overpower, 100.0), "a dearer skill leaves less");
        assert!(left(AbilityType::Frenzy, 100.0) > left(AbilityType::Frenzy, 40.0), "and so does an emptier pool");
    }

    #[test]
    fn a_move_closes_on_a_foe_beyond_reach_and_a_sidestep_leaves_its_faces() {
        let tuning = Tuning::DEFAULT;
        let mut near = view(&tuning, AbilityType::Pounce, ActorAttributes::default());
        assert_eq!(scored(&mut near, "close"), 0.0, "nothing to close beside its foe");
        let mut apart = view(&tuning, AbilityType::Pounce, ActorAttributes::default());
        apart.foe.as_mut().unwrap().distance = 4;
        assert!(scored(&mut apart, "close") > WAIT);

        let mut faced = view(&tuning, AbilityType::Sidestep, ActorAttributes::default());
        assert!(scored(&mut faced, "flank") > WAIT, "before its foe, it steps round");
        let mut round = view(&tuning, AbilityType::Sidestep, ActorAttributes::default());
        round.foe.as_mut().unwrap().flanked = true;
        assert_eq!(scored(&mut round, "flank"), 0.0, "past its faces already, it stands");
    }
}
