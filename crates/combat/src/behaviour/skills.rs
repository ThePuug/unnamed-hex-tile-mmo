//! An NPC's skills channel: which of its skills it uses now, if any.
//!
//! Every skill on its bar brings a decision for what it does now (a strike,
//! a reaction, a leap clear or in as its range has it, an effect it puts
//! on someone), and each decision brings considerations from what it does
//! and from the commitments its user holds. A commitment's considerations
//! read its own state, so an NPC that scores them plays the way its
//! commitment pays: no style is written down. Every skill's worth is
//! weighed against both its costs, the stamina and the recovery it leaves,
//! and no decision is weighed that the gate would refuse
//! ([`admits`]). The channel's do-nothing decision, waiting, scores
//! [`WAIT`], and a skill is used only where it scores higher; the combo its
//! recovery offers scores [`COMBO`] more, so a chain carries on rather than
//! wait for another skill to unlock.
//!
//! Every input is a ratio in the game's own terms, so a curve means the
//! same at any level and holds when a skill's numbers change.

use std::time::Duration;

use common_bevy::{
    components::{reaction_queue::QueuedThreat, recovery::GlobalRecovery, resources::Endurance, status::Status, ActorAttributes},
    message::AbilityType,
    systems::combat::combos::recovery_after,
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
    "fatigue_after", "stamina_left", "recovery_left", "foe_just_acted", "worth_answering",
    "leash_left", "strike_worth", "effect_added", "reactions_left", "foe_across",
];

/// `stamina_left`'s bounds and curve, which the movement channel's reads by
/// the same name share: one setting shapes both
pub const STAMINA_LEFT_BOUNDS: (f32, f32) = (0.0, 0.5);
pub const STAMINA_LEFT_CURVE: Curve = Curve::RISING.floored(1.0);

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
    pub stamina: f32,
    pub endurance: f32,
    pub endurance_max: f32,
    pub recovery: Option<GlobalRecovery>,
    /// Its own timed effects
    pub status: Status,
    /// How full its Grit's bank is, of what it holds full
    pub grit_filled: f32,
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
    /// Its foe's last skill falls inside the window its mind counts a skill
    /// as just used, how long it takes a foe to be recovering
    /// ([`weigh`] sets it)
    pub foe_recovering: bool,
}

/// Its queue, as a reaction would meet it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Threats {
    /// Damage a reaction would take now: the front threat and its span
    pub swept: f32,
    /// Of that, what the blows deal on landing, apart from what their
    /// DoTs have left: what a reflection returns a share of
    pub swept_direct: f32,
    /// How long ago the front threat was queued, against its user's span:
    /// a threat queued after the span closes lands past it
    pub span_closed: f32,
}

impl Threats {
    /// What a reaction reaching `span` behind the front of `queue`, a
    /// queue's threats in order, would take at `now`, the game's clock.
    pub fn reading(queue: &[QueuedThreat], span: Duration, now: Duration) -> Self {
        let Some(front) = queue.first() else { return Self::default() };
        let (first, last) = (front.lands_at(), front.lands_at() + span);
        let swept = queue.iter().filter(|threat| (first..=last).contains(&threat.lands_at()));
        let (mut damage, mut direct) = (0.0, 0.0);
        for threat in swept {
            let blow = threat.damage + threat.dot_left();
            damage += blow;
            direct += threat.damage;
        }
        let since = now.saturating_sub(front.inserted_at).as_secs_f32();
        Self {
            swept: damage,
            swept_direct: direct,
            span_closed: if span.is_zero() { 1.0 } else { since / span.as_secs_f32() },
        }
    }
}

/// Its target, as it sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foe {
    pub distance: i32,
    /// The health it has left, as its target frame shows it
    pub health: f32,
    /// Within the arc it strikes within
    pub in_arc: bool,
    /// Past its forward faces, where a swing breaks its stride
    pub across: bool,
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
    let window = mind.shape(&FOE_JUST_ACTED).bounds.0;
    view.foe_recovering = view.foe.and_then(|foe| foe.since_skill).is_some_and(|since| since < window);
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
    let mut considerations = vec![USABLE, ENDURANCE, STAMINA_LEFT, RECOVERY_LEFT];
    considerations.extend(part_considerations(part));
    considerations.extend(commitment_considerations(part, &view.attrs));
    if effect(view).is_some() {
        considerations.push(EFFECT_ADDED);
    }
    Some((reason, considerations))
}

fn part_considerations(part: Part) -> Vec<Considered> {
    match part {
        Part::Strike => vec![FOE_JUST_ACTED, CAPACITY, STRIKE_WORTH],
        Part::Reaction => vec![SPAN_CLOSED, WORTH_ANSWERING],
        Part::Clear => vec![SPAN_CLOSED, WORTH_ANSWERING, LEASH_LEFT],
        Part::Dive => vec![CAPACITY, STRIKE_WORTH, LEASH_LEFT],
        Part::Effect => vec![IN_REACH],
    }
}

fn commitment_considerations(part: Part, attrs: &ActorAttributes) -> Vec<Considered> {
    let mut considerations = Vec::new();
    if attrs.preparation().index() > 0 && part == Part::Reaction {
        considerations.push(REACTIONS_LEFT);
    }
    if attrs.grace().index() > 0 && part == Part::Effect {
        considerations.push(FOE_ACROSS);
    }
    considerations
}

/// A timed effect a decision puts on someone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Effect {
    /// Perfect Stride, on itself
    PerfectStride,
    /// The bind a full Grit bank releases into a strike, a slow on its foe
    Bind,
}

impl Effect {
    /// Seconds a fresh one lasts
    fn lasts(self, tuning: &Tuning) -> f32 {
        match self {
            Effect::PerfectStride => tuning.stride_secs,
            Effect::Bind => tuning.grit_bind_secs,
        }
    }

    /// Seconds left of the same effect on whoever it lands on
    fn left(self, view: &View) -> f32 {
        let timed = match self {
            Effect::PerfectStride => view.status.perfect_stride,
            Effect::Bind => view.foe.and_then(|foe| foe.status.slow),
        };
        timed.map_or(0.0, |timed| timed.remaining.max(0.0))
    }
}

/// The timed effect `view.ability` would put on someone now: Perfect
/// Stride's on itself, or the bind of a full Grit bank on whatever a skill
/// strikes. None for a decision that puts on none.
fn effect(view: &View) -> Option<Effect> {
    match view.ability {
        AbilityType::PerfectStride => Some(Effect::PerfectStride),
        ability if strikes(ability, view) && view.grit_filled >= 1.0 => Some(Effect::Bind),
        _ => None,
    }
}

/// Whether `ability` strikes its foe now: a strike, or a Leap onto a foe
/// out of reach
fn strikes(ability: AbilityType, view: &View) -> bool {
    ability.reach(view.reach).is_some() || (ability == AbilityType::Leap && view.foe.is_some_and(|foe| foe.distance > view.reach))
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
/// foe within its reach and arc for one that strikes, and its stamina
const USABLE: Considered = step("usable", |view| {
    let foe = view.foe.map(|foe| (foe.distance, foe.in_arc));
    flag(admits(view.ability, view.recovery.as_ref(), &view.attrs, view.reach, foe, view.stamina, view.tuning.cost(view.ability)).is_ok())
});

/// The fatigue it would be left with once it paid the skill's endurance:
/// fatigue bites as the pool empties, lengthening every recovery,
/// shortening every window against it and slowing its stamina, so a
/// skill that spends it near empty must be worth the more
const ENDURANCE: Considered = Consideration {
    name: "fatigue_after",
    read: |view| {
        let price = match view.ability {
            ability if ability.is_reaction() => view.attrs.skill_endurance(&view.tuning, ability) + view.attrs.reaction_effort(&view.tuning, view.queue.swept),
            ability => view.attrs.skill_endurance(&view.tuning, ability),
        };
        Endurance { state: (view.endurance - price).max(0.0), max: view.endurance_max }.fatigue(&view.tuning)
    },
    bounds: (0.0, 1.0),
    curve: Curve::FALLING.floored(0.1),
};

/// The stamina it would have left once it paid, of its most: what it keeps
/// for whatever it does next. Weighed only as far as a mind lowers its floor
const STAMINA_LEFT: Considered = Consideration {
    name: "stamina_left",
    read: |view| (view.stamina - view.tuning.cost(view.ability)) / view.attrs.max_stamina(&view.tuning).max(1.0),
    bounds: STAMINA_LEFT_BOUNDS,
    curve: STAMINA_LEFT_CURVE,
};

/// Seconds of recovery the skill would leave it in, as its fatigue, a combo
/// fired early or a reaction through a recovery make them: the time it
/// spends. Weighed only as far as a mind lowers its floor
const RECOVERY_LEFT: Considered = Consideration {
    name: "recovery_left",
    read: |view| {
        let fatigue = Endurance { state: view.endurance, max: view.endurance_max }.fatigue(&view.tuning);
        let after = recovery_after(&view.tuning, view.ability, view.recovery.as_ref(), &view.attrs, None, fatigue);
        after.remaining + after.chain.owed
    },
    bounds: (0.0, 6.0),
    curve: Curve::FALLING.floored(1.0),
};

/// How much of the timed effect it would put on someone is new: what a
/// fresh one lasts less what is left of the same on whoever it lands on,
/// over what a fresh one lasts. Nothing new vetoes it
const EFFECT_ADDED: Considered = Consideration {
    name: "effect_added",
    read: |view| effect(view).map_or(1.0, |effect| {
        let lasts = effect.lasts(&view.tuning).max(f32::EPSILON);
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

/// What a strike deals, as a share of the health its foe has left: how much
/// nearer it brings the kill, a finishing blow most. More by Grit's share
/// with a full bank to release into it, and by Punish's bonus on a foe it
/// takes to be recovering. A leap that falls short of its foe deals
/// nothing, and is weighed for the ground it closes only as far as the
/// floor lets it
const STRIKE_WORTH: Considered = Consideration {
    name: "strike_worth",
    read: |view| {
        let Some(foe) = view.foe else { return 0.0 };
        if view.ability == AbilityType::Leap && foe.distance > view.leap + view.reach {
            return 0.0;
        }
        let tuning = &view.tuning;
        let release = if view.grit_filled >= 1.0 { 1.0 + tuning.grit_share } else { 1.0 };
        let punish = if view.ability == AbilityType::Punish { punish::weight(tuning, view.foe_recovering) } else { 1.0 };
        let dealt = view.attrs.base_potency(tuning) * tuning.damage(view.ability) * view.attrs.line_power(tuning, view.ability) * release * punish;
        dealt / foe.health.max(1.0)
    },
    bounds: (0.0, 0.25),
    curve: Curve::RISING.floored(0.3),
};

// --- Reaction, and a leap clear to dodge ---

/// What it would clear, and what it would return of that, over its health
const WORTH_ANSWERING: Considered = Consideration {
    name: "worth_answering",
    read: |view| {
        let returned = view.queue.swept_direct * returned(&view.tuning, view.ability) * view.attrs.line_power(&view.tuning, view.ability);
        (view.queue.swept + returned) / health(view)
    },
    bounds: (0.0, WORTH),
    curve: Curve { shape: Shape::Logistic { mid: 0.4, steep: 8.0 }, falling: false, floor: 0.0 },
};

/// Its span has closed: a threat queued after it lands past it, so no
/// threat still to come can join the answer. Every threat's window outlasts
/// the widest span and the slowest reaction delay, so waiting for it never
/// lets the front threat land
const SPAN_CLOSED: Considered = step("span_closed", |view| flag(view.queue.span_closed >= 1.0));

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

// --- Commitments ---

/// Preparation: reactions it may still fire early in this chain
const REACTIONS_LEFT: Considered = Consideration {
    name: "reactions_left",
    read: |view| {
        let tier = view.attrs.preparation().index() as f32;
        match view.recovery.as_ref().filter(|recovery| recovery.is_active()) {
            Some(recovery) => (tier - recovery.chain.early_reactions as f32) / tier.max(1.0),
            None => 1.0,
        }
    },
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.6),
};

/// Grace: a foe past its forward faces, struck there only in a stride
const FOE_ACROSS: Considered = Consideration {
    name: "foe_across",
    read: |view| flag(view.foe.is_some_and(|foe| foe.across && foe.in_arc)),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.3),
};

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;
    use common_bevy::{components::status::Timed, systems::combat::queue::create_threat};

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
            stamina: 100.0,
            // A pool deep enough to clear the blows these tests queue
            endurance: attrs.max_endurance(tuning).max(1000.0),
            endurance_max: attrs.max_endurance(tuning).max(1000.0),
            recovery: None,
            status: Status::default(),
            grit_filled: 0.0,
            reach: 2,
            leap: 9,
            leap_room: 1.0,
            capacity_taken: false,
            queue: Threats::default(),
            foe: Some(Foe { distance: 1, health: 600.0, in_arc: true, across: false, since_skill: None, status: Status::default() }),
            foe_recovering: false,
        }
    }

    fn threats(tuning: &Tuning, blows: &[(f32, bool, u64)], span: Duration, now: Duration) -> Threats {
        let plain = ActorAttributes::default();
        let source = Entity::from_raw_u32(9).unwrap();
        let queue: Vec<QueuedThreat> = blows.iter().map(|&(damage, ability, queued)| {
            let ability = Some(if ability { AbilityType::Frenzy } else { AbilityType::AutoAttack });
            create_threat(tuning, source, &plain, &plain, damage, ability, Duration::from_millis(queued), 0.0, 0.0)
        }).collect();
        Threats::reading(&queue, span, now)
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
        let now = Duration::from_millis(1000);
        let mut heavy = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        heavy.queue = threats(&tuning, &[(150.0, true, 0)], span, now);
        let mut light = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        light.queue = threats(&tuning, &[(40.0, false, 0)], span, now);
        assert!(scored(&mut heavy, "answer") > WAIT, "a blow of a quarter of its health is answered");
        assert!(scored(&mut light, "answer") < WAIT, "one light blow is let land");
    }

    #[test]
    fn a_reaction_waits_for_its_span_to_close() {
        let tuning = Tuning::DEFAULT;
        let span = Duration::from_millis(1000);
        let mut fresh = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        fresh.queue = threats(&tuning, &[(150.0, true, 0)], span, Duration::from_millis(900));
        let mut closed = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        closed.queue = threats(&tuning, &[(150.0, true, 0)], span, Duration::from_millis(1100));
        assert_eq!(scored(&mut fresh, "answer"), 0.0, "a blow just queued waits for what may join it");
        assert!(scored(&mut closed, "answer") > WAIT);
        let mut dodge = view(&tuning, AbilityType::Leap, ActorAttributes::default());
        dodge.queue = fresh.queue;
        assert_eq!(scored(&mut dodge, "dodge"), 0.0, "and so does a leap clear");
    }

    #[test]
    fn every_window_outlasts_the_widest_span_and_the_slowest_reaction() {
        let tuning = Tuning::DEFAULT;
        let shortest = tuning.reaction_window * (1.0 - tuning.fatigue_window);
        let slowest = crate::behaviour::perception::Skill::SLOPPY.slowest.as_secs_f32();
        assert!(shortest > tuning.awareness_span_max + slowest, "{shortest}s against {}s", tuning.awareness_span_max + slowest);
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
        let mut poor = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        poor.stamina = 0.0;
        assert_eq!(scored(&mut poor, "strike"), 0.0, "what it cannot afford");
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
        answering.queue = threats(&tuning, &[(150.0, true, 0)], Duration::from_millis(250), Duration::from_millis(1000));
        assert!(scored(&mut answering, "answer") > WAIT, "a reaction takes no slot");
    }

    #[test]
    fn a_skill_spent_near_empty_must_be_worth_more_than_one_spent_from_a_full_pool() {
        let tuning = Tuning::DEFAULT;
        let mut full = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        let mut low = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        let price = low.attrs.skill_endurance(&tuning, AbilityType::Feint);
        low.endurance = price * 1.5;
        let mut half = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        half.endurance = half.endurance_max / 2.0;
        let (full, half, low) = (scored(&mut full, "strike"), scored(&mut half, "strike"), scored(&mut low, "strike"));
        assert!(full - half < half - low, "half a pool costs little; the last of it costs much: {full} {half} {low}");
        assert!(low < WAIT, "near empty, a plain strike is not worth it");
    }

    #[test]
    fn a_full_grit_bank_makes_a_strike_worth_more() {
        let tuning = Tuning::DEFAULT;
        let gritty = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let mut empty = view(&tuning, AbilityType::Feint, gritty);
        let mut full = view(&tuning, AbilityType::Feint, gritty);
        full.grit_filled = 1.0;
        assert!(response(&mut full, "strike_worth", &Mind::default()) > response(&mut empty, "strike_worth", &Mind::default()));
    }

    #[test]
    fn a_release_is_not_spent_binding_a_foe_already_bound() {
        let tuning = Tuning::DEFAULT;
        let gritty = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let mut loose = view(&tuning, AbilityType::Feint, gritty);
        loose.grit_filled = 1.0;
        let mut bound = loose.clone();
        bound.foe = Some(Foe { status: Status { slow: Some(Timed { pace: 0.8, remaining: tuning.grit_bind_secs }), ..Status::default() }, ..bound.foe.unwrap() });
        assert!(scored(&mut loose, "strike") > 0.0);
        assert_eq!(scored(&mut bound, "strike"), 0.0, "freshly bound, a release would add nothing");
        let mut unbanked = view(&tuning, AbilityType::Feint, gritty);
        unbanked.foe = bound.foe;
        assert!(scored(&mut unbanked, "strike") > 0.0, "a strike that releases nothing puts no bind on");
    }

    #[test]
    fn the_combo_its_recovery_offers_beats_waiting_where_it_would_not_alone() {
        let tuning = Tuning::DEFAULT;
        let mut offered = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        offered.recovery = Some(recovery_after(&tuning, AbilityType::Parry, None, &offered.attrs, None, 0.0));
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
    fn a_mind_keeping_stamina_back_takes_the_cheaper_strike_when_short() {
        let tuning = Tuning::DEFAULT;
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.stamina_left.floor", "0").unwrap();
        let mind = minds.mind(None);
        let attrs = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let short = tuning.cost(AbilityType::Overpower) + attrs.max_stamina(&tuning) * 0.1;
        let shape = |ability, stamina| {
            let mut v = view(&tuning, ability, attrs);
            v.stamina = stamina;
            response(&mut v, "stamina_left", &mind)
        };
        assert!(shape(AbilityType::Overpower, short) < shape(AbilityType::Feint, short), "the dearer strike leaves less behind");
        assert_eq!(shape(AbilityType::Overpower, attrs.max_stamina(&tuning)), shape(AbilityType::Feint, attrs.max_stamina(&tuning)), "with a full pool, neither is held back");
    }

    #[test]
    fn a_mind_weighing_time_marks_a_long_recovery_down() {
        let tuning = Tuning::DEFAULT;
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.recovery_left.floor", "0").unwrap();
        let mind = minds.mind(None);
        let (quick, slow) = if tuning.recovery(AbilityType::Feint) < tuning.recovery(AbilityType::Overpower) {
            (AbilityType::Feint, AbilityType::Overpower)
        } else {
            (AbilityType::Overpower, AbilityType::Feint)
        };
        let at = |ability| response(&mut view(&tuning, ability, ActorAttributes::default()), "recovery_left", &mind);
        assert!(at(slow) < at(quick));
    }

    #[test]
    fn a_counter_is_worth_more_than_a_parry_by_what_it_returns() {
        let tuning = Tuning::DEFAULT;
        let span = Duration::from_millis(250);
        let now = Duration::from_millis(1000);
        let queue = threats(&tuning, &[(60.0, true, 0)], span, now);
        let mut parry = view(&tuning, AbilityType::Parry, ActorAttributes::default());
        parry.queue = queue;
        let mut counter = view(&tuning, AbilityType::Counter, ActorAttributes::default());
        counter.queue = queue;
        assert!(scored(&mut counter, "answer") > scored(&mut parry, "answer"));
    }

    #[test]
    fn a_punish_is_worth_more_on_a_foe_its_mind_takes_to_be_recovering() {
        let tuning = Tuning::DEFAULT;
        let mut fresh = view(&tuning, AbilityType::Punish, ActorAttributes::default());
        let mut opened = fresh.clone();
        opened.foe = Some(Foe { since_skill: Some(0.2), ..opened.foe.unwrap() });
        let mind = Mind::default();
        assert!(response(&mut opened, "strike_worth", &mind) > response(&mut fresh, "strike_worth", &mind));
        let mut feint = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        feint.foe = opened.foe;
        let mut plain = view(&tuning, AbilityType::Feint, ActorAttributes::default());
        assert_eq!(response(&mut feint, "strike_worth", &mind), response(&mut plain, "strike_worth", &mind), "only Punish pays for the opening");
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.foe_just_acted.from", "0.1").unwrap();
        let brief = minds.mind(None);
        assert_eq!(response(&mut opened, "strike_worth", &brief), response(&mut fresh, "strike_worth", &brief), "past the window its mind counts, no opening");
    }

    #[test]
    fn a_heavier_strike_and_a_weaker_foe_are_worth_more() {
        let tuning = Tuning::DEFAULT;
        // Committed to Vitality, Overpower's line, so it strikes whole
        let vital = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let mind = Mind::default();
        let (mut light, mut heavy) = (view(&tuning, AbilityType::Feint, vital), view(&tuning, AbilityType::Overpower, vital));
        assert!(response(&mut heavy, "strike_worth", &mind) > response(&mut light, "strike_worth", &mind));
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
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.strike_worth.floor", "0").unwrap();
        assert_eq!(scored_by(&mut short, "dive", &minds.mind(None)), 0.0, "and a mind that wants a blow from it never takes one");
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
        let mut fresh = view(&tuning, AbilityType::PerfectStride, graceful);
        fresh.foe = Some(Foe { across: true, ..fresh.foe.unwrap() });
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
