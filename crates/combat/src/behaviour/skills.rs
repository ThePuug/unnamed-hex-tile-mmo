//! An NPC's skills channel: which of its skills it uses now, if any.
//!
//! Every skill on its bar brings a decision for what it does now (a strike,
//! a reaction, a leap clear or in as its range has it, an effect it puts
//! on someone), and each decision brings considerations from what it does
//! and from the commitments its user holds. A commitment's considerations
//! read its own state, so an NPC that scores them plays the way its
//! commitment pays: no style is written down. Every skill's worth is
//! weighed against its cost, the endurance it spends, through curves no
//! mind sets, or a search would shape them to even out how often skills
//! are used; and no decision is weighed that the gate would refuse
//! ([`admits`]). The channel's do-nothing decision, waiting, scores
//! [`WAIT`], and a skill is used only where it scores higher; the combo its
//! recovery offers scores [`COMBO`] more, so a chain carries on rather than
//! wait for another skill to unlock.
//!
//! Every input is a ratio in the game's own terms, so a curve means the
//! same at any level and holds when a skill's numbers change.

use std::time::Duration;

use common_bevy::{
    components::{heading::Heading, reaction_queue::QueuedThreat, recovery::GlobalRecovery, status::Status, ActorAttributes, Loc},
    message::AbilityType,
    moment::Moment,
};

use super::{mind::Mind, utility::{score, Consideration, Curve, Shape}};
use crate::abilities::{admits, punish};
use common_bevy::tuning::Tuning;

/// What waiting scores unless a mind sets it: the threshold every skill's
/// decision must beat.
pub const WAIT: f32 = 0.35;

/// What the combo its recovery offers scores beside its own, unless a mind
/// sets it
pub const COMBO: f32 = 0.15;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &[
    "foe_just_acted", "worth_answering", "leash_left", "effect_added", "exposure",
];

/// `leash_left`'s bounds and curve, shared with the movement channel's
pub const LEASH_LEFT_BOUNDS: (f32, f32) = (0.0, 0.3);
pub const LEASH_LEFT_CURVE: Curve = Curve::RISING;

/// The blow, as a share of its own health, a reaction or a leap clear
/// counts as fully worth answering.
const WORTH: f32 = 0.3;

/// What an NPC perceives as it weighs its skills, and the skill it weighs.
#[derive(Clone, Debug)]
pub struct View {
    /// The numbers it plays by
    pub tuning: Tuning,
    pub ability: AbilityType,
    pub attrs: ActorAttributes,
    pub health: f32,
    pub endurance: f32,
    pub endurance_max: f32,
    pub recovery: Option<GlobalRecovery>,
    /// Its own timed effects
    pub status: Status,
    /// Its own reach, in tiles
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    /// Share of its leash it would have left where a Leap lands, clear of
    /// its target in reach or onto it out of reach: 1 with no leash, 0 with
    /// nowhere to land
    pub leap_room: f32,
    /// Its pack's attack capacity on its target is taken: as many others of
    /// its engagement have an ability standing in the target's queue
    pub capacity_taken: bool,
    pub queue: Threats,
    pub foe: Option<Foe>,
}

/// Its queue, as a reaction would meet it.
#[derive(Clone, Copy, Debug)]
pub struct Threats {
    /// Damage a reaction pressed now would take: what it judges landing in
    /// its band
    pub swept: f32,
    /// Of that, what the blows deal on landing, apart from what their
    /// DoTs have left: what a reflection returns a share of
    pub swept_direct: f32,
    /// How many threats that is: past the first, each pays back a share of
    /// the reaction's price at Awareness's facet
    pub taken: usize,
    /// How far into its band the soonest threat it would take stands, its
    /// time left as a share of the span: 1 at the band's far end, and with
    /// nothing in it
    pub soonest_left: f32,
}

impl Default for Threats {
    fn default() -> Self {
        Self { swept: 0.0, swept_direct: 0.0, taken: 0, soonest_left: 1.0 }
    }
}

impl Threats {
    /// What a reaction pressed at `now`, the game's clock, reaching `span`
    /// would take of `queue`, each threat beside when it is judged to land:
    /// the band from the press, or with a `snap` (Awareness's capstone) from
    /// a threat judged landing within `snap` of it, as `ReactionQueue::band`
    /// starts it.
    pub fn reading(queue: &[(Moment, QueuedThreat)], span: Duration, snap: Option<Duration>, now: Moment) -> Self {
        let start = snap.and_then(|snap| queue.iter().map(|(lands, _)| *lands).filter(|lands| (now..=now + snap).contains(lands)).min()).unwrap_or(now);
        let mut read = Self::default();
        for (lands, threat) in queue.iter().filter(|(lands, _)| (start..=start + span).contains(lands)) {
            read.swept += threat.damage + threat.dot_left();
            read.swept_direct += threat.damage;
            read.taken += 1;
            let left = if span.is_zero() { 0.0 } else { lands.since(now).as_secs_f32() / span.as_secs_f32() };
            read.soonest_left = read.soonest_left.min(left);
        }
        read
    }
}

/// Its target, as it sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// The share likelier the foe's skills would crit it for each stack of
    /// Overcommitted it carries, weighed by how much harder its crits land:
    /// none for a foe without Patience
    pub patient: f32,
    /// Seconds since the foe last used a skill, as its clip showed, as
    /// its Approach weighs them ([`super::mind::Mind::just_acted`])
    pub since_skill: Option<f32>,
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
/// shapes them, where it beats waiting; None to wait. The combo its
/// recovery offers scores `mind.combo` more. `view.ability` is set to each
/// skill in turn. `stray` gives each score's error, a share of it.
pub fn choose(view: &mut View, bar: &[AbilityType], mind: &Mind, mut stray: impl FnMut(&Decision) -> f32) -> Option<Decision> {
    let offered = view.recovery.as_ref().filter(|recovery| recovery.is_active()).and_then(|recovery| recovery.combo).map(|combo| combo.ability);
    weigh(view, bar, mind).into_iter()
        .map(|decision| {
            let chained = if offered == Some(decision.ability) && decision.score > 0.0 { mind.combo } else { 0.0 };
            Decision { score: decision.score * (1.0 + stray(&decision)) + chained, ..decision }
        })
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
    Reaction,
    /// A leap clear of a foe in reach
    Clear,
    /// A leap onto a foe out of reach
    Dive,
    /// A timed effect it puts on itself
    Effect,
}

/// What `ability` does for what `view` perceives, and its considerations:
/// what any skill asks, what its part asks, what the commitments its user
/// holds ask of that part, and what any timed effect it would put on
/// someone asks. A Leap does what its range has it do: clear of a foe in
/// reach, onto one out of it. None where it does nothing.
fn reason(ability: AbilityType, view: &View) -> Option<(&'static str, Vec<Considered>)> {
    let (reason, part) = match ability {
        AbilityType::AutoAttack => return None,
        AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower => ("strike", Part::Strike),
        AbilityType::Punish => ("punish", Part::Strike),
        AbilityType::Parry | AbilityType::Counter => ("answer", Part::Reaction),
        AbilityType::Leap => match view.foe?.distance <= view.reach {
            true => ("dodge", Part::Clear),
            false => ("dive", Part::Dive),
        },
        AbilityType::PerfectStride => ("stride", Part::Effect),
    };
    let mut considerations = vec![USABLE, ENDURANCE_LEFT];
    considerations.extend(part_considerations(part));
    if effect(view).is_some() {
        considerations.push(EFFECT_ADDED);
    }
    Some((reason, considerations))
}

fn part_considerations(part: Part) -> Vec<Considered> {
    match part {
        Part::Strike => vec![FOE_JUST_ACTED, CAPACITY, STRIKE_WORTH, EXPOSURE],
        Part::Reaction => vec![MID_BAND, WORTH_ANSWERING],
        Part::Clear => vec![MID_BAND, WORTH_ANSWERING, LEASH_LEFT],
        Part::Dive => vec![CAPACITY, STRIKE_WORTH, LEASH_LEFT, EXPOSURE],
        Part::Effect => vec![IN_REACH],
    }
}

/// A timed effect a decision puts on someone, the decision's whole purpose:
/// a strike's own effects (Grace's capstone breaking a flanked foe's
/// stride) ride on what it deals, so a fresh one already standing never
/// holds the strike back.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Effect {
    /// Perfect Stride, on itself
    PerfectStride,
}

impl Effect {
    /// Seconds a fresh one lasts on whoever it lands on
    fn lasts(self, view: &View) -> f32 {
        match self {
            Effect::PerfectStride => view.tuning.stride_secs,
        }
    }

    /// Seconds left of the same effect on whoever it lands on
    fn left(self, view: &View) -> f32 {
        let timed = match self {
            Effect::PerfectStride => view.status.perfect_stride,
        };
        timed.map_or(0.0, |timed| timed.remaining.max(0.0))
    }
}

/// The timed effect `view.ability` would put on someone now: Perfect
/// Stride's on itself. None for a decision that puts on none.
fn effect(view: &View) -> Option<Effect> {
    match view.ability {
        AbilityType::PerfectStride => Some(Effect::PerfectStride),
        _ => None,
    }
}

/// Whether `view.ability` would be a reaction used now: a Leap with its
/// foe in reach leaps clear
fn reacting(view: &View) -> bool {
    view.ability.reacts(view.foe.is_some_and(|foe| foe.distance <= view.reach))
}

const fn step(name: &'static str, read: fn(&View) -> f32) -> Considered {
    Consideration { name, read, bounds: (1.0, 1.0), curve: Curve::RISING }
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// The share of each blow it clears that `ability` sends back to the blow's
/// source: Counter's reflection, and nothing for any other
fn returned(tuning: &Tuning, ability: AbilityType) -> f32 {
    match ability {
        AbilityType::Counter => tuning.counter_reflect,
        _ => 0.0,
    }
}

/// Its health, never so low a ratio over it runs away
fn health(view: &View) -> f32 {
    view.health.max(1.0)
}

// --- Any skill ---

/// The gate would let the skill through on its foe now: its recovery, its
/// foe within its reach and arc for one that strikes
const USABLE: Considered = step("usable", |view| {
    let foe = view.foe.map(|foe| (foe.distance, foe.in_arc));
    flag(admits(view.ability, reacting(view), view.recovery.as_ref(), &view.attrs, view.reach, foe, view.status.is_pinned()).is_ok())
});

/// The share of its pool it would have left once the skill is paid for,
/// at the price it would pay: dearer by the toll of an Intimidating foe's
/// zone it stands in, cheaper for a reaction taking many at Awareness's
/// facet, as the gate charges it. What spending it now leaves the rest of
/// the fight, a dearer skill and an emptier pool weighing more. Authored
/// and no mind's to set, or a search would shape it to even out how often
/// skills are used
const ENDURANCE_LEFT: Considered = Consideration {
    name: "endurance_left",
    read: |view| {
        let taken = if reacting(view) { view.queue.taken } else { 0 };
        let refund = (view.attrs.awareness_refund(&view.tuning) * taken.saturating_sub(1) as f32).min(1.0);
        let price = view.attrs.skill_endurance(&view.tuning, view.ability) * view.status.price() * (1.0 - refund);
        (view.endurance - price).max(0.0) / view.endurance_max.max(f32::EPSILON)
    },
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.2),
};

/// How much of the timed effect it would put on someone is new: what a
/// fresh one lasts less what is left of the same on whoever it lands on,
/// over what a fresh one lasts. Nothing new vetoes it
const EFFECT_ADDED: Considered = Consideration {
    name: "effect_added",
    read: |view| effect(view).map_or(1.0, |effect| {
        let lasts = effect.lasts(view).max(f32::EPSILON);
        (lasts - effect.left(view)) / lasts
    }),
    bounds: (0.0, 1.0),
    curve: Curve::RISING,
};

// --- Strike, and a leap onto a foe ---

/// Room in its pack's attack capacity on its target
const CAPACITY: Considered = step("capacity", |view| flag(!view.capacity_taken));

/// A foe that has just used a skill is likely in its recovery and cannot
/// answer with another: the sooner after, the likelier
const FOE_JUST_ACTED: Considered = Consideration {
    name: "foe_just_acted",
    read: |view| view.foe.and_then(|foe| foe.since_skill).unwrap_or(f32::INFINITY),
    bounds: (2.0, 0.0),
    curve: Curve::RISING.floored(0.6),
};

/// What a strike would deal, with every modifier it carries, as a share of
/// the health its foe has left: how much nearer it brings the kill, a
/// finishing blow most. A strike that cannot land deals nothing. Authored
/// and no mind's to set, or a search would flatten it until a heavy blow
/// counted no more than a light one
const STRIKE_WORTH: Considered = Consideration {
    name: "strike_worth",
    read: |view| {
        let Some(foe) = view.foe else { return 0.0 };
        if view.ability == AbilityType::Leap && foe.distance > view.leap + view.reach {
            return 0.0;
        }
        let tuning = &view.tuning;
        let stacks = foe.status.overcommits();
        let flank = if foe.flanked { 1.0 + view.attrs.flank(tuning) } else { 1.0 };
        // Patience: a crit likelier by the stacks, harder by its facet, and
        // certain once its capstone's stacks stand
        let opening = stacks > 0 && view.attrs.patience_opening(tuning).is_some_and(|at| stacks >= at);
        let chance = if opening { 1.0 } else { (view.attrs.patience_crit(tuning) * stacks as f32).min(1.0) };
        let power = if stacks > 0 { view.attrs.patience_power(tuning) } else { 0.0 };
        let patient = 1.0 + (tuning.crit_power * (1.0 + power) - 1.0) * chance;
        let share = match view.ability {
            AbilityType::Punish => punish::share(tuning, &view.attrs, stacks),
            ability => tuning.damage(ability) * view.attrs.line_power(tuning, ability),
        };
        let dealt = view.attrs.base_potency(tuning) * share * flank * patient;
        dealt / foe.health.max(1.0)
    },
    bounds: (0.0, 0.25),
    curve: Curve::RISING.floored(0.3),
};

/// How much likelier a patient foe's skills would crit it once the strike
/// overcommits it a stack more: the dearer, the less the strike pays.
/// Weighed only as far as a mind lowers its floor
const EXPOSURE: Considered = Consideration {
    name: "exposure",
    read: |view| view.foe.map_or(0.0, |foe| foe.patient * (view.status.overcommits() + 1) as f32),
    bounds: (0.0, 0.9),
    curve: Curve::FALLING.floored(1.0),
};

// --- Reaction, and a leap clear to dodge ---

/// What it would clear, and what it would return of that, with every
/// modifier either carries, over its health
const WORTH_ANSWERING: Considered = Consideration {
    name: "worth_answering",
    read: |view| {
        let returned = view.queue.swept_direct * returned(&view.tuning, view.ability) * view.attrs.line_power(&view.tuning, view.ability);
        (view.queue.swept + returned) / health(view)
    },
    bounds: (0.0, WORTH),
    curve: Curve { shape: Shape::Logistic { mid: 0.4, steep: 8.0 }, falling: false, floor: 0.0 },
};

/// The soonest threat in its band has reached the band's middle, where a
/// press misjudged by up to half its span either way still takes it; a
/// snap then starts the band at that threat, reaching deeper behind it.
/// Every threat's window outlasts the widest span and the slowest reaction
/// delay, so it is seen before it reaches the band
const MID_BAND: Considered = step("mid_band", |view| flag(view.queue.soonest_left <= 0.5));

// --- Leap ---

/// The share of its leash it would have left where it lands
const LEASH_LEFT: Considered = Consideration {
    name: "leash_left",
    read: |view| view.leap_room,
    bounds: LEASH_LEFT_BOUNDS,
    curve: LEASH_LEFT_CURVE,
};

// --- Effect ---

/// Its foe stands within its reach, where an effect on itself pays
const IN_REACH: Considered = step("in_reach", |view| flag(view.foe.is_some_and(|foe| foe.distance <= view.reach)));

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;
    use common_bevy::{components::status::Timed, systems::combat::queue::{create_threat, threat_window}};

    /// An actor built from its three pairs' axis, spectrum and shift
    fn built(points: [i8; 9]) -> ActorAttributes {
        let [a, b, c, d, e, f, g, h, i] = points;
        ActorAttributes::new(a, b, c, d, e, f, g, h, i)
    }

    fn view(tuning: &Tuning, ability: AbilityType, attrs: ActorAttributes) -> View {
        View {
            tuning: *tuning,
            ability,
            attrs,
            health: 600.0,
            // A pool deep enough to clear the blows these tests queue
            endurance: attrs.max_endurance(tuning).max(1000.0),
            endurance_max: attrs.max_endurance(tuning).max(1000.0),
            recovery: None,
            status: Status::default(),
            reach: 2,
            leap: 9,
            leap_room: 1.0,
            capacity_taken: false,
            queue: Threats::default(),
            foe: Some(Foe { at: Loc::default(), heading: None, distance: 1, health: 600.0, in_arc: true, flanked: false, patient: 0.0, since_skill: None, status: Status::default() }),
        }
    }

    /// A queue read with `span`: each blow its damage, whether it is a
    /// skill's, and how many milliseconds it has left, judged right
    fn threats(tuning: &Tuning, blows: &[(f32, bool, u64)], span: Duration) -> Threats {
        let plain = ActorAttributes::default();
        let source = Entity::from_raw_u32(9).unwrap();
        let now = Moment::from_millis(10_000);
        let window = threat_window(tuning, &plain, &plain, 0.0);
        let queue: Vec<(Moment, QueuedThreat)> = blows.iter().map(|&(damage, ability, left)| {
            let ability = Some(if ability { AbilityType::Frenzy } else { AbilityType::AutoAttack });
            let threat = create_threat(tuning, source, &plain, &plain, damage, ability, now + Duration::from_millis(left) - window, 0.0, 0.0);
            (threat.lands_at(), threat)
        }).collect();
        Threats::reading(&queue, span, None, now)
    }

    fn scored_by(view: &mut View, reason: &str, mind: &Mind) -> f32 {
        let bar = [view.ability];
        weigh(view, &bar, mind).into_iter().find(|decision| decision.reason == reason).map_or(0.0, |decision| decision.score)
    }

    fn scored(view: &mut View, reason: &str) -> f32 {
        scored_by(view, reason, &Mind::default())
    }

    fn response(view: &mut View, name: &str, mind: &Mind) -> f32 {
        let bar = [view.ability];
        weigh(view, &bar, mind)[0].responses.iter().find(|(named, _)| *named == name).unwrap().1
    }

    #[test]
    fn nothing_queued_nothing_to_answer() {
        let tuning = Tuning::DEFAULT;
        let mut quiet = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        assert!(choose(&mut quiet, &[AbilityType::Counter], &Mind::default(), |_| 0.0).is_none());
    }

    #[test]
    fn a_heavy_blow_is_answered_and_a_light_one_let_land() {
        let tuning = Tuning::DEFAULT;
        let span = Duration::from_millis(250);
        let mut heavy = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        heavy.queue = threats(&tuning, &[(150.0, true, 100)], span);
        let mut light = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        light.queue = threats(&tuning, &[(40.0, false, 100)], span);
        assert!(scored(&mut heavy, "answer") > WAIT, "a blow of a quarter of its health is answered");
        assert!(scored(&mut light, "answer") < WAIT, "one light blow is let land");
    }

    #[test]
    fn a_reaction_waits_for_the_middle_of_its_band() {
        let tuning = Tuning::DEFAULT;
        let span = Duration::from_millis(1000);
        let mut entering = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        entering.queue = threats(&tuning, &[(150.0, true, 900)], span);
        let mut middle = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        middle.queue = threats(&tuning, &[(150.0, true, 400)], span);
        let mut far = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        far.queue = threats(&tuning, &[(150.0, true, 2000)], span);
        assert_eq!(scored(&mut entering, "answer"), 0.0, "a blow just in its band waits for the middle");
        assert!(scored(&mut middle, "answer") > WAIT);
        assert_eq!(scored(&mut far, "answer"), 0.0, "one short of its band has nothing to answer");
        let mut dodge = view(&tuning, AbilityType::Leap, ActorAttributes::default());
        dodge.queue = entering.queue;
        assert_eq!(scored(&mut dodge, "dodge"), 0.0, "and so does a leap clear");
    }

    #[test]
    fn a_threat_is_seen_before_it_reaches_the_widest_band() {
        let tuning = Tuning::DEFAULT;
        let shortest = tuning.reaction_window * (1.0 - tuning.fatigue_window);
        let slowest = crate::behaviour::perception::Skill::SLOPPY.slowest.as_secs_f32();
        let widest = tuning.awareness_core[1];
        assert!(shortest > widest + slowest, "{shortest}s against {}s", widest + slowest);
    }

    #[test]
    fn what_the_gate_refuses_is_never_weighed() {
        let tuning = Tuning::DEFAULT;
        let mut far = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        far.foe = Some(Foe { distance: 3, ..far.foe.unwrap() });
        assert_eq!(scored(&mut far, "strike"), 0.0, "out of reach");
        let mut behind = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        behind.foe = Some(Foe { in_arc: false, ..behind.foe.unwrap() });
        assert_eq!(scored(&mut behind, "strike"), 0.0, "outside its arc");
        let mut recovering = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        recovering.recovery = Some(GlobalRecovery::new(2.0));
        assert_eq!(scored(&mut recovering, "strike"), 0.0, "in a recovery that offers it nothing");
    }

    #[test]
    fn a_strike_prefers_a_foe_that_just_acted() {
        let tuning = Tuning::DEFAULT;
        let mut fresh = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        let mut spent = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        spent.foe = Some(Foe { since_skill: Some(0.3), ..fresh.foe.unwrap() });
        assert!(scored(&mut spent, "strike") > scored(&mut fresh, "strike"));
    }

    #[test]
    fn a_strike_waits_while_its_pack_holds_the_capacity() {
        let tuning = Tuning::DEFAULT;
        let mut full = view(&tuning, AbilityType::Frenzy, ActorAttributes::default());
        full.capacity_taken = true;
        assert_eq!(scored(&mut full, "strike"), 0.0);
        let mut answering = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        answering.capacity_taken = true;
        answering.queue = threats(&tuning, &[(150.0, true, 100)], Duration::from_millis(250));
        assert!(scored(&mut answering, "answer") > WAIT, "a reaction takes no slot");
    }

    #[test]
    fn patience_makes_a_strike_on_an_overcommitted_foe_worth_more_and_most_once_its_opening_stands() {
        let tuning = Tuning::DEFAULT;
        let worth = |steps: i8, stacks: usize| {
            let mut striking = view(&tuning, AbilityType::Feint, built([0, 0, 0, 0, 0, 0, -steps, 0, 0]));
            let mut status = Status::default();
            (0..stacks).for_each(|_| status.overcommit(tuning.overcommit_secs));
            striking.foe = Some(Foe { status, ..striking.foe.unwrap() });
            response(&mut striking, "strike_worth", &Mind::default())
        };
        assert!(worth(3, 4) > worth(3, 0), "likelier to crit");
        assert!(worth(9, 4) > worth(3, 4), "and harder at its facet");
        let at = tuning.patience_opening[0];
        assert!(worth(18, at) > worth(15, at), "certain once its capstone's stacks stand");
    }

    #[test]
    fn a_toll_or_an_answer_to_many_moves_what_a_skill_leaves_of_the_pool() {
        let tuning = Tuning::DEFAULT;
        let mut free = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        (free.endurance, free.endurance_max) = (10.0, 10.0);
        let mut taxed = free.clone();
        taxed.status.tax(1.5, 5.0);
        assert!(response(&mut taxed, "endurance_left", &Mind::default()) < response(&mut free, "endurance_left", &Mind::default()), "a toll leaves less");
        let three = threats(&tuning, &[(50.0, true, 100), (50.0, true, 150), (50.0, true, 200)], Duration::from_millis(900));
        let mut plain = view(&tuning, AbilityType::Parry, ActorAttributes::default());
        (plain.endurance, plain.endurance_max) = (20.0, 20.0);
        plain.queue = three;
        let mut faceted = view(&tuning, AbilityType::Parry, built([0, 0, 0, 0, 0, 0, 9, 0, 0]));
        (faceted.endurance, faceted.endurance_max) = (20.0, 20.0);
        faceted.queue = three;
        assert!(response(&mut faceted, "endurance_left", &Mind::default()) > response(&mut plain, "endurance_left", &Mind::default()), "answering many with one costs less at the facet");
    }

    #[test]
    fn the_combo_its_recovery_offers_beats_waiting_where_it_would_not_alone() {
        let tuning = Tuning::DEFAULT;
        let mut offered = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        offered.recovery = Some(common_bevy::systems::combat::combos::recovery_after(&tuning, AbilityType::Parry, true, None, &offered.attrs, None, 0.0));
        let combo = offered.recovery.unwrap().combo.unwrap();
        assert_eq!(combo.ability, AbilityType::Feint);
        offered.recovery.as_mut().unwrap().remaining = combo.unlock_at;
        let alone = scored(&mut offered, "strike");
        let mut mind = Mind::default();
        mind.wait = alone + 0.01;
        let chosen = choose(&mut offered, &[AbilityType::Feint], &mind, |_| 0.0);
        assert!(chosen.is_some_and(|decision| decision.ability == AbilityType::Feint), "carried on past a wait it would not beat alone");
        let mut fresh = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        assert!(choose(&mut fresh, &[AbilityType::Feint], &mind, |_| 0.0).is_none(), "and out of recovery the same strike waits");
    }

    #[test]
    fn a_counter_is_worth_more_than_a_parry_by_what_it_returns() {
        let tuning = Tuning::DEFAULT;
        let queue = threats(&tuning, &[(60.0, true, 100)], Duration::from_millis(250));
        let mut parry = view(&tuning, AbilityType::Parry, ActorAttributes::default());
        parry.queue = queue;
        let mut counter = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        counter.queue = queue;
        assert!(scored(&mut counter, "answer") > scored(&mut parry, "answer"));
    }

    #[test]
    fn a_punish_is_worth_more_for_each_stack_its_foe_is_overcommitted() {
        let tuning = Tuning::DEFAULT;
        let mind = Mind::default();
        let overcommitted = |view: &mut View, stacks: usize| {
            let mut foe = view.foe.unwrap();
            (0..stacks).for_each(|_| foe.status.overcommit(tuning.overcommit_secs));
            view.foe = Some(foe);
        };
        let worth = |ability, attrs, stacks| {
            let mut struck = view(&tuning, ability, attrs);
            overcommitted(&mut struck, stacks);
            response(&mut struck, "strike_worth", &mind)
        };
        let plain = ActorAttributes::default();
        assert!(worth(AbilityType::Punish, plain, 1) > worth(AbilityType::Punish, plain, 0));
        assert!(worth(AbilityType::Punish, plain, 4) > worth(AbilityType::Punish, plain, 1));
        assert_eq!(worth(AbilityType::Feint, plain, 4), worth(AbilityType::Feint, plain, 0), "without Patience, only Punish pays for the stacks");
        let patient = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        assert!(worth(AbilityType::Feint, patient, 4) > worth(AbilityType::Feint, patient, 0), "a patient striker's skills crit them likelier");
    }

    #[test]
    fn a_strike_into_a_patient_foe_is_dearer_the_more_it_is_overcommitted() {
        let tuning = Tuning::DEFAULT;
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.exposure.floor", "0.0").unwrap();
        let mind = minds.mind(None);
        let mut calm = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        calm.foe = Some(Foe { patient: 0.09, ..calm.foe.unwrap() });
        let mut pressed = calm.clone();
        (0..5).for_each(|_| pressed.status.overcommit(tuning.overcommit_secs));
        assert!(response(&mut pressed, "exposure", &mind) < response(&mut calm, "exposure", &mind));
        let mut idle = calm.clone();
        idle.foe = Some(Foe { patient: 0.0, ..idle.foe.unwrap() });
        assert_eq!(response(&mut idle, "exposure", &mind), 1.0, "a foe without Patience exposes it to nothing");
        assert_eq!(response(&mut pressed, "exposure", &Mind::default()), 1.0, "weighed only as far as its mind lowers the floor");
    }

    #[test]
    fn a_heavier_strike_and_a_weaker_foe_are_worth_more() {
        let tuning = Tuning::DEFAULT;
        // Committed to Physique, Overpower's line, so it strikes whole
        let vital = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let mind = Mind::default();
        let (mut light, mut heavy) = (view(&tuning, AbilityType::Feint, vital), view(&tuning, AbilityType::Overpower, vital));
        assert!(response(&mut heavy, "strike_worth", &mind) > response(&mut light, "strike_worth", &mind));
        assert!(scored(&mut heavy, "strike") > scored(&mut light, "strike"), "and outweighs what it costs more");
        let mut finishing = view(&tuning, AbilityType::Feint, vital);
        finishing.foe = Some(Foe { health: 20.0, ..finishing.foe.unwrap() });
        assert!(scored(&mut finishing, "strike") > scored(&mut light, "strike"), "a blow that nears the kill");
    }

    #[test]
    fn a_leap_clears_a_foe_in_reach_and_dives_onto_one_out_of_it() {
        let tuning = Tuning::DEFAULT;
        let reasons = |distance| {
            let mut v = view(&tuning, AbilityType::Leap, ActorAttributes::default());
            v.foe = Some(Foe { distance, ..v.foe.unwrap() });
            weigh(&mut v, &[AbilityType::Leap], &Mind::default()).iter().map(|decision| decision.reason).collect::<Vec<_>>()
        };
        assert_eq!(reasons(1), vec!["dodge"]);
        assert_eq!(reasons(6), vec!["dive"]);
        let mut landing = view(&tuning, AbilityType::Leap, ActorAttributes::default());
        landing.foe = Some(Foe { distance: landing.leap + landing.reach, ..landing.foe.unwrap() });
        let mut short = landing.clone();
        short.foe = Some(Foe { distance: short.leap + short.reach + 1, ..short.foe.unwrap() });
        assert!(scored(&mut short, "dive") < scored(&mut landing, "dive"), "a dive that falls short strikes nothing");
        let mut plain = view(&tuning, AbilityType::Leap, ActorAttributes::default());
        assert!(choose(&mut plain, &[AbilityType::Leap], &Mind::default(), |_| 0.0).is_none(), "in reach with nothing queued, it stays");
    }

    #[test]
    fn a_leap_toward_its_leash_scores_below_one_back_inward() {
        let tuning = Tuning::DEFAULT;
        let mut inward = view(&tuning, AbilityType::Leap, ActorAttributes::default());
        inward.foe = Some(Foe { distance: 6, ..inward.foe.unwrap() });
        let mut outward = inward.clone();
        outward.leap_room = 0.05;
        assert!(scored(&mut inward, "dive") > scored(&mut outward, "dive"));
        outward.leap_room = 0.0;
        assert_eq!(scored(&mut outward, "dive"), 0.0, "with nowhere to land it does not leap");
    }

    #[test]
    fn an_effect_is_put_on_by_how_much_of_it_is_new() {
        let tuning = Tuning::DEFAULT;
        let graceful = built([10, 0, 0, 0, 0, 0, 0, 0, 0]);
        let fresh = view(&tuning, AbilityType::PerfectStride, graceful);
        let at = |remaining: f32| {
            let mut v = fresh.clone();
            v.status.perfect_stride = Some(Timed { pace: 1.1, remaining });
            scored(&mut v, "stride")
        };
        assert!(scored(&mut fresh.clone(), "stride") > WAIT);
        assert!(at(tuning.stride_secs * 0.2) > at(tuning.stride_secs * 0.8), "the nearer it runs out, the more refreshing it adds");
        assert_eq!(at(tuning.stride_secs), 0.0, "a fresh one adds nothing");
        let mut far = fresh.clone();
        far.foe = Some(Foe { distance: 5, ..far.foe.unwrap() });
        assert_eq!(scored(&mut far, "stride"), 0.0, "and none is taken with no foe in reach");
    }
}
