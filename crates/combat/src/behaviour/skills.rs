//! An NPC's skills channel: which of its skills it uses now, if any.
//!
//! Every skill on its bar brings one decision for each reason to use it,
//! and each decision brings considerations from what the skill does (a
//! strike, a reaction, a leap clear or in, a stance) and from the
//! commitments its user holds. A commitment's considerations read its own
//! state, so an NPC that scores them plays the way its commitment pays:
//! no style is written down. Every skill's worth is weighed against both
//! its costs, the stamina and the recovery it leaves. The channel's
//! do-nothing decision, waiting, scores [`WAIT`], and a skill is used only
//! where it scores higher; the combo its recovery offers scores
//! [`COMBO`] more, so a chain carries on rather than wait for another
//! skill to unlock.
//!
//! Every input is a ratio in the game's own terms, so a curve means the
//! same at any level and holds when a skill's numbers change.

use std::time::Duration;

use common_bevy::{
    components::{reaction_queue::QueuedThreat, recovery::GlobalRecovery, resources::Endurance, ActorAttributes},
    message::AbilityType,
    systems::combat::combos::{may_use, reacts_through, recovery_after},
};

use super::{mind::Mind, utility::{score, Consideration, Curve, Shape}};

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
    "room_to_land", "grit_banked", "strike_worth", "opening",
    "reactions_left", "stamina_spent", "stamina_ready", "foe_across",
];

/// The blow, as a share of its own health, a reaction or a leap clear
/// counts as fully worth answering.
const WORTH: f32 = 0.3;

/// What an NPC perceives as it weighs its skills, and the skill it weighs.
#[derive(Clone, Debug)]
pub struct View {
    pub ability: AbilityType,
    pub attrs: ActorAttributes,
    pub health: f32,
    pub stamina: f32,
    pub endurance: f32,
    pub endurance_max: f32,
    pub recovery: Option<GlobalRecovery>,
    /// Whether a Perfect Stride holds
    pub striding: bool,
    /// How full its Grit's bank is, of what it holds full
    pub grit_filled: f32,
    /// Its swing clock runs, engaged, so its Patience pays
    pub engaged: bool,
    /// Its own reach, in tiles
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    /// Share of its leash it would have left where a leap clear of its
    /// target lands: 1 with no leash, 0 with nowhere to land
    pub clear_room: f32,
    /// Its pack's attack capacity on its target is taken: as many others of
    /// its engagement have an ability standing in the target's queue
    pub capacity_taken: bool,
    pub queue: Threats,
    pub foe: Option<Foe>,
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
        for (reason, considerations) in reasons(ability, view) {
            let responses: Vec<(&'static str, f32)> = considerations.iter()
                .map(|consideration| (consideration.name, mind.shape(consideration).answer(view)))
                .collect();
            let score = score(1.0, responses.iter().map(|&(_, response)| response));
            decisions.push(Decision { ability, reason, score, responses });
        }
    }
    decisions
}

type Considered = Consideration<View>;

/// What one part of a skill does, each bringing its considerations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Part {
    Strike,
    Reaction,
    /// A leap clear of a foe in reach
    Clear,
    /// A leap onto a foe out of reach
    Dive,
    /// A state its user holds for a while
    Stance,
}

/// Each reason to use `ability`, and its considerations for what `view`
/// perceives: what any skill asks, what its part asks, and what the
/// commitments its user holds ask of that part. Patience asks only while
/// it is engaged.
fn reasons(ability: AbilityType, view: &View) -> Vec<(&'static str, Vec<Considered>)> {
    let patient = view.engaged && view.attrs.patience().index() > 0;
    let parts: &[(&'static str, Part)] = match ability {
        AbilityType::AutoAttack => &[],
        AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower => &[("strike", Part::Strike)],
        AbilityType::Punish => &[("punish", Part::Strike)],
        AbilityType::Parry | AbilityType::Counter => &[("answer", Part::Reaction)],
        AbilityType::Leap => &[("dodge", Part::Clear), ("recover", Part::Clear), ("dive", Part::Dive)],
        AbilityType::PerfectStride => &[("stride", Part::Stance)],
    };
    parts.iter()
        .filter(|&&(reason, _)| reason != "recover" || patient)
        .map(|&(reason, part)| {
            let mut considerations = vec![OPEN, AFFORDABLE, ENDURANCE, STAMINA_LEFT, RECOVERY_LEFT];
            considerations.extend(part_considerations(reason, part));
            considerations.extend(commitment_considerations(reason, part, &view.attrs, patient));
            (reason, considerations)
        })
        .collect()
}

fn part_considerations(reason: &str, part: Part) -> Vec<Considered> {
    match (part, reason) {
        (Part::Strike, "punish") => vec![FOE_STRUCK, FOE_JUST_ACTED, CAPACITY, STRIKE_WORTH, OPENING],
        (Part::Strike, _) => vec![FOE_STRUCK, FOE_JUST_ACTED, CAPACITY, STRIKE_WORTH],
        (Part::Reaction, _) => vec![SPAN_CLOSED, WORTH_ANSWERING],
        (Part::Clear, "dodge") => vec![FOE_IN_REACH, SPAN_CLOSED, WORTH_ANSWERING, ROOM_TO_LAND],
        (Part::Clear, _) => vec![FOE_IN_REACH, ROOM_TO_LAND],
        (Part::Dive, _) => vec![FOE_OUT_OF_REACH, FOE_WITHIN_A_DIVE, CAPACITY, STRIKE_WORTH],
        (Part::Stance, _) => vec![NOT_STRIDING, FOE_IN_REACH],
    }
}

fn commitment_considerations(reason: &str, part: Part, attrs: &ActorAttributes, patient: bool) -> Vec<Considered> {
    let mut considerations = Vec::new();
    if attrs.grit_fill() > 0 && part == Part::Strike {
        considerations.push(GRIT_BANKED);
    }
    if attrs.preparation().index() > 0 && part == Part::Reaction {
        considerations.push(REACTIONS_LEFT);
    }
    if patient {
        match (part, reason) {
            (Part::Clear, "recover") => considerations.push(STAMINA_SPENT),
            (Part::Dive, _) => considerations.push(STAMINA_READY),
            _ => {}
        }
    }
    if attrs.grace().index() > 0 && part == Part::Stance {
        considerations.push(FOE_ACROSS);
    }
    considerations
}

const fn step(name: &'static str, read: fn(&View) -> f32) -> Considered {
    Consideration { name, read, bounds: (1.0, 1.0), curve: Curve::RISING }
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

fn foe_distance(view: &View) -> Option<i32> {
    view.foe.map(|foe| foe.distance)
}

/// The share of each blow it clears that `ability` sends back to the blow's
/// source: Counter's reflection, and nothing for any other
fn returned(ability: AbilityType) -> f32 {
    match ability {
        AbilityType::Counter => common_bevy::tuning::tuning().counter_reflect,
        _ => 0.0,
    }
}

/// Its health, never so low a ratio over it runs away
fn health(view: &View) -> f32 {
    view.health.max(1.0)
}

// --- Any skill ---

/// The gate would let it through its recovery
const OPEN: Considered = step("open", |view| {
    let recovery = view.recovery.as_ref().filter(|recovery| recovery.is_active());
    flag(may_use(view.ability, recovery) || reacts_through(view.ability, recovery, Some(&view.attrs)))
});

/// It has the stamina; endurance refuses nothing, and is weighed as the
/// fatigue it leaves
const AFFORDABLE: Considered = step("affordable", |view| {
    flag(view.stamina >= common_bevy::tuning::tuning().cost(view.ability))
});

/// The fatigue it would be left with once it paid the skill's endurance:
/// fatigue bites as the pool empties, lengthening every recovery,
/// shortening every window against it and slowing its stamina, so a
/// skill that spends it near empty must be worth the more
const ENDURANCE: Considered = Consideration {
    name: "fatigue_after",
    read: |view| {
        let price = match view.ability {
            ability if ability.is_reaction() => view.attrs.skill_endurance(ability) + view.attrs.reaction_effort(view.queue.swept),
            ability => view.attrs.skill_endurance(ability),
        };
        Endurance { state: (view.endurance - price).max(0.0), max: view.endurance_max }.fatigue()
    },
    bounds: (0.0, 1.0),
    curve: Curve::FALLING.floored(0.1),
};

/// The stamina it would have left once it paid, of its most: what it keeps
/// for whatever it does next. Weighed only as far as a mind lowers its floor
const STAMINA_LEFT: Considered = Consideration {
    name: "stamina_left",
    read: |view| (view.stamina - common_bevy::tuning::tuning().cost(view.ability)) / view.attrs.max_stamina().max(1.0),
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(1.0),
};

/// Seconds of recovery the skill would leave it in, as its fatigue, a combo
/// fired early or a reaction through a recovery make them: the time it
/// spends. Weighed only as far as a mind lowers its floor
const RECOVERY_LEFT: Considered = Consideration {
    name: "recovery_left",
    read: |view| {
        let fatigue = Endurance { state: view.endurance, max: view.endurance_max }.fatigue();
        recovery_after(view.ability, view.recovery.as_ref(), &view.attrs, None, fatigue).remaining
    },
    bounds: (0.0, 6.0),
    curve: Curve::FALLING.floored(1.0),
};

// --- Strike ---

const FOE_STRUCK: Considered = step("foe_struck", |view| {
    flag(view.foe.is_some_and(|foe| foe.distance <= view.reach && foe.in_arc))
});

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

/// Punish: what it pays for is a foe still recovering, so the opening a
/// skill it just used leaves weighs on it apart from any strike's
const OPENING: Considered = Consideration {
    name: "opening",
    read: |view| view.foe.and_then(|foe| foe.since_skill).unwrap_or(f32::INFINITY),
    bounds: (2.0, 0.0),
    curve: Curve::RISING.floored(0.2),
};

// --- Reaction, and a leap clear to dodge ---

/// What it would clear, and what it would return of that, over its health
const WORTH_ANSWERING: Considered = Consideration {
    name: "worth_answering",
    read: |view| {
        let returned = view.queue.swept_direct * returned(view.ability) * view.attrs.line_power(view.ability);
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

const FOE_IN_REACH: Considered = step("foe_in_reach", |view| flag(foe_distance(view).is_some_and(|d| d <= view.reach)));

const FOE_OUT_OF_REACH: Considered = step("foe_out_of_reach", |view| flag(foe_distance(view).is_some_and(|d| d > view.reach)));

/// A dive lands it in reach to strike
const FOE_WITHIN_A_DIVE: Considered = step("foe_within_a_dive", |view| {
    flag(foe_distance(view).is_some_and(|d| d <= view.leap + view.reach))
});

/// A leap clear toward its leash's edge lands where it has no room left
/// to give ground; one back inward leaves it room
const ROOM_TO_LAND: Considered = Consideration {
    name: "room_to_land",
    read: |view| view.clear_room,
    bounds: (0.0, 0.3),
    curve: Curve::RISING,
};

// --- Stance ---

const NOT_STRIDING: Considered = step("not_striding", |view| flag(!view.striding));

// --- Commitments ---

/// What a strike deals, as a share of the health its foe has left: how much
/// nearer it brings the kill, a finishing blow most. More by Grit's share
/// with a full bank to release into it
const STRIKE_WORTH: Considered = Consideration {
    name: "strike_worth",
    read: |view| {
        let tuning = common_bevy::tuning::tuning();
        let release = if view.grit_filled >= 1.0 { 1.0 + tuning.grit_share } else { 1.0 };
        let dealt = view.attrs.base_potency() * tuning.damage(view.ability) * view.attrs.line_power(view.ability) * release;
        view.foe.map_or(0.0, |foe| dealt / foe.health.max(1.0))
    },
    bounds: (0.0, 0.25),
    curve: Curve::RISING.floored(0.3),
};

/// Grit: how full its bank is, only a full one released
const GRIT_BANKED: Considered = Consideration {
    name: "grit_banked",
    read: |view| view.grit_filled,
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.3),
};

/// Preparation: reactions it may still take through this recovery
const REACTIONS_LEFT: Considered = Consideration {
    name: "reactions_left",
    read: |view| {
        let tier = view.attrs.preparation().index() as f32;
        match view.recovery.as_ref().filter(|recovery| recovery.is_active()) {
            Some(recovery) => (tier - recovery.reactions as f32) / tier.max(1.0),
            None => 1.0,
        }
    },
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.6),
};

/// Patience: stamina spent, which it refills faster out of reach
const STAMINA_SPENT: Considered = Consideration {
    name: "stamina_spent",
    read: |view| view.stamina / view.attrs.max_stamina().max(1.0),
    bounds: (0.0, 1.0),
    curve: Curve::FALLING,
};

/// Patience: stamina back, to spend on a dive and what follows it
const STAMINA_READY: Considered = Consideration {
    name: "stamina_ready",
    read: |view| view.stamina / view.attrs.max_stamina().max(1.0),
    bounds: (0.0, 1.0),
    curve: Curve { shape: Shape::Power(2.0), falling: false, floor: 0.2 },
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
    use common_bevy::systems::combat::queue::create_threat;

    /// An actor built from its three pairs' axis, spectrum and shift
    fn built(points: [i8; 9]) -> ActorAttributes {
        let [a, b, c, d, e, f, g, h, i] = points;
        ActorAttributes::new(a, b, c, d, e, f, g, h, i)
    }

    fn view(ability: AbilityType, attrs: ActorAttributes) -> View {
        View {
            ability,
            attrs,
            health: 600.0,
            stamina: 100.0,
            // A pool deep enough to clear the blows these tests queue
            endurance: attrs.max_endurance().max(1000.0),
            endurance_max: attrs.max_endurance().max(1000.0),
            recovery: None,
            striding: false,
            grit_filled: 0.0,
            engaged: true,
            reach: 2,
            leap: 9,
            clear_room: 1.0,
            capacity_taken: false,
            queue: Threats::default(),
            foe: Some(Foe { distance: 1, health: 600.0, in_arc: true, across: false, since_skill: None }),
        }
    }

    fn threats(blows: &[(f32, bool, u64)], span: Duration, now: Duration) -> Threats {
        let plain = ActorAttributes::default();
        let source = Entity::from_raw_u32(9).unwrap();
        let queue: Vec<QueuedThreat> = blows.iter().map(|&(damage, ability, queued)| {
            let ability = Some(if ability { AbilityType::Frenzy } else { AbilityType::AutoAttack });
            create_threat(source, &plain, &plain, damage, ability, Duration::from_millis(queued), 0.0, 0.0)
        }).collect();
        Threats::reading(&queue, span, now)
    }

    fn scored(view: &mut View, reason: &str) -> f32 {
        let bar = [view.ability];
        weigh(view, &bar, &Mind::default()).into_iter().find(|decision| decision.reason == reason).map_or(0.0, |decision| decision.score)
    }

    #[test]
    fn nothing_queued_nothing_to_answer() {
        let mut quiet = view(AbilityType::Counter, ActorAttributes::default());
        assert!(choose(&mut quiet, &[AbilityType::Counter], &Mind::default(), |_| 0.0).is_none());
    }

    #[test]
    fn a_heavy_blow_is_answered_before_light_pressure() {
        let span = Duration::from_millis(250);
        let now = Duration::from_millis(1000);
        let mut heavy = view(AbilityType::Counter, ActorAttributes::default());
        heavy.queue = threats(&[(150.0, true, 0)], span, now);
        let mut light = view(AbilityType::Counter, ActorAttributes::default());
        light.queue = threats(&[(40.0, false, 0)], span, now);
        assert!(scored(&mut heavy, "answer") > WAIT, "a blow of a quarter of its health is answered");
        assert!(scored(&mut light, "answer") < WAIT, "one light auto-attack is let land");
    }

    #[test]
    fn a_reaction_waits_for_its_span_to_close() {
        let span = Duration::from_millis(1000);
        let mut fresh = view(AbilityType::Counter, ActorAttributes::default());
        fresh.queue = threats(&[(150.0, true, 0)], span, Duration::from_millis(900));
        let mut closed = view(AbilityType::Counter, ActorAttributes::default());
        closed.queue = threats(&[(150.0, true, 0)], span, Duration::from_millis(1100));
        assert_eq!(scored(&mut fresh, "answer"), 0.0, "a blow just queued waits for what may join it");
        assert!(scored(&mut closed, "answer") > WAIT);
        let mut dodge = view(AbilityType::Leap, ActorAttributes::default());
        dodge.queue = fresh.queue;
        assert_eq!(scored(&mut dodge, "dodge"), 0.0, "and so does a leap clear");
    }

    #[test]
    fn every_window_outlasts_the_widest_span_and_the_slowest_reaction() {
        let tuning = common_bevy::tuning::tuning();
        let shortest = tuning.reaction_window * (1.0 - tuning.fatigue_window);
        let slowest = crate::behaviour::perception::Skill::SLOPPY.slowest.as_secs_f32();
        assert!(shortest > tuning.awareness_span_max + slowest, "{shortest}s against {}s", tuning.awareness_span_max + slowest);
    }

    #[test]
    fn a_strike_needs_its_foe_in_reach_and_arc_and_prefers_one_that_just_acted() {
        let mut fresh = view(AbilityType::Feint, ActorAttributes::default());
        let mut spent = view(AbilityType::Feint, ActorAttributes::default());
        spent.foe = Some(Foe { since_skill: Some(0.3), ..fresh.foe.unwrap() });
        assert!(scored(&mut spent, "strike") > scored(&mut fresh, "strike"));
        let mut far = view(AbilityType::Feint, ActorAttributes::default());
        far.foe = Some(Foe { distance: 3, ..fresh.foe.unwrap() });
        assert_eq!(scored(&mut far, "strike"), 0.0);
        let mut poor = view(AbilityType::Feint, ActorAttributes::default());
        poor.stamina = 0.0;
        assert_eq!(scored(&mut poor, "strike"), 0.0, "what it cannot afford it never asks for");
    }

    #[test]
    fn a_strike_waits_while_its_pack_holds_the_capacity() {
        let mut full = view(AbilityType::Frenzy, ActorAttributes::default());
        full.capacity_taken = true;
        assert_eq!(scored(&mut full, "strike"), 0.0);
        let mut answering = view(AbilityType::Counter, ActorAttributes::default());
        answering.capacity_taken = true;
        answering.queue = threats(&[(150.0, true, 0)], Duration::from_millis(250), Duration::from_millis(1000));
        assert!(scored(&mut answering, "answer") > WAIT, "a reaction takes no slot");
    }

    #[test]
    fn a_skill_spent_near_empty_must_be_worth_more_than_one_spent_from_a_full_pool() {
        let mut full = view(AbilityType::Feint, ActorAttributes::default());
        let mut low = view(AbilityType::Feint, ActorAttributes::default());
        let price = low.attrs.skill_endurance(AbilityType::Feint);
        low.endurance = price * 1.5;
        let mut half = view(AbilityType::Feint, ActorAttributes::default());
        half.endurance = half.endurance_max / 2.0;
        let (full, half, low) = (scored(&mut full, "strike"), scored(&mut half, "strike"), scored(&mut low, "strike"));
        assert!(full - half < half - low, "half a pool costs little; the last of it costs much: {full} {half} {low}");
        assert!(low < WAIT, "near empty, a plain strike is not worth it");
    }

    #[test]
    fn grit_strikes_more_readily_the_fuller_its_bank() {
        let gritty = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        assert!(gritty.grit_fill() > 0);
        let mut empty = view(AbilityType::Feint, gritty);
        let mut full = view(AbilityType::Feint, gritty);
        full.grit_filled = 1.0;
        assert!(scored(&mut full, "strike") > scored(&mut empty, "strike"));
    }

    #[test]
    fn the_combo_its_recovery_offers_beats_waiting_where_it_would_not_alone() {
        let mut offered = view(AbilityType::Feint, ActorAttributes::default());
        offered.recovery = Some(recovery_after(AbilityType::Parry, None, &offered.attrs, None, 0.0));
        let combo = offered.recovery.unwrap().combo.unwrap();
        assert_eq!(combo.ability, AbilityType::Feint);
        offered.recovery.as_mut().unwrap().remaining = combo.unlock_at;
        let alone = scored(&mut offered, "strike");
        let mut mind = Mind::default();
        mind.wait = alone + 0.01;
        let chosen = choose(&mut offered, &[AbilityType::Feint], &mind, |_| 0.0);
        assert!(chosen.is_some_and(|decision| decision.ability == AbilityType::Feint), "carried on past a wait it would not beat alone");
        let mut fresh = view(AbilityType::Feint, ActorAttributes::default());
        assert!(choose(&mut fresh, &[AbilityType::Feint], &mind, |_| 0.0).is_none(), "and out of recovery the same strike waits");
    }

    #[test]
    fn a_mind_keeping_stamina_back_takes_the_cheaper_strike_when_short() {
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.stamina_left.floor", "0").unwrap();
        let mind = minds.mind(None);
        let attrs = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let tuning = common_bevy::tuning::tuning();
        let short = tuning.cost(AbilityType::Overpower) + attrs.max_stamina() * 0.1;
        let shape = |ability, stamina| {
            let mut v = view(ability, attrs);
            v.stamina = stamina;
            weigh(&mut v, &[ability], &mind)[0].responses.iter().find(|(name, _)| *name == "stamina_left").unwrap().1
        };
        assert!(shape(AbilityType::Overpower, short) < shape(AbilityType::Feint, short), "the dearer strike leaves less behind");
        assert_eq!(shape(AbilityType::Overpower, attrs.max_stamina()), shape(AbilityType::Feint, attrs.max_stamina()), "with a full pool, neither is held back");
    }

    #[test]
    fn a_mind_weighing_time_marks_a_long_recovery_down() {
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.recovery_left.floor", "0").unwrap();
        let mind = minds.mind(None);
        let tuning = common_bevy::tuning::tuning();
        let (quick, slow) = if tuning.recovery(AbilityType::Feint) < tuning.recovery(AbilityType::Overpower) {
            (AbilityType::Feint, AbilityType::Overpower)
        } else {
            (AbilityType::Overpower, AbilityType::Feint)
        };
        let response = |ability| {
            let mut v = view(ability, ActorAttributes::default());
            weigh(&mut v, &[ability], &mind)[0].responses.iter().find(|(name, _)| *name == "recovery_left").unwrap().1
        };
        assert!(response(slow) < response(quick));
    }

    #[test]
    fn a_counter_is_worth_more_than_a_parry_by_what_it_returns() {
        let span = Duration::from_millis(250);
        let now = Duration::from_millis(1000);
        let queue = threats(&[(60.0, true, 0)], span, now);
        let mut parry = view(AbilityType::Parry, ActorAttributes::default());
        parry.queue = queue;
        let mut counter = view(AbilityType::Counter, ActorAttributes::default());
        counter.queue = queue;
        assert!(scored(&mut counter, "answer") > scored(&mut parry, "answer"));
    }

    #[test]
    fn a_punish_waits_for_an_opening() {
        let mut fresh = view(AbilityType::Punish, ActorAttributes::default());
        let mut opened = fresh.clone();
        opened.foe = Some(Foe { since_skill: Some(0.2), ..opened.foe.unwrap() });
        assert!(scored(&mut opened, "punish") > scored(&mut fresh, "punish"));
    }

    #[test]
    fn a_heavier_strike_and_a_weaker_foe_are_worth_more() {
        // Committed to Vitality, Overpower's line, so it strikes whole
        let vital = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        let (mut light, mut heavy) = (view(AbilityType::Feint, vital), view(AbilityType::Overpower, vital));
        assert!(scored(&mut heavy, "strike") > scored(&mut light, "strike"));
        let mut finishing = view(AbilityType::Feint, vital);
        finishing.foe = Some(Foe { health: 20.0, ..finishing.foe.unwrap() });
        assert!(scored(&mut finishing, "strike") > scored(&mut light, "strike"), "a blow that nears the kill");
    }

    #[test]
    fn patience_leaps_clear_with_its_stamina_spent_and_dives_with_it_back() {
        let patient = built([0, 0, 0, 0, 0, 0, -10, 0, 0]);
        assert!(patient.patience().index() > 0);
        // Spent, with the stamina for a Leap and little more
        let mut empty = view(AbilityType::Leap, patient);
        empty.stamina = common_bevy::tuning::tuning().cost(AbilityType::Leap);
        let mut full = view(AbilityType::Leap, patient);
        full.stamina = patient.max_stamina();
        assert!(scored(&mut empty, "recover") > scored(&mut full, "recover"));
        for v in [&mut empty, &mut full] {
            v.foe = Some(Foe { distance: 8, ..v.foe.unwrap() });
        }
        assert!(scored(&mut full, "dive") > scored(&mut empty, "dive"));
        assert!(scored(&mut full, "dive") > WAIT, "refilled, it dives");
        assert_eq!(scored(&mut empty, "recover"), 0.0, "and out of reach there is nothing to leap clear of");
    }

    #[test]
    fn a_leap_clear_toward_its_leash_scores_below_one_back_inward() {
        let patient = built([0, 0, 0, 0, 0, 0, -10, 0, 0]);
        let mut inward = view(AbilityType::Leap, patient);
        let mut outward = view(AbilityType::Leap, patient);
        outward.clear_room = 0.05;
        inward.stamina = common_bevy::tuning::tuning().cost(AbilityType::Leap);
        outward.stamina = inward.stamina;
        assert!(scored(&mut inward, "recover") > scored(&mut outward, "recover"));
        assert!(scored(&mut outward, "recover") < WAIT, "it does not leap to its leash's edge to recover");
    }

    #[test]
    fn without_patience_a_leap_clear_is_only_a_dodge() {
        let mut plain = view(AbilityType::Leap, ActorAttributes::default());
        let reasons: Vec<&str> = weigh(&mut plain, &[AbilityType::Leap], &Mind::default()).iter().map(|decision| decision.reason).collect();
        assert_eq!(reasons, vec!["dodge", "dive"]);
        assert!(choose(&mut plain, &[AbilityType::Leap], &Mind::default(), |_| 0.0).is_none(), "in reach with nothing queued, it stays");
    }

    #[test]
    fn a_stride_is_taken_once_and_for_a_foe_in_reach() {
        let graceful = built([10, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut fresh = view(AbilityType::PerfectStride, graceful);
        fresh.foe = Some(Foe { across: true, ..fresh.foe.unwrap() });
        assert!(scored(&mut fresh, "stride") > WAIT);
        let mut held = fresh.clone();
        held.striding = true;
        assert_eq!(scored(&mut held, "stride"), 0.0);
    }
}
