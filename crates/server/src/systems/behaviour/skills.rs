//! An NPC's skills channel: which of its skills it uses now, if any.
//!
//! Every skill on its bar brings one decision for each reason to use it,
//! and each decision brings considerations from what the skill does (a
//! strike, a reaction, a leap clear or in, a stance) and from the
//! commitments its user holds. A commitment's considerations read its own
//! state, so an NPC that scores them plays the way its commitment pays:
//! no style is written down. The channel's do-nothing decision, waiting,
//! scores [`WAIT`], and a skill is used only where it scores higher.
//!
//! Every input is a ratio in the game's own terms, so a curve means the
//! same at any level and holds when a skill's numbers change.

use std::time::Duration;

use common_bevy::{
    components::{reaction_queue::QueuedThreat, recovery::GlobalRecovery, ActorAttributes},
    message::AbilityType,
    systems::combat::combos::{may_use, reacts_through},
};

use super::utility::{score, Consideration, Curve, Shape};

/// What waiting scores: the threshold every skill's decision must beat.
pub const WAIT: f32 = 0.35;

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
    pub recovery: Option<GlobalRecovery>,
    /// Whether a Perfect Stride holds
    pub striding: bool,
    /// Blows its Grit holds
    pub grit_held: u8,
    /// Swings its Patience holds behind the due one
    pub banked: u32,
    /// Its own reach, in tiles
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    pub queue: Threats,
    pub foe: Option<Foe>,
}

/// Its queue, as a reaction would meet it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Threats {
    /// Damage a reaction would take now: the front threat and its span
    pub swept: f32,
    /// Of that, what auto-attacks would deal
    pub swept_pressure: f32,
    /// The front threat's damage
    pub front: f32,
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
        let (mut damage, mut pressure) = (0.0, 0.0);
        for threat in swept {
            let blow = threat.damage + threat.dot_left();
            damage += blow;
            if threat.is_pressure() {
                pressure += blow;
            }
        }
        let since = now.saturating_sub(front.inserted_at).as_secs_f32();
        Self {
            swept: damage,
            swept_pressure: pressure,
            front: front.damage + front.dot_left(),
            span_closed: if span.is_zero() { 1.0 } else { since / span.as_secs_f32() },
        }
    }
}

/// Its target, as it sees it.
#[derive(Clone, Copy, Debug)]
pub struct Foe {
    pub distance: i32,
    /// Within the arc it strikes within
    pub in_arc: bool,
    /// Past its forward faces, where a swing breaks its stride
    pub across: bool,
    /// Share of the foe's recovery still to run, 0 with none
    pub recovering: f32,
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

/// The best of `bar`'s decisions for what `view` perceives, where it beats
/// waiting; None to wait. `view.ability` is set to each skill in turn.
pub fn choose(view: &mut View, bar: &[AbilityType]) -> Option<Decision> {
    weigh(view, bar).into_iter()
        .filter(|decision| decision.score > WAIT)
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

/// Every decision `bar` brings, scored.
pub fn weigh(view: &mut View, bar: &[AbilityType]) -> Vec<Decision> {
    let mut decisions = Vec::new();
    for &ability in bar {
        view.ability = ability;
        for (reason, considerations) in reasons(ability, &view.attrs) {
            let responses: Vec<(&'static str, f32)> = considerations.iter()
                .map(|consideration| (consideration.name, consideration.answer(view)))
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

/// Each reason to use `ability`, and its considerations for a user with
/// `attrs`: what any skill asks, what its part asks, and what the
/// commitments its user holds ask of that part.
fn reasons(ability: AbilityType, attrs: &ActorAttributes) -> Vec<(&'static str, Vec<Considered>)> {
    let parts: &[(&'static str, Part)] = match ability {
        AbilityType::AutoAttack => &[],
        AbilityType::Frenzy | AbilityType::Feint => &[("strike", Part::Strike)],
        AbilityType::Parry | AbilityType::Counter => &[("answer", Part::Reaction)],
        AbilityType::Leap => &[("dodge", Part::Clear), ("bank", Part::Clear), ("dive", Part::Dive)],
        AbilityType::PerfectStride => &[("stride", Part::Stance)],
    };
    parts.iter()
        .filter(|&&(reason, _)| reason != "bank" || attrs.patience().index() > 0)
        .map(|&(reason, part)| {
            let mut considerations = vec![OPEN, AFFORDABLE, ENDURANCE];
            considerations.extend(part_considerations(reason, part));
            considerations.extend(commitment_considerations(ability, reason, part, attrs));
            (reason, considerations)
        })
        .collect()
}

fn part_considerations(reason: &str, part: Part) -> Vec<Considered> {
    match (part, reason) {
        (Part::Strike, _) => vec![FOE_STRUCK, FOE_RECOVERING],
        (Part::Reaction, _) => vec![WORTH_ANSWERING, PRESSURE, SPAN_CLOSED],
        (Part::Clear, "dodge") => vec![FOE_IN_REACH, WORTH_ANSWERING],
        (Part::Clear, _) => vec![FOE_IN_REACH],
        (Part::Dive, _) => vec![FOE_OUT_OF_REACH, FOE_WITHIN_A_DIVE],
        (Part::Stance, _) => vec![NOT_STRIDING, FOE_IN_REACH],
    }
}

fn commitment_considerations(ability: AbilityType, reason: &str, part: Part, attrs: &ActorAttributes) -> Vec<Considered> {
    let mut considerations = Vec::new();
    if attrs.ferocity().index() > 0 && ability.combo().is_some() {
        considerations.extend([COMBO_OFFERED, BURST_CARRIED]);
    }
    if attrs.grit_holds() > 0 && matches!(part, Part::Strike | Part::Dive) {
        considerations.push(GRIT_BANKED);
    }
    if attrs.preparation().index() > 0 && part == Part::Reaction {
        considerations.push(REACTIONS_LEFT);
    }
    if attrs.patience().index() > 0 {
        match (part, reason) {
            (Part::Clear, "bank") => considerations.push(BANK_EMPTY),
            (Part::Dive, _) => considerations.push(BANK_FULL),
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

/// It has the stamina, and a Parry the endurance for the front threat
const AFFORDABLE: Considered = step("affordable", |view| {
    let stamina = view.stamina >= common_bevy::tuning::tuning().cost(view.ability);
    let endurance = view.ability != AbilityType::Parry || view.endurance >= view.attrs.parry_effort(view.queue.front);
    flag(stamina && endurance)
});

/// The endurance it spends, over what it has: spent endurance lengthens
/// every recovery after
const ENDURANCE: Considered = Consideration {
    name: "endurance",
    read: |view| {
        let price = match view.ability {
            AbilityType::Parry => view.attrs.parry_effort(view.queue.swept),
            ability => view.attrs.skill_endurance(ability),
        };
        price / view.endurance.max(f32::EPSILON)
    },
    bounds: (0.0, 1.0),
    curve: Curve::FALLING.floored(0.5),
};

// --- Strike ---

const FOE_STRUCK: Considered = step("foe in reach and arc", |view| {
    flag(view.foe.is_some_and(|foe| foe.distance <= view.reach && foe.in_arc))
});

/// A foe in recovery cannot answer with a skill
const FOE_RECOVERING: Considered = Consideration {
    name: "foe recovering",
    read: |view| view.foe.map_or(0.0, |foe| foe.recovering),
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(0.6),
};

// --- Reaction, and a leap clear to dodge ---

/// What it would clear, over its health
const WORTH_ANSWERING: Considered = Consideration {
    name: "worth answering",
    read: |view| view.queue.swept / health(view),
    bounds: (0.0, WORTH),
    curve: Curve { shape: Shape::Logistic { mid: 0.4, steep: 8.0 }, falling: false, floor: 0.0 },
};

/// Auto-attacks are pressure, steady and light; abilities are what a
/// reaction is for
const PRESSURE: Considered = Consideration {
    name: "pressure share",
    read: |view| if view.queue.swept > 0.0 { view.queue.swept_pressure / view.queue.swept } else { 0.0 },
    bounds: (0.0, 1.0),
    curve: Curve::FALLING.floored(0.5),
};

/// A threat queued once the span has closed lands past it, so waiting
/// until then lets the most join one answer
const SPAN_CLOSED: Considered = Consideration {
    name: "span closed",
    read: |view| view.queue.span_closed,
    bounds: (0.0, 1.0),
    curve: Curve { shape: Shape::Power(2.0), falling: false, floor: 0.1 },
};

// --- Leap ---

const FOE_IN_REACH: Considered = step("foe in reach", |view| flag(foe_distance(view).is_some_and(|d| d <= view.reach)));

const FOE_OUT_OF_REACH: Considered = step("foe out of reach", |view| flag(foe_distance(view).is_some_and(|d| d > view.reach)));

/// A dive lands it in reach to strike
const FOE_WITHIN_A_DIVE: Considered = step("foe within a dive", |view| {
    flag(foe_distance(view).is_some_and(|d| d <= view.leap + view.reach))
});

// --- Stance ---

const NOT_STRIDING: Considered = step("not striding", |view| flag(!view.striding));

// --- Commitments ---

/// Ferocity: the skill is the combo its recovery offers, a burst under way
const COMBO_OFFERED: Considered = Consideration {
    name: "combo offered",
    read: |view| flag(view.recovery.as_ref().and_then(|recovery| recovery.combo).is_some_and(|combo| combo.ability == view.ability)),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.6),
};

/// Ferocity: stamina to carry a whole burst, every bite its tier fires early
const BURST_CARRIED: Considered = Consideration {
    name: "burst carried",
    read: |view| {
        let burst = common_bevy::tuning::tuning().cost(view.ability) * (view.attrs.ferocity().index() as f32 + 1.0);
        view.stamina / burst.max(f32::EPSILON)
    },
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.4),
};

/// Grit: the blows banked, against what it holds
const GRIT_BANKED: Considered = Consideration {
    name: "grit banked",
    read: |view| view.grit_held as f32 / view.attrs.grit_holds().max(1) as f32,
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.3),
};

/// Preparation: reactions it may still take through this recovery
const REACTIONS_LEFT: Considered = Consideration {
    name: "reactions left",
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

/// Patience: a bank with room to fill, which it fills standing out of reach
const BANK_EMPTY: Considered = Consideration {
    name: "bank empty",
    read: |view| view.banked as f32 / view.attrs.patience().index().max(1) as f32,
    bounds: (0.0, 1.0),
    curve: Curve::FALLING,
};

/// Patience: a full bank, spent by a dive
const BANK_FULL: Considered = Consideration {
    name: "bank full",
    read: |view| view.banked as f32 / view.attrs.patience().index().max(1) as f32,
    bounds: (0.0, 1.0),
    curve: Curve { shape: Shape::Power(2.0), falling: false, floor: 0.2 },
};

/// Grace: a foe past its forward faces, struck there only in a stride
const FOE_ACROSS: Considered = Consideration {
    name: "foe across",
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
            endurance: 100.0,
            recovery: None,
            striding: false,
            grit_held: 0,
            banked: 0,
            reach: 2,
            leap: 9,
            queue: Threats::default(),
            foe: Some(Foe { distance: 1, in_arc: true, across: false, recovering: 0.0 }),
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
        weigh(view, &bar).into_iter().find(|decision| decision.reason == reason).map_or(0.0, |decision| decision.score)
    }

    #[test]
    fn nothing_queued_nothing_to_answer() {
        let mut quiet = view(AbilityType::Counter, ActorAttributes::default());
        assert!(choose(&mut quiet, &[AbilityType::Counter]).is_none());
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
        fresh.queue = threats(&[(150.0, true, 0)], span, Duration::from_millis(100));
        let mut closed = view(AbilityType::Counter, ActorAttributes::default());
        closed.queue = threats(&[(150.0, true, 0)], span, Duration::from_millis(1100));
        assert!(scored(&mut fresh, "answer") < scored(&mut closed, "answer"));
        assert!(scored(&mut fresh, "answer") < WAIT, "a blow just queued waits for what may join it");
    }

    #[test]
    fn a_strike_needs_its_foe_in_reach_and_arc_and_prefers_one_recovering() {
        let mut fresh = view(AbilityType::Feint, ActorAttributes::default());
        let mut spent = view(AbilityType::Feint, ActorAttributes::default());
        spent.foe = Some(Foe { recovering: 0.5, ..fresh.foe.unwrap() });
        assert!(scored(&mut spent, "strike") > scored(&mut fresh, "strike"));
        let mut far = view(AbilityType::Feint, ActorAttributes::default());
        far.foe = Some(Foe { distance: 3, ..fresh.foe.unwrap() });
        assert_eq!(scored(&mut far, "strike"), 0.0);
        let mut poor = view(AbilityType::Feint, ActorAttributes::default());
        poor.stamina = 0.0;
        assert_eq!(scored(&mut poor, "strike"), 0.0, "what it cannot afford it never asks for");
    }

    #[test]
    fn grit_strikes_harder_the_more_it_has_banked() {
        let gritty = built([0, 0, 0, -10, 0, 0, 0, 0, 0]);
        assert!(gritty.grit_holds() > 0);
        let mut empty = view(AbilityType::Feint, gritty);
        let mut full = view(AbilityType::Feint, gritty);
        full.grit_held = gritty.grit_holds();
        assert!(scored(&mut full, "strike") > scored(&mut empty, "strike"));
    }

    #[test]
    fn patience_leaps_clear_with_its_bank_empty_and_dives_with_it_full() {
        let patient = built([0, 0, 0, 0, 0, 0, -10, 0, 0]);
        assert!(patient.patience().index() > 0);
        let tier = patient.patience().index() as u32;
        let mut empty = view(AbilityType::Leap, patient);
        let mut full = view(AbilityType::Leap, patient);
        full.banked = tier;
        assert!(scored(&mut empty, "bank") > scored(&mut full, "bank"));
        for v in [&mut empty, &mut full] {
            v.foe = Some(Foe { distance: 8, ..v.foe.unwrap() });
        }
        assert!(scored(&mut full, "dive") > scored(&mut empty, "dive"));
        assert!(scored(&mut full, "dive") > WAIT, "a full bank dives");
        assert_eq!(scored(&mut empty, "bank"), 0.0, "and out of reach there is nothing to leap clear of");
    }

    #[test]
    fn without_patience_a_leap_clear_is_only_a_dodge() {
        let mut plain = view(AbilityType::Leap, ActorAttributes::default());
        let reasons: Vec<&str> = weigh(&mut plain, &[AbilityType::Leap]).iter().map(|decision| decision.reason).collect();
        assert!(!reasons.contains(&"bank"));
        assert!(choose(&mut plain, &[AbilityType::Leap]).is_none(), "in reach with nothing queued, it stays");
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
