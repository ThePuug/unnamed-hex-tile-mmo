//! `arena curves`: every archetype's decisions as the code scores them, as
//! JSON for the skills reference page (`arena/skill_curves.html`): its bar,
//! each skill's decisions with their considerations, each its own curve and
//! as the archetype's mind shapes it, its steps' likewise, and its mind's
//! thresholds.

use combat::behaviour::{mind::{Mind, Minds}, moves, skills, utility::{Consideration, Shape}};
use common_bevy::archetype::EnemyArchetype;
use serde_json::{json, Map, Value};

/// A number as the page shows it, free of f32's tail
fn n(value: f32) -> Value {
    json!((value as f64 * 1000.0).round() / 1000.0)
}

fn curve<V>(consideration: &Consideration<V>) -> Value {
    let shape = match consideration.curve.shape {
        Shape::Power(power) => json!({ "power": n(power) }),
        Shape::Logistic { mid, steep } => json!({ "logistic": [n(mid), n(steep)] }),
    };
    let (from, to) = consideration.bounds;
    json!({ "from": n(from), "to": n(to), "floor": n(consideration.curve.floor), "falling": consideration.curve.falling, "shape": shape })
}

fn considered<V>(consideration: &Consideration<V>, mind: &Mind) -> Value {
    json!({
        "name": consideration.name,
        "condition": consideration.is_condition(),
        "own": curve(consideration),
        "mind": curve(&mind.shape(consideration)),
    })
}

/// Prints every archetype's decisions and mind, as the world fights with
/// them ([`Minds::tuned`]).
pub fn print() {
    let minds = Minds::tuned();
    let archetypes: Vec<Value> = EnemyArchetype::ALL.into_iter().map(|archetype| {
        let mind = minds.mind(Some(archetype));
        let profile = archetype.profile();
        let decisions: Vec<Value> = archetype.bar().into_iter()
            .flat_map(|ability| skills::decisions(ability).into_iter().map(move |decision| (ability, decision)))
            .map(|(ability, (reason, considerations))| json!({
                "skill": format!("{ability:?}"),
                "own": ability == profile.ability,
                "reason": reason,
                "considerations": considerations.iter().map(|consideration| considered(consideration, &mind)).collect::<Vec<_>>(),
            }))
            .collect();
        let just_acted: Map<String, Value> = EnemyArchetype::ALL.into_iter()
            .map(|foe| (format!("{foe:?}"), n(mind.just_acted(Some(foe.profile().approach)))))
            .collect();
        json!({
            "archetype": format!("{archetype:?}"),
            "approach": format!("{:?}", profile.approach),
            "wait": n(mind.wait), "hold": n(mind.hold), "momentum": n(mind.momentum), "combo": n(mind.combo),
            "decisions": decisions,
            "steps": moves::ENGAGE.iter().map(|consideration| considered(consideration, &mind)).collect::<Vec<_>>(),
            "just_acted": just_acted,
        })
    }).collect();
    let plain = Mind::default();
    let all = json!({
        "defaults": { "wait": n(plain.wait), "hold": n(plain.hold), "momentum": n(plain.momentum), "combo": n(plain.combo) },
        "archetypes": archetypes,
    });
    println!("{}", serde_json::to_string_pretty(&all).expect("arena: curves to JSON"));
}
