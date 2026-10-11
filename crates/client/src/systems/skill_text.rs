//! What a skill is, said for the actor who holds it, plainly enough to
//! stand alone: what it does with each of its numbers as this build has
//! them, damage before its spread rolls, then what it costs and the
//! recovery it leaves. How it plays against other skills is left for the
//! player to find. The help on a skill's key and the skills tab both say
//! it; the tab marks each number its attribute line scales ([`Rules`]).

use common_bevy::{
    components::ActorAttributes,
    message::AbilityType,
    tuning::Tuning,
};

/// What `ability` is for an actor with `attrs`, by `tuning`'s numbers:
/// its name, what it does, then its cost and recovery on a line of their
/// own.
pub fn describe(ability: AbilityType, attrs: &ActorAttributes, tuning: &Tuning) -> String {
    if ability == AbilityType::AutoAttack {
        return "Auto-attack: strikes the hostile target within reach on its own".to_string();
    }
    format!(
        "{}: {}.\n{:.0} endurance, {} recovery",
        name(ability), does(ability, attrs, tuning).plain(), tuning.cost(ability), seconds(tuning.recovery(ability)),
    )
}

/// What a skill does, as a clause following its name ("bite your target
/// in reach for 140"), each number its attribute line scales marked.
pub struct Rules(String);

/// What opens and closes a number its line scales, inside [`Rules`]
const SCALED: char = '\u{1}';
const PLAIN: char = '\u{2}';

impl Rules {
    /// The clause as it reads, unmarked
    pub fn plain(&self) -> String {
        self.0.chars().filter(|&c| c != SCALED && c != PLAIN).collect()
    }

    /// The clause as a sentence of its own, in pieces, each with whether
    /// its line scales it
    pub fn sentence(&self) -> Vec<(String, bool)> {
        let mut pieces = Vec::new();
        let (mut piece, mut scaled) = (String::new(), false);
        for c in self.0.chars() {
            if c == SCALED || c == PLAIN {
                if !piece.is_empty() {
                    pieces.push((std::mem::take(&mut piece), scaled));
                }
                scaled = c == SCALED;
            } else if pieces.is_empty() && piece.is_empty() {
                piece.push(c.to_ascii_uppercase());
            } else {
                piece.push(c);
            }
        }
        if !piece.is_empty() {
            pieces.push((piece, scaled));
        }
        match pieces.last_mut() {
            Some((last, false)) => last.push('.'),
            _ => pieces.push((".".to_string(), false)),
        }
        pieces
    }
}

/// What `ability`, a skill, does for an actor with `attrs`.
pub fn does(ability: AbilityType, attrs: &ActorAttributes, tuning: &Tuning) -> Rules {
    let say = Say { ability, attrs, tuning };
    const ANSWER: &str = "answer the blows about to land on you";
    Rules(match ability {
        AbilityType::AutoAttack => "strike the hostile target within reach on its own".to_string(),
        AbilityType::Frenzy => format!("bite your target in reach for {}", say.strike()),
        AbilityType::Overpower => format!("strike your target in reach with one heavy blow for {}", say.strike()),
        AbilityType::Feint => format!("strike your target in reach with a light blow for {}", say.strike()),
        AbilityType::Punish => format!(
            "strike your target in reach for {}, plus {} for each stack of Overcommitted on it",
            damage(say.potency() * tuning.punish_base),
            say.lined(say.potency() * tuning.punish_per_stack, damage),
        ),
        AbilityType::Backstab => format!(
            "strike your target in reach from beside or behind it for {}. Its crits land {} harder",
            say.strike(), percent(tuning.backstab_crit),
        ),
        AbilityType::Kick => format!(
            "kick your target in reach for {}, knocking it back {}",
            damage(say.potency() * tuning.kick_damage), say.tiles(tuning.kick_tiles),
        ),
        AbilityType::Defy => format!(
            "strike your target in reach for {}, plus {} for each blow about to land on you",
            damage(say.potency() * tuning.defy_damage),
            say.lined(say.potency() * tuning.defy_damage * tuning.defy_per_threat, damage),
        ),
        AbilityType::Leap => format!(
            "leap {} away from your target in reach, dodging the blows coming, or onto it from afar for {}",
            say.tiles(tuning.leap_distance as f32), say.strike(),
        ),
        AbilityType::Parry => ANSWER.to_string(),
        AbilityType::Counter => format!("{ANSWER}, and send {} of each back to whoever struck it", say.lined(tuning.counter_reflect, percent)),
        AbilityType::Clash => format!("{ANSWER}, and stun whoever struck them for {}", say.lined(tuning.clash_stun, seconds)),
        AbilityType::Sidestep => format!("{ANSWER}, and step {} round whoever struck them, toward its back", say.tiles(tuning.sidestep_steps)),
        AbilityType::Yank => format!("{ANSWER}, and drag whoever struck them {} toward you", say.tiles(tuning.yank_tiles)),
        AbilityType::Pounce => format!("{ANSWER}, and lunge up to {} toward whoever struck them", say.tiles(tuning.pounce_tiles)),
        AbilityType::PerfectStride => format!(
            "for {}, run {} faster, and striking a target beside or behind you doesn't slow you",
            seconds(tuning.stride_secs), say.lined(tuning.stride_speed, percent),
        ),
        AbilityType::Bloodlust => format!(
            "for {}, heal for {} of the damage your attacks land",
            seconds(tuning.bloodlust_secs), say.lined(tuning.bloodlust_share, percent),
        ),
        AbilityType::Brace => format!("for {}, nothing a foe does stuns, slows, pins or drags you, nor breaks your stride", say.lined(tuning.brace_secs, seconds)),
        AbilityType::Surge => format!(
            "for {}, each tile you move makes your next skill strike {} harder",
            seconds(tuning.surge_secs), say.lined(tuning.surge_per_tile, percent),
        ),
        AbilityType::Vigil => format!("for {}, no strike takes you in the flank", say.lined(tuning.vigil_secs, seconds)),
        AbilityType::Lurk => format!(
            "for {}, no foe beyond your reach can see you, until you attack. A skill struck from hiding always crits",
            say.lined(tuning.lurk_secs, seconds),
        ),
    })
}

/// A skill's name as a player reads it: `PerfectStride` as Perfect Stride
pub fn name(ability: AbilityType) -> String {
    let mut name = String::new();
    for (i, c) in format!("{ability:?}").chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            name.push(' ');
        }
        name.push(c);
    }
    name
}

/// One skill said for one holder.
struct Say<'a> {
    ability: AbilityType,
    attrs: &'a ActorAttributes,
    tuning: &'a Tuning,
}

impl Say<'_> {
    fn potency(&self) -> f32 {
        self.attrs.base_potency(self.tuning)
    }

    /// `whole` as this holder's line has it, put by `put` and marked as
    /// its line scales it; all of it, unmarked, for a skill of no line
    fn lined(&self, whole: f32, put: impl Fn(f32) -> String) -> String {
        match ActorAttributes::line(self.ability) {
            Some(_) => format!("{SCALED}{}{PLAIN}", put(whole * self.attrs.line_power(self.tuning, self.ability))),
            None => put(whole),
        }
    }

    /// The damage its own strike deals, raised by its line
    fn strike(&self) -> String {
        self.lined(self.potency() * self.tuning.damage(self.ability), damage)
    }

    /// `whole` tiles as its line has them, never less than one
    /// (`ActorAttributes::line_tiles`), marked as its line scales them
    fn tiles(&self, whole: f32) -> String {
        let tiles = match self.attrs.line_tiles(self.tuning, self.ability, whole) {
            1 => "1 tile".to_string(),
            n => format!("{n} tiles"),
        };
        match ActorAttributes::line(self.ability) {
            Some(_) => format!("{SCALED}{tiles}{PLAIN}"),
            None => tiles,
        }
    }
}

/// Damage as it stands before its spread rolls, whole
fn damage(damage: f32) -> String {
    format!("{damage:.0}")
}

fn percent(share: f32) -> String {
    format!("{:.0}%", share * 100.0)
}

/// Whole seconds as whole, else to a tenth: 3s, 5.8s
pub fn seconds(secs: f32) -> String {
    let tenths = (secs * 10.0).round() / 10.0;
    if tenths.fract() == 0.0 { format!("{tenths:.0}s") } else { format!("{tenths:.1}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a skill does fits the skills tab's card: four lines of its
    /// text box, the most the longest of the rest takes
    const CARD_CHARS: usize = 106;

    #[test]
    fn what_every_skill_does_fits_its_card() {
        let tuning = Tuning::DEFAULT;
        for ability in AbilityType::ALL {
            let said = does(ability, &ActorAttributes::default(), &tuning).plain();
            assert!(said.len() <= CARD_CHARS, "{ability:?} takes {} characters: {said}", said.len());
        }
    }

    #[test]
    fn every_skill_says_what_it_does_then_its_cost_with_no_aside() {
        let tuning = Tuning::DEFAULT;
        let attrs = ActorAttributes::default();
        for ability in AbilityType::ALL.into_iter().filter(|&ability| ability != AbilityType::AutoAttack) {
            let said = describe(ability, &attrs, &tuning);
            assert!(said.starts_with(&format!("{}: ", name(ability))), "{said}");
            assert!(said.lines().last().is_some_and(|last| last.contains("endurance") && last.contains("recovery")), "{said}");
            assert!(!said.contains('('), "{ability:?} says it plainly: {said}");
            assert!(said.chars().all(|c| c == '\n' || c.is_ascii_graphic() || c == ' '), "{ability:?} says nothing the UI's font lacks: {said}");
        }
        assert_eq!(name(AbilityType::PerfectStride), "Perfect Stride");
        assert_eq!((seconds(3.0), seconds(5.8)), ("3s".to_string(), "5.8s".to_string()));
    }

    #[test]
    fn a_line_skill_marks_what_its_line_scales_and_a_skill_of_none_marks_nothing() {
        let tuning = Tuning::DEFAULT;
        let attrs = ActorAttributes::default();
        let kick = does(AbilityType::Kick, &attrs, &tuning).sentence();
        let scaled: Vec<&str> = kick.iter().filter(|(_, scaled)| *scaled).map(|(piece, _)| piece.as_str()).collect();
        assert_eq!(scaled.len(), 1, "only the knock back is its line's: {kick:?}");
        assert!(scaled[0].ends_with("tile") || scaled[0].ends_with("tiles"));
        assert!(kick[0].0.starts_with('K') && kick.last().unwrap().0.ends_with('.'), "a sentence of its own: {kick:?}");
        assert!(does(AbilityType::Parry, &attrs, &tuning).sentence().iter().all(|(_, scaled)| !scaled));
        assert_eq!(does(AbilityType::Kick, &attrs, &tuning).plain().chars().filter(|c| c.is_control()).count(), 0);
    }
}
