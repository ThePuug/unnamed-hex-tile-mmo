//! Every ability an actor uses goes through one gate, [`Abilities::cast`].
//! The gate asks, in order and the same of every ability: is the caster
//! alive and unheld; is it on the bar, for a player (`Loadout`); is the
//! caster activating nothing, which a stance alone need not wait for; is
//! the skill recharged; does the caster hold
//! its endurance; does a pin hold it, for a skill that is only a move; is
//! its target a living hostile within the skill's reach and the caster's
//! arc, for one with a reach (`AbilityType::reach`). An auto-attack asks its own
//! clock in place of a recharge, and costs nothing.
//!
//! Let through, a skill is paid for and its activation starts
//! (`Activating`), told to every client once. It lands when the activation
//! ends ([`Abilities::finish`]): one with a reach on the target it named,
//! if that one lives and stands within reach and arc then, else it misses.
//! A skill with no activation lands as it is used. However an activation
//! ends, landed, missed, interrupted ([`Abilities::interrupt`]) or
//! cancelled (`crate::Cancel`), the skill's recharge starts there.
//!
//! Every attack, a strike's or a swing's, lands through
//! [`Abilities::attack`]: the one place a block stops it, damage is dealt
//! and what it carries takes hold.
//!
//! What each ability costs, how long it activates and recharges and how
//! far it reaches are the ability's own to say (`Tuning::cost`,
//! `Tuning::activation`, `Tuning::recharge`, `AbilityType::reach`). Its module
//! here holds only what it does.
//!
//! One system, [`use_abilities`], runs all of it in a stated order: the
//! activations that have ended land, soonest first; the auto-attacks come
//! due; then what players asked for; then what each NPC's skills channel
//! chooses.

pub mod block;
pub mod leap;
pub mod npc;
pub mod pounce;
pub mod sidestep;
pub mod stance;
pub mod strike;
pub mod yank;

use std::time::Duration;

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    components::{
        activation::{Activating, Activation, Recharge, Recharges},
        behaviour::Side,
        entity_type::EntityType,
        heading::Heading,
        resources::{CombatState, Endurance, Health, RespawnTimer},
        rune::Rune,
        status::{Stance, Status},
        target::Target,
        ActorAttributes, AttackRange, Loc, Swing,
    },
    message::{AbilityType, Component as MessageComponent, Do, Event as GameEvent, Outcome, SkillKind, Try},
    moment::Moment,
    resources::map::Map,
    systems::{combat::{damage, state::enter_combat}, targeting},
    tuning::Tuning,
};

use crate::{
    behaviour::{chase::Chase, perception::{Sight, Skill}},
    engagement::{Engagement, EngagementMember},
    landing,
};

pub use common_bevy::systems::combat::gate::{admits, AbilityFailReason, Standing};

/// What an attack carries to its target: its damage, and what lands with
/// it, seconds of stun, tiles of drag (toward its striker where positive)
/// and whether it interrupts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Blow {
    pub damage: f32,
    pub stun: f32,
    pub drag: i8,
    pub interrupts: bool,
}

/// One use of an ability: who uses it, from where, facing which way, at
/// whom.
#[derive(Clone, Copy, Debug)]
pub struct Cast {
    pub ent: Entity,
    pub ability: AbilityType,
    pub loc: Loc,
    pub heading: Option<Heading>,
    pub attrs: ActorAttributes,
    pub side: Option<Side>,
    /// The caster's own reach, in tiles
    pub reach: i32,
    /// Whoever it is used on
    pub target: Option<Entity>,
    /// Where that one stands, for an ability with a reach whose target is
    /// a living hostile
    pub target_loc: Option<Loc>,
}

impl Cast {
    /// The living hostile it is used on and where it stands
    pub fn struck(&self) -> Result<(Entity, Loc), AbilityFailReason> {
        self.target.zip(self.target_loc).ok_or(AbilityFailReason::NoTargets)
    }
}

/// Everything an ability reads or changes as it is used.
#[derive(SystemParam)]
pub struct Abilities<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub writer: MessageWriter<'w, Do>,
    pub actors: Query<'w, 's, (
        &'static Loc,
        &'static ActorAttributes,
        Option<&'static Heading>,
        Option<&'static Side>,
        Option<&'static AttackRange>,
        Has<RespawnTimer>,
    )>,
    pub health: Query<'w, 's, &'static mut Health>,
    pub endurance: Query<'w, 's, &'static mut Endurance>,
    pub skills: Query<'w, 's, (Entity, &'static mut Activating, &'static mut Recharges)>,
    pub statuses: Query<'w, 's, &'static mut Status>,
    pub swings: Query<'w, 's, &'static mut Swing>,
    pub combat: Query<'w, 's, &'static mut CombatState>,
    pub surges: Query<'w, 's, &'static stance::Surged>,
    pub targets: Query<'w, 's, (Entity, &'static Target)>,
    pub npcs: Query<'w, 's, (Entity, &'static EntityType, &'static crate::behaviour::Bar), With<Chase>>,
    pub minds: Query<'w, 's, (&'static Skill, &'static mut Sight)>,
    pub leashed: Query<'w, 's, (&'static Chase, &'static EngagementMember)>,
    pub dens: Query<'w, 's, &'static Loc, With<Engagement>>,
    pub engagements: Query<'w, 's, &'static Engagement>,
    pub loadouts: Query<'w, 's, &'static common_bevy::components::loadout::Loadout>,
    pub positions: Query<'w, 's, &'static common_bevy::components::position::Position>,
    /// Every rune standing, where, and whose: a strike's target where it
    /// names one ([`Abilities::foe`]), and the tiles a rune is refused on
    pub runes: Query<'w, 's, (Entity, &'static Loc, &'static mut crate::runes::RuneStone)>,
    pub rune_sides: Query<'w, 's, &'static Side, With<crate::runes::RuneStone>>,
    pub nntree: Res<'w, common_bevy::plugins::nntree::NNTree>,
    pub map: Res<'w, Map>,
    pub time: Res<'w, Time>,
    pub runtime: Res<'w, crate::RunTime>,
    pub dice: Res<'w, crate::dice::Dice>,
    pub tuning: Res<'w, Tuning>,
    pub mind_set: Res<'w, crate::behaviour::mind::Minds>,
    pub rolls: Query<'w, 's, &'static mut crate::dice::Rolls>,
    pub decisions: Option<ResMut<'w, crate::behaviour::Decisions>>,
}

/// Lands every activation that has ended, then uses every ability due or
/// asked for this frame: the auto-attacks that have come due, what players
/// asked, in the order asked, then what each NPC chooses.
pub fn use_abilities(mut reader: MessageReader<Try>, mut abilities: Abilities) {
    abilities.finish();
    // Swings go before skills: a Leap moves its user as it is used, so a
    // swing after it would strike from where it lands
    abilities.swing();
    for message in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target } } = message else { continue };
        abilities.cast(*ent, *ability, *target).ok();
    }
    abilities.skills();
}

/// Ends the activation of the actor a `crate::Cancel` names, by its own
/// step: nothing lands, its endurance stays spent and its recharge
/// starts.
pub fn cancel(trigger: On<crate::Cancel>, mut abilities: Abilities) {
    abilities.end(trigger.event().ent, Outcome::Cancelled);
}

impl Abilities<'_, '_> {
    /// Uses `ability` for `ent` now if the gate lets it. Returns whether
    /// it was used; a refusal is told to no one.
    pub fn ask(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>) -> bool {
        self.cast(ent, ability, asked).is_ok()
    }

    /// Swings for every actor whose auto-attack has come due at the hostile
    /// it targets: an NPC's from its chase, a player's from its facing.
    fn swing(&mut self) {
        let due: Vec<(Entity, Entity)> = self.targets.iter()
            .filter_map(|(ent, target)| Some((ent, target.entity?)))
            .collect();
        for (ent, target) in due {
            self.ask(ent, AbilityType::AutoAttack, Some(target));
        }
    }

    /// Lands every activation whose time has run, the soonest ended first,
    /// and starts its skill's recharge. One a skill landing before it this
    /// frame interrupted lands nothing.
    fn finish(&mut self) {
        let now = self.game_now();
        let mut due: Vec<(Moment, Entity)> = self.skills.iter()
            .filter_map(|(ent, activating, _)| activating.0.filter(|activation| activation.ends() <= now).map(|activation| (activation.ends(), ent)))
            .collect();
        due.sort();
        for (_, ent) in due {
            let Some(activation) = self.activation_of(ent) else { continue };
            self.clear_activation(ent);
            self.recharge(ent, activation.ability);
            if let Some(cast) = self.caster(ent, activation.ability, activation.target) {
                self.resolve(&cast);
            }
        }
    }

    /// The gate. `asked` is the actor the ability is used on. Errs with
    /// the reason it was refused, or with none where there is nothing to
    /// tell: a dead caster, a swing not yet due.
    pub fn cast(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>) -> Result<(), Option<AbilityFailReason>> {
        let tuning = *self.tuning;
        let now = self.game_now();
        let Some(mut cast) = self.caster(ent, ability, asked) else { return Err(None) };
        let status = self.statuses.get(ent).ok().copied().unwrap_or_default();

        // A player uses only what its bar holds
        if ability != AbilityType::AutoAttack && self.loadouts.get(ent).is_ok_and(|loadout| !loadout.holds(ability)) {
            return Err(Some(AbilityFailReason::NotHeld));
        }

        // An auto-attack comes due on its own clock
        if ability == AbilityType::AutoAttack {
            let due = self.swings.get(ent).is_ok_and(|swing| swing.waited(Moment::ZERO + self.time.elapsed()).is_some());
            if !due {
                return Err(None);
            }
        }

        // An NPC's strike lands on what stands nearest in front of it
        if ability.kind() == SkillKind::Strike && self.npcs.contains(ent) {
            cast.target = self.in_front(&cast);
        }
        // Reach is measured as a swing measures it, the first level of
        // height between free
        if ability.reach(&tuning, &cast.attrs, cast.reach).is_some() {
            cast.target_loc = self.foe(&cast).ok().map(|(_, target_loc)| target_loc);
        }
        let foe = cast.target_loc.map(|target_loc| (cast.loc.distance(&target_loc), targeting::faces(cast.heading.as_ref(), &cast.loc, &target_loc)));
        let standing = self.standing(ent, ability, &status, now);
        admits(&tuning, ability, &standing, &cast.attrs, cast.reach, foe).map_err(Some)?;
        self.room(&cast).map_err(Some)?;

        if ability == AbilityType::AutoAttack {
            let (target, _) = cast.struck().map_err(Some)?;
            if let Ok(mut swing) = self.swings.get_mut(ent) {
                swing.due = Some(Moment::ZERO + self.time.elapsed() + Duration::from_secs_f32(tuning.base_interval));
            }
            self.attack(ent, target, ability, Blow { damage: cast.attrs.auto_damage(&tuning), ..Blow::default() });
            return Ok(());
        }

        // Endurance is paid as the skill starts, however it ends
        self.spend(ent, tuning.cost(ability));
        let activation = Activation { ability, target: cast.target, began: now, length: Duration::from_secs_f32(tuning.activation(ability)) };
        self.writer.write(Do { event: GameEvent::Activate { ent, activation } });
        if activation.length.is_zero() {
            self.recharge(ent, ability);
            self.resolve(&cast);
        } else if let Ok((_, mut activating, _)) = self.skills.get_mut(ent) {
            activating.0 = Some(activation);
        }
        Ok(())
    }

    /// `ent` using `ability` on `target` from where it stands now; None
    /// for one that is no actor, or dead
    fn caster(&self, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Option<Cast> {
        let (&loc, &attrs, heading, side, range, _) = self.actors.get(ent).ok()?;
        self.lives(ent).then_some(Cast {
            ent,
            ability,
            loc,
            heading: heading.copied(),
            attrs,
            side: side.copied(),
            reach: range.copied().unwrap_or_default().0,
            target,
            target_loc: None,
        })
    }

    /// How `ent` stands as the gate asks `ability` of it at `now`
    pub fn standing(&self, ent: Entity, ability: AbilityType, status: &Status, now: Moment) -> Standing {
        let (activating, recharged) = self.skills.get(ent)
            .map_or((false, true), |(_, activating, recharges)| (activating.0.is_some(), recharges.is_ready(ability, now)));
        Standing {
            held: status.is_held(),
            pinned: status.is_pinned(),
            activating,
            recharged,
            endurance: self.endurance.get(ent).map_or(0.0, |endurance| endurance.state),
        }
    }

    /// Whether `cast` has somewhere to go, asked before it is paid for: a
    /// move its landing, a rune its tile ahead
    fn room(&self, cast: &Cast) -> Result<(), AbilityFailReason> {
        match cast.ability {
            AbilityType::Leap => leap::landing(self, cast).map(|_| ()),
            AbilityType::Sidestep | AbilityType::Pounce => self.foe(cast).map(|_| ()),
            AbilityType::Drag => crate::runes::room(self, cast, Rune::Drag).map(|_| ()),
            _ => Ok(()),
        }
    }

    /// What `cast` does as its activation ends, from where its user and
    /// its target then stand. One with a reach lands on the target it
    /// named, living, within its reach and its user's arc, and for one
    /// that lands only from a flank (`AbilityType::flanks`) with its user
    /// past the target's forward faces; else it misses.
    fn resolve(&mut self, cast: &Cast) {
        let tuning = *self.tuning;
        let mut cast = *cast;
        if let Some(within) = cast.ability.reach(&tuning, &cast.attrs, cast.reach) {
            let lands = self.foe(&cast).ok().filter(|(target, target_loc)| {
                let facing = self.actors.get(*target).ok().and_then(|(_, _, heading, ..)| heading);
                within.contains(&cast.loc.distance(target_loc))
                    && targeting::faces(cast.heading.as_ref(), &cast.loc, target_loc)
                    && (!cast.ability.flanks() || targeting::flanked(facing, target_loc, &cast.loc))
            });
            let Some((_, target_loc)) = lands else {
                self.told(cast.ent, cast.ability, cast.target, 0.0, Outcome::Missed);
                return;
            };
            cast.target_loc = Some(target_loc);
        }
        match cast.ability {
            AbilityType::AutoAttack => {}
            AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower | AbilityType::Punish
            | AbilityType::Backstab | AbilityType::Kick | AbilityType::Defy => strike::strike(self, &cast),
            AbilityType::Parry | AbilityType::Counter | AbilityType::Clash => block::take(self, &cast),
            AbilityType::Leap => leap::leap(self, &cast),
            AbilityType::Sidestep => sidestep::step(self, &cast),
            AbilityType::Pounce => pounce::lunge(self, &cast),
            AbilityType::Yank => yank::pull(self, &cast),
            AbilityType::PerfectStride | AbilityType::Bloodlust | AbilityType::Brace | AbilityType::Surge
            | AbilityType::Vigil | AbilityType::Lurk => stance::take(self, &cast),
            AbilityType::Drag => {
                let outcome = if crate::runes::cut(self, &cast, Rune::Drag).is_ok() { Outcome::Landed } else { Outcome::Missed };
                self.told(cast.ent, cast.ability, None, 0.0, outcome);
            }
        }
    }

    /// Whether `ent` is alive: it has health left and waits on no respawn
    pub fn lives(&self, ent: Entity) -> bool {
        self.health.get(ent).is_ok_and(|health| health.state > 0.0)
            && self.actors.get(ent).is_ok_and(|(.., respawning)| !respawning)
    }

    /// The living hostile `cast` was asked about, and where it stands: an
    /// actor, or a hostile rune, which a blow strikes down
    pub fn foe(&self, cast: &Cast) -> Result<(Entity, Loc), AbilityFailReason> {
        let target = cast.target.ok_or(AbilityFailReason::NoTargets)?;
        if let (Ok((_, &loc, _)), Ok(side)) = (self.runes.get(target), self.rune_sides.get(target)) {
            let hostile = cast.side.is_some_and(|own| own.is_hostile_to(*side));
            return if hostile { Ok((target, loc)) } else { Err(AbilityFailReason::NoTargets) };
        }
        let Ok((&loc, _, _, side, range, _)) = self.actors.get(target) else {
            return Err(AbilityFailReason::NoTargets);
        };
        let hostile = cast.side.zip(side.copied()).is_some_and(|(own, theirs)| own.is_hostile_to(theirs));
        let hidden = targeting::unseen(self.statuses.get(target).ok(), range.copied().unwrap_or_default().0, cast.loc.flat_distance(&loc));
        if !self.lives(target) || !hostile || hidden {
            return Err(AbilityFailReason::NoTargets);
        }
        Ok((target, loc))
    }

    /// What an NPC's swing or strike lands on: the hostile nearest in front
    /// of it by the one rule a player targets by
    /// (`targeting::select_target`), a rune among them, the target it
    /// chases holding a tie. A player's lands on what its client chose by
    /// the same rule.
    fn in_front(&self, cast: &Cast) -> Option<Entity> {
        let side_of = |other: Entity| self.actors.get(other).ok().and_then(|(_, _, _, side, ..)| side.copied())
            .or_else(|| self.rune_sides.get(other).ok().copied());
        let seen = |other: Entity| self.actors.get(other).map_or(true, |(&at, _, _, _, range, _)| {
            self.lives(other) && !targeting::unseen(self.statuses.get(other).ok(), range.copied().unwrap_or_default().0, cast.loc.flat_distance(&at))
        });
        let hostile = |other: Entity| cast.side.zip(side_of(other)).is_some_and(|(own, theirs)| own.is_hostile_to(theirs)) && seen(other);
        targeting::select_target(cast.ent, cast.loc, cast.heading.unwrap_or_default(), &self.nntree, hostile, cast.target)
    }

    /// The leash `ent`'s moves keep inside: an NPC's, round its
    /// engagement's place. A player has none.
    pub fn leash(&self, ent: Entity) -> Option<crate::leap::Leash> {
        crate::leap::Leash::of(ent, &self.leashed, &self.dens)
    }

    /// Takes `spent` of `ent`'s endurance and tells its clients
    fn spend(&mut self, ent: Entity, spent: f32) {
        if spent <= 0.0 {
            return;
        }
        let Ok(mut endurance) = self.endurance.get_mut(ent) else { return };
        endurance.state = (endurance.state - spent).max(0.0);
        self.writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Endurance(*endurance) } });
    }

    /// The skill `ent` is activating, if any
    pub fn activation_of(&self, ent: Entity) -> Option<Activation> {
        self.skills.get(ent).ok().and_then(|(_, activating, _)| activating.0)
    }

    fn clear_activation(&mut self, ent: Entity) {
        if let Ok((_, mut activating, _)) = self.skills.get_mut(ent) {
            activating.0 = None;
        }
    }

    /// Starts `ability`'s recharge for `ent` and tells its clients
    fn recharge(&mut self, ent: Entity, ability: AbilityType) {
        let recharge = Recharge { began: self.game_now(), length: Duration::from_secs_f32(self.tuning.recharge(ability)) };
        if let Ok((_, _, mut recharges)) = self.skills.get_mut(ent) {
            recharges.start(ability, recharge);
        }
        self.writer.write(Do { event: GameEvent::Recharge { ent, ability, began: recharge.began, length: recharge.length } });
    }

    /// Ends the skill `ent` is activating with `outcome` and nothing done:
    /// its endurance stays spent and its recharge starts
    pub fn end(&mut self, ent: Entity, outcome: Outcome) {
        let Some(activation) = self.activation_of(ent) else { return };
        self.clear_activation(ent);
        self.recharge(ent, activation.ability);
        self.told(ent, activation.ability, activation.target, 0.0, outcome);
    }

    /// Interrupts the skill `ent` is activating, if any
    pub fn interrupt(&mut self, ent: Entity) {
        self.end(ent, Outcome::Interrupted);
    }

    /// Tells every client how `ent`'s use of `ability` on `target` ended
    pub fn told(&mut self, ent: Entity, ability: AbilityType, target: Option<Entity>, damage: f32, outcome: Outcome) {
        self.writer.write(Do { event: GameEvent::Resolved { ent, ability, target, damage, outcome, dot: false } });
    }

    /// An attack by `source` resolves on `target` as `ability`'s: a
    /// strike's blow or a swing's, and all it carries. A rune is struck
    /// down. A skill struck out of a Lurk ends it and lands harder by it; a
    /// Surge is spent on a skill strike; the level gap weighs the blow
    /// (`damage::level_factor`) and its damage is rolled within its range
    /// (`Tuning::damage_spread`). A block on the target stops it whole
    /// (`Status::stop`), as a Vigil does one from outside its arc, and the
    /// skill that blocked answers its striker: a Clash stuns it, a Counter
    /// strikes it for its share of the blow. Else its damage lands, then
    /// what it carries: an interrupt, a stun, a drag.
    pub fn attack(&mut self, source: Entity, target: Entity, ability: AbilityType, blow: Blow) -> Outcome {
        let tuning = *self.tuning;
        if self.runes.contains(target) {
            self.writer.write(Do { event: GameEvent::Despawn { ent: target, died: true } });
            self.told(source, ability, Some(target), 0.0, Outcome::Landed);
            return Outcome::Landed;
        }
        let places = self.actors.get(source).ok().map(|(&from, &attrs, ..)| (from, attrs))
            .zip(self.actors.get(target).ok().map(|(&at, &attrs, heading, ..)| (at, attrs, heading.copied())));
        let Some(((from, attrs), (at, target_attrs, facing))) = places.filter(|_| self.lives(target)) else {
            self.told(source, ability, Some(target), 0.0, Outcome::Missed);
            return Outcome::Missed;
        };
        let skill = ability != AbilityType::AutoAttack;
        let mut dealt = blow.damage;

        // An attack gives a lurker away
        if let Some(lurk) = self.statuses.get(source).ok().and_then(|status| status.stance(Stance::Lurk)) {
            landing::update(source, &mut self.statuses, &mut self.commands, &mut self.writer, |status| status.lurk = None);
            if skill {
                dealt *= 1.0 + lurk.pace;
            }
        }
        // A Surge spends what its user's moves built on its next skill strike
        let surge = self.statuses.get(source).map_or(0.0, |status| status.surging());
        if surge > 0.0 && skill && ability.kind() == SkillKind::Strike {
            let tiles = self.surges.get(source).map_or(0, |surged| surged.tiles);
            landing::update(source, &mut self.statuses, &mut self.commands, &mut self.writer, |status| status.surge = None);
            dealt *= 1.0 + surge * tiles as f32;
        }
        dealt *= damage::level_factor(&tuning, attrs.total_level(), target_attrs.total_level());
        if let Ok(mut rolls) = self.rolls.get_mut(source) {
            dealt = damage::spread(dealt, tuning.damage_spread, self.dice.draw(&mut rolls, ("spread", source)).signed());
        }

        let vigil = self.statuses.get(target).is_ok_and(|status| status.is_vigilant()) && targeting::flanked(facing.as_ref(), &at, &from);
        let mut block = None;
        if !vigil && self.statuses.get(target).is_ok_and(|status| status.blocking().is_some()) {
            landing::update(target, &mut self.statuses, &mut self.commands, &mut self.writer, |status| block = status.stop());
        }
        if vigil || block.is_some() {
            self.told(source, ability, Some(target), 0.0, Outcome::Blocked);
            match block.map(|block| (block.ability, block.power)) {
                Some((AbilityType::Clash, seconds)) => self.stun(source, seconds),
                Some((AbilityType::Counter, share)) => self.land(target, source, AbilityType::Counter, dealt * share),
                _ => {}
            }
            return Outcome::Blocked;
        }

        self.land(source, target, ability, dealt);
        if blow.interrupts {
            self.interrupt(target);
        }
        if blow.stun > 0.0 {
            self.stun(target, blow.stun);
        }
        if blow.drag != 0 {
            self.drag(target, from, blow.drag);
        }
        Outcome::Landed
    }

    /// Takes `damage` from `target`'s health as `source`'s `ability` and
    /// tells every client; a share of it comes back to a source in a
    /// Bloodlust (`Status::lifesteal`), and both are in combat.
    pub fn land(&mut self, source: Entity, target: Entity, ability: AbilityType, damage: f32) {
        let Ok(mut health) = self.health.get_mut(target) else { return };
        landing::damage(source, target, ability, damage, false, &mut health, &mut self.writer);
        let lifesteal = self.statuses.get(source).map_or(0.0, |status| status.lifesteal());
        if lifesteal > 0.0 && damage > 0.0 {
            if let Ok(mut own) = self.health.get_mut(source) {
                if own.state > 0.0 {
                    own.state = (own.state + damage * lifesteal).min(own.max);
                    self.writer.write(Do { event: GameEvent::Incremental { ent: source, component: MessageComponent::Health(*own) } });
                }
            }
        }
        for ent in [source, target] {
            if let Ok(mut state) = self.combat.get_mut(ent) {
                enter_combat(ent, &mut state, &self.time, &mut self.writer);
            }
        }
    }

    /// Stuns `ent` for `seconds`, ending the skill it was activating. A
    /// braced actor takes none.
    pub fn stun(&mut self, ent: Entity, seconds: f32) {
        if seconds <= 0.0 || self.statuses.get(ent).is_ok_and(|status| status.is_braced()) {
            return;
        }
        landing::update(ent, &mut self.statuses, &mut self.commands, &mut self.writer, |status| {
            status.hold(seconds);
        });
        self.interrupt(ent);
    }

    /// Drags `ent` up to `tiles` over the ground, toward `from` where
    /// positive, stopping beside it, or away along the line from it,
    /// inside its leash. It slides there at `Tuning::drag_pace`, so a
    /// longer drag takes longer, held in place until it arrives, and the
    /// skill it was activating ends. A braced actor stands, one with
    /// nowhere to go is not moved, and neither is one with no heading: a
    /// nest, which stands where its den does.
    pub fn drag(&mut self, ent: Entity, from: Loc, tiles: i8) {
        if tiles == 0 || !self.lives(ent) || self.statuses.get(ent).is_ok_and(|status| status.is_braced()) {
            return;
        }
        let Ok((&at, _, Some(_), ..)) = self.actors.get(ent) else { return };
        let leash = self.leash(ent);
        let steps = tiles.unsigned_abs() as usize;
        let landing = if tiles > 0 { crate::leap::toward(&self.map, *at, *from, steps, leash) } else { crate::leap::away(&self.map, *at, *from, steps, leash) };
        let Some(landing) = landing else { return };
        let seconds = at.flat_distance(&landing) as f32 / self.tuning.drag_pace;
        landing::update(ent, &mut self.statuses, &mut self.commands, &mut self.writer, |status| {
            status.hold(seconds);
        });
        crate::leap::slide(ent, landing, (seconds * 1000.0).round().min(u16::MAX as f32) as u16, None, &mut self.commands, &mut self.writer);
        self.interrupt(ent);
    }

    /// The game's clock, the one an activation's and a recharge's moments
    /// are on
    pub fn game_now(&self) -> Moment {
        self.runtime.now(&self.time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use common_bevy::plugins::nntree::{NNTreePlugin, NearestNeighbor};
    use qrz::Qrz;

    /// Every `Do` the server has said, in order.
    #[derive(Resource, Default)]
    struct Said(Vec<GameEvent>);

    fn record(mut reader: MessageReader<Do>, mut said: ResMut<Said>) {
        said.0.extend(reader.read().map(|message| message.event.clone()));
    }

    fn arena() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, NNTreePlugin, crate::plugin::CombatPlugin));
        app.insert_resource(Tuning::DEFAULT);
        let tiles = Map::new(qrz::Map::new(1.0, 0.8));
        for q in -12..=12 {
            for r in -12..=12 {
                tiles.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        app.insert_resource(tiles);
        app.insert_resource(common_bevy::components::resources::SpawnPoint(Qrz { q: 0, r: 0, z: 1 }));
        app.init_resource::<Said>();
        app.add_systems(PostUpdate, record);
        app
    }

    /// An actor on `side` standing at `(q, 0)`, facing up the q axis.
    fn actor(app: &mut App, side: Side, q: i32) -> Entity {
        let loc = Loc::new(Qrz { q, r: 0, z: 1 });
        let ent = app.world_mut().spawn((
            loc,
            ActorAttributes::default(),
            side,
            Health { state: 1000.0, max: 1000.0 },
            Endurance::full(100.0),
            Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }),
            CombatState::default(),
        )).id();
        app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, loc));
        ent
    }

    /// An NPC on the wild side at `(q, 0)`, facing up the q axis
    fn npc(app: &mut App, q: i32) -> Entity {
        let ent = actor(app, Side::WILD, q);
        app.world_mut().entity_mut(ent).insert((
            Chase { acquisition_range: 10, leash_distance: 30 },
            EntityType::default(),
            crate::behaviour::Bar::of(common_bevy::archetype::EnemyArchetype::Berserker),
        ));
        ent
    }

    fn game_now(app: &App) -> Moment {
        app.world().resource::<crate::RunTime>().now(app.world().resource::<Time>())
    }

    /// Sets the game's clock in `app` on to `at`. The clock reads its
    /// `Time` in whole milliseconds (`RunTime::now`), so the offset is
    /// taken against that reading, or it lands a millisecond short.
    fn clock_to(app: &mut App, at: Moment) {
        let elapsed = app.world().resource::<Time>().elapsed().as_millis() as u64;
        app.world_mut().resource_mut::<crate::RunTime>().elapsed_offset = at.since(Moment::from_millis(elapsed)).as_millis();
    }

    /// Runs a frame and returns what the server said in it
    fn frame(app: &mut App) -> Vec<GameEvent> {
        app.world_mut().resource_mut::<Said>().0.clear();
        app.update();
        std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
    }

    fn ask(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Vec<GameEvent> {
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent, ability, target } });
        frame(app)
    }

    fn activating(app: &App, ent: Entity) -> Option<Activation> {
        app.world().get::<Activating>(ent).unwrap().0
    }

    /// Runs the clock to the end of the skill `ent` is activating and
    /// returns what the server said as it ended
    fn finish(app: &mut App, ent: Entity) -> Vec<GameEvent> {
        let ends = activating(app, ent).expect("activating").ends();
        clock_to(app, ends);
        frame(app)
    }

    /// [`ask`], then its activation run to its end
    fn used(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Vec<GameEvent> {
        let mut said = ask(app, ent, ability, target);
        if activating(app, ent).is_some() {
            said.extend(finish(app, ent));
        }
        said
    }

    /// Runs the clock on past `ability`'s recharge
    fn recharged(app: &mut App, ability: AbilityType) {
        let ready = game_now(app) + Duration::from_secs_f32(Tuning::DEFAULT.recharge(ability));
        clock_to(app, ready);
        app.update();
    }

    /// Why the gate refuses `ent` the use of `ability`, asked of the gate
    /// itself; None where it has nothing to say, or lets it through.
    fn refused(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Option<AbilityFailReason> {
        app.world_mut()
            .run_system_once(move |abilities: Abilities| {
                let now = abilities.game_now();
                let mut cast = abilities.caster(ent, ability, target)?;
                let status = abilities.statuses.get(ent).ok().copied().unwrap_or_default();
                cast.target_loc = abilities.foe(&cast).ok().map(|(_, loc)| loc);
                let foe = cast.target_loc.map(|loc| (cast.loc.distance(&loc), targeting::faces(cast.heading.as_ref(), &cast.loc, &loc)));
                admits(&abilities.tuning, ability, &abilities.standing(ent, ability, &status, now), &cast.attrs, cast.reach, foe)
                    .and_then(|_| abilities.room(&cast)).err()
            })
            .unwrap()
    }

    /// How `by`'s use of `with` ended in `said`, and for what damage
    fn ended(said: &[GameEvent], by: Entity, with: AbilityType) -> Option<(Outcome, f32)> {
        said.iter().find_map(|event| match event {
            GameEvent::Resolved { ent, ability, outcome, damage, .. } if (*ent, *ability) == (by, with) => Some((*outcome, *damage)),
            _ => None,
        })
    }

    fn outcome(said: &[GameEvent], by: Entity, with: AbilityType) -> Option<Outcome> {
        ended(said, by, with).map(|(outcome, _)| outcome)
    }

    fn health(app: &App, ent: Entity) -> f32 {
        app.world().get::<Health>(ent).unwrap().state
    }

    fn endurance(app: &App, ent: Entity) -> f32 {
        app.world().get::<Endurance>(ent).unwrap().state
    }

    fn status(app: &App, ent: Entity) -> Status {
        app.world().get::<Status>(ent).copied().unwrap_or_default()
    }

    fn q_of(app: &App, ent: Entity) -> i32 {
        app.world().get::<Loc>(ent).unwrap().q
    }

    fn turned_to(app: &mut App, ent: Entity, q: i32) {
        app.world_mut().entity_mut(ent).insert(Heading::from_hex(Qrz { q, r: 0, z: 0 }));
    }

    fn moved_to(app: &mut App, ent: Entity, q: i32) {
        app.world_mut().entity_mut(ent).insert(Loc::new(Qrz { q, r: 0, z: 1 }));
    }

    /// The rune standing at `(q, 0)`, if one does
    fn rune_at(app: &mut App, q: i32) -> Option<Entity> {
        let mut runes = app.world_mut().query::<(Entity, &Loc, &crate::runes::RuneStone)>();
        runes.iter(app.world()).find(|(_, loc, _)| loc.q == q && loc.r == 0).map(|(ent, ..)| ent)
    }

    /// A caster at the origin and a foe a tile ahead, each facing the other
    fn duel(app: &mut App) -> (Entity, Entity) {
        let caster = actor(app, Side::PLAYERS, 0);
        let foe = actor(app, Side::WILD, 1);
        turned_to(app, foe, -1);
        app.update();
        (caster, foe)
    }

    #[test]
    fn a_strike_needs_a_living_hostile_within_reach_and_arc_and_lands_when_its_activation_ends() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let ally = actor(&mut app, Side::PLAYERS, 1);
        let far = actor(&mut app, Side::WILD, 3);
        let behind = actor(&mut app, Side::WILD, -1);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        let bite = |app: &mut App, target| refused(app, caster, AbilityType::Frenzy, target);
        assert_eq!(bite(&mut app, None), Some(AbilityFailReason::NoTargets), "no target named");
        assert_eq!(bite(&mut app, Some(ally)), Some(AbilityFailReason::NoTargets), "an ally is no target");
        assert_eq!(bite(&mut app, Some(far)), Some(AbilityFailReason::OutOfRange));
        assert_eq!(bite(&mut app, Some(behind)), Some(AbilityFailReason::NotFacing));

        let said = ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        let told = said.iter().any(|event| matches!(event, GameEvent::Activate { ent, activation } if *ent == caster && activation.ability == AbilityType::Frenzy && activation.target == Some(near)));
        assert!(told, "its activation is told once, whole");
        assert!(endurance(&app, caster) < 100.0, "it is paid for as it starts");
        assert_eq!(health(&app, near), 1000.0, "nothing lands while it activates");
        assert_eq!(ended(&said, caster, AbilityType::Frenzy), None);

        let said = finish(&mut app, caster);
        let (outcome, damage) = ended(&said, caster, AbilityType::Frenzy).expect("it ends");
        assert_eq!(outcome, Outcome::Landed);
        assert!(damage > 0.0 && health(&app, near) < 1000.0, "its blow lands at once as the activation ends");
        assert!(said.iter().any(|event| matches!(event, GameEvent::Recharge { ent, ability: AbilityType::Frenzy, .. } if *ent == caster)), "and its recharge is told");
        assert_eq!(activating(&app, caster), None);
    }

    #[test]
    fn a_strike_misses_a_target_that_left_reach_or_arc_before_its_activation_ended() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        ask(&mut app, caster, AbilityType::Overpower, Some(near));
        let paid = endurance(&app, caster);
        moved_to(&mut app, near, 5);
        let said = finish(&mut app, caster);
        assert_eq!(outcome(&said, caster, AbilityType::Overpower), Some(Outcome::Missed), "out of reach as it ends");
        assert_eq!(health(&app, near), 1000.0);
        assert_eq!(endurance(&app, caster), paid, "a miss keeps its endurance spent");
        assert_eq!(refused(&mut app, caster, AbilityType::Overpower, Some(near)), Some(AbilityFailReason::Recharging), "and recharges as one that landed");

        moved_to(&mut app, near, 1);
        ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        moved_to(&mut app, near, -1);
        let said = finish(&mut app, caster);
        assert_eq!(outcome(&said, caster, AbilityType::Frenzy), Some(Outcome::Missed), "round past its arc as it ends");
    }

    #[test]
    fn activating_commits_its_user_and_a_recharge_locks_its_own_skill_alone() {
        let mut app = arena();
        let (caster, near) = duel(&mut app);

        ask(&mut app, caster, AbilityType::Overpower, Some(near));
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::Activating), "it uses no other skill");
        let said = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert_eq!(ended(&said, caster, AbilityType::AutoAttack), None, "and swings no auto-attack");
        assert_eq!(refused(&mut app, caster, AbilityType::Brace, None), None, "a stance alone is taken mid-activation");
        ask(&mut app, caster, AbilityType::Brace, None);
        assert_eq!(refused(&mut app, caster, AbilityType::Brace, None), Some(AbilityFailReason::Recharging), "it took hold");
        assert_eq!(activating(&app, caster).map(|activation| activation.ability), Some(AbilityType::Overpower), "and leaves the skill running");

        finish(&mut app, caster);
        assert_eq!(refused(&mut app, caster, AbilityType::Overpower, Some(near)), Some(AbilityFailReason::Recharging));
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), None, "another skill is free");

        let spent = endurance(&app, caster);
        let said = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert_eq!(outcome(&said, caster, AbilityType::AutoAttack), Some(Outcome::Landed), "a due swing lands at once");
        let again = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert_eq!(ended(&again, caster, AbilityType::AutoAttack), None, "the next is not due yet");
        assert_eq!(endurance(&app, caster), spent, "a swing costs no endurance");

        recharged(&mut app, AbilityType::Overpower);
        assert_eq!(refused(&mut app, caster, AbilityType::Overpower, Some(near)), None, "recharged, it is ready again");
    }

    #[test]
    fn a_skill_is_refused_without_its_endurance() {
        let mut app = arena();
        let (caster, near) = duel(&mut app);
        let cost = Tuning::DEFAULT.cost(AbilityType::Frenzy);
        app.world_mut().get_mut::<Endurance>(caster).unwrap().state = cost / 2.0;
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::NoEndurance));
        ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        assert_eq!(activating(&app, caster), None, "nothing starts");
        assert_eq!(endurance(&app, caster), cost / 2.0, "and nothing is spent");

        app.world_mut().get_mut::<Endurance>(caster).unwrap().state = cost;
        ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        assert!(activating(&app, caster).is_some(), "holding its price, it is used");
    }

    #[test]
    fn an_interrupt_ends_an_activation_and_starts_its_recharge() {
        let mut app = arena();
        let (heavy, quick) = duel(&mut app);

        ask(&mut app, heavy, AbilityType::Overpower, Some(quick));
        let paid = endurance(&app, heavy);
        ask(&mut app, quick, AbilityType::Feint, Some(heavy));
        let said = finish(&mut app, quick);
        assert_eq!(outcome(&said, quick, AbilityType::Feint), Some(Outcome::Landed));
        assert_eq!(ended(&said, heavy, AbilityType::Overpower), Some((Outcome::Interrupted, 0.0)), "the skill it was activating ends with nothing done");
        assert_eq!(activating(&app, heavy), None);
        assert_eq!(health(&app, quick), 1000.0, "and never lands");
        assert_eq!(endurance(&app, heavy), paid, "its endurance stays spent");
        assert_eq!(refused(&mut app, heavy, AbilityType::Overpower, Some(quick)), Some(AbilityFailReason::Recharging), "and its recharge starts");

        // A plain blow never interrupts
        ask(&mut app, heavy, AbilityType::Defy, Some(quick));
        ask(&mut app, quick, AbilityType::Frenzy, Some(heavy));
        let said = finish(&mut app, quick);
        assert_eq!(outcome(&said, quick, AbilityType::Frenzy), Some(Outcome::Landed));
        assert!(activating(&app, heavy).is_some(), "a plain blow leaves it activating");
    }

    #[test]
    fn a_step_of_its_own_cancels_an_activation() {
        let mut app = arena();
        let (caster, near) = duel(&mut app);
        ask(&mut app, caster, AbilityType::Overpower, Some(near));
        app.world_mut().trigger(crate::Cancel { ent: caster });
        let said = frame(&mut app);
        assert_eq!(ended(&said, caster, AbilityType::Overpower), Some((Outcome::Cancelled, 0.0)));
        assert_eq!(activating(&app, caster), None);
        assert_eq!(refused(&mut app, caster, AbilityType::Overpower, Some(near)), Some(AbilityFailReason::Recharging), "cancelled, it recharges as one that landed");
        assert_eq!(health(&app, near), 1000.0);
    }

    #[test]
    fn a_block_stops_the_next_attack_and_not_the_one_after() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let first = actor(&mut app, Side::WILD, 1);
        let second = actor(&mut app, Side::WILD, 1);
        for attacker in [first, second] {
            turned_to(&mut app, attacker, -1);
        }
        app.update();

        ask(&mut app, defender, AbilityType::Parry, None);
        assert!(status(&app, defender).blocking().is_some(), "a block takes hold at once");
        let said = used(&mut app, first, AbilityType::Frenzy, Some(defender));
        assert_eq!(ended(&said, first, AbilityType::Frenzy), Some((Outcome::Blocked, 0.0)), "the next attack is stopped whole");
        assert_eq!(health(&app, defender), 1000.0);
        let said = used(&mut app, second, AbilityType::Frenzy, Some(defender));
        assert_eq!(outcome(&said, second, AbilityType::Frenzy), Some(Outcome::Landed), "the one after lands");
        assert!(health(&app, defender) < 1000.0);
    }

    #[test]
    fn a_block_stops_a_swing_and_all_a_blow_carried() {
        let mut app = arena();
        let (defender, attacker) = duel(&mut app);

        ask(&mut app, defender, AbilityType::Parry, None);
        let said = ask(&mut app, attacker, AbilityType::AutoAttack, Some(defender));
        assert_eq!(ended(&said, attacker, AbilityType::AutoAttack), Some((Outcome::Blocked, 0.0)), "an auto-attack is blocked as a strike is");

        recharged(&mut app, AbilityType::Parry);
        ask(&mut app, defender, AbilityType::Parry, None);
        let said = used(&mut app, attacker, AbilityType::Kick, Some(defender));
        assert_eq!(ended(&said, attacker, AbilityType::Kick), Some((Outcome::Blocked, 0.0)));
        app.update();
        assert_eq!(q_of(&app, defender), 0, "its knockback is stopped with it");
    }

    #[test]
    fn a_clash_stuns_the_striker_it_blocks_and_a_counter_strikes_it() {
        let mut app = arena();
        let (defender, attacker) = duel(&mut app);

        ask(&mut app, defender, AbilityType::Clash, None);
        used(&mut app, attacker, AbilityType::Frenzy, Some(defender));
        assert!(status(&app, attacker).is_held(), "a Clash stuns its striker");
        assert_eq!(refused(&mut app, attacker, AbilityType::Overpower, Some(defender)), Some(AbilityFailReason::Held), "which uses no skill held");

        app.world_mut().entity_mut(attacker).remove::<Status>();
        ask(&mut app, defender, AbilityType::Counter, None);
        let said = used(&mut app, attacker, AbilityType::Overpower, Some(defender));
        assert_eq!(ended(&said, attacker, AbilityType::Overpower), Some((Outcome::Blocked, 0.0)));
        let (outcome, damage) = ended(&said, defender, AbilityType::Counter).expect("its return");
        assert_eq!(outcome, Outcome::Landed);
        assert!(damage > 0.0 && health(&app, attacker) < 1000.0, "a Counter strikes its striker at once");
        assert_eq!(health(&app, defender), 1000.0);
    }

    #[test]
    fn taking_a_stance_ends_the_one_before() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        app.update();
        let said = ask(&mut app, caster, AbilityType::Bloodlust, None);
        assert!(said.iter().any(|event| matches!(event, GameEvent::Activate { activation, .. } if activation.length.is_zero())), "a stance has no activation");
        assert_eq!(activating(&app, caster), None);
        assert_eq!(status(&app, caster).held_stance(), Some(Stance::Bloodlust), "and takes hold at once");
        assert_eq!(refused(&mut app, caster, AbilityType::Bloodlust, None), Some(AbilityFailReason::Recharging));
        ask(&mut app, caster, AbilityType::Brace, None);
        assert_eq!(status(&app, caster).held_stance(), Some(Stance::Brace));
        assert_eq!(status(&app, caster).lifesteal(), 0.0, "the Bloodlust is over");
    }

    #[test]
    fn a_backstab_lands_only_from_past_its_targets_forward_faces() {
        let mut app = arena();
        let (caster, target) = duel(&mut app);
        let said = used(&mut app, caster, AbilityType::Backstab, Some(target));
        assert_eq!(ended(&said, caster, AbilityType::Backstab), Some((Outcome::Missed, 0.0)), "from in front it misses");

        recharged(&mut app, AbilityType::Backstab);
        turned_to(&mut app, target, 1);
        let said = used(&mut app, caster, AbilityType::Backstab, Some(target));
        assert_eq!(outcome(&said, caster, AbilityType::Backstab), Some(Outcome::Landed), "from behind it lands");
    }

    #[test]
    fn a_vigil_blocks_every_attack_from_outside_its_arc_and_none_from_within() {
        let mut app = arena();
        let watcher = actor(&mut app, Side::PLAYERS, 0);
        let behind = actor(&mut app, Side::WILD, -1);
        let before = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, before, -1);
        app.update();
        ask(&mut app, watcher, AbilityType::Vigil, None);
        let said = ask(&mut app, behind, AbilityType::AutoAttack, Some(watcher));
        assert_eq!(ended(&said, behind, AbilityType::AutoAttack), Some((Outcome::Blocked, 0.0)));
        let said = used(&mut app, behind, AbilityType::Backstab, Some(watcher));
        assert_eq!(ended(&said, behind, AbilityType::Backstab), Some((Outcome::Blocked, 0.0)), "as many as come");
        let said = used(&mut app, before, AbilityType::Frenzy, Some(watcher));
        assert_eq!(outcome(&said, before, AbilityType::Frenzy), Some(Outcome::Landed), "one from in front lands");
    }

    #[test]
    fn a_kick_knocks_its_target_back_and_ends_what_it_was_activating() {
        let mut app = arena();
        let (caster, target) = duel(&mut app);
        ask(&mut app, target, AbilityType::Overpower, Some(caster));
        let said = used(&mut app, caster, AbilityType::Kick, Some(target));
        app.update();
        assert!(q_of(&app, target) > 1, "knocked back along the line from its striker");
        assert_eq!(ended(&said, target, AbilityType::Overpower), Some((Outcome::Interrupted, 0.0)), "a drag interrupts");
        assert!(status(&app, target).is_held(), "held while it slides");

        let mut app = arena();
        let (caster, target) = duel(&mut app);
        ask(&mut app, target, AbilityType::Brace, None);
        ask(&mut app, target, AbilityType::Overpower, Some(caster));
        used(&mut app, caster, AbilityType::Kick, Some(target));
        app.update();
        assert_eq!(q_of(&app, target), 1, "a braced target stands");
        assert!(activating(&app, target).is_some(), "and is not interrupted");
    }

    #[test]
    fn a_longer_drag_holds_its_target_longer() {
        let held = |from: i32| {
            let mut app = arena();
            let source = actor(&mut app, Side::PLAYERS, 0);
            let target = actor(&mut app, Side::WILD, from);
            app.update();
            let at = *app.world().get::<Loc>(source).unwrap();
            app.world_mut().run_system_once(move |mut abilities: Abilities| abilities.drag(target, at, i8::MAX)).unwrap();
            app.update();
            status(&app, target).held.map_or(0.0, |held| held.remaining)
        };
        assert!(held(3) > 0.0, "held while it slides");
        assert!(held(9) > held(3), "and longer the further it is dragged");
    }

    #[test]
    fn a_yank_drags_a_target_beyond_reach_to_its_user() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        app.world_mut().entity_mut(caster).insert(ActorAttributes::new(0, 0, 0, -25, 0, 0, 0, 0, 0));
        let target = actor(&mut app, Side::WILD, 4);
        app.update();
        let said = used(&mut app, caster, AbilityType::Yank, Some(target));
        app.update();
        assert_eq!(outcome(&said, caster, AbilityType::Yank), Some(Outcome::Landed));
        assert_eq!(q_of(&app, target), 1, "beside its user");
    }

    #[test]
    fn a_punish_lands_harder_on_a_target_mid_activation() {
        let landed = |caught: bool| {
            let mut app = arena();
            let (caster, target) = duel(&mut app);
            if caught {
                ask(&mut app, target, AbilityType::Overpower, Some(caster));
            }
            let said = used(&mut app, caster, AbilityType::Punish, Some(target));
            ended(&said, caster, AbilityType::Punish).expect("it lands").1
        };
        let spread = 1.0 + 2.0 * Tuning::DEFAULT.damage_spread;
        assert!(landed(true) > landed(false) * spread, "past what its roll alone could add");
    }

    #[test]
    fn a_leap_clears_a_foe_in_reach_and_dives_onto_one_beyond_it() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();
        let said = ask(&mut app, caster, AbilityType::Leap, Some(near));
        app.update();
        assert!((q_of(&app, caster) - q_of(&app, near)).abs() > 2, "clear of a foe in its reach");
        assert_eq!(ended(&said, caster, AbilityType::Leap), None, "striking nothing");

        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let far = actor(&mut app, Side::WILD, 4);
        app.update();
        let said = ask(&mut app, caster, AbilityType::Leap, Some(far));
        app.update();
        assert!(q_of(&app, caster) > 0 && q_of(&app, caster) < 4, "toward one beyond it, never onto its tile");
        assert_eq!(outcome(&said, caster, AbilityType::Leap), Some(Outcome::Landed), "striking as it arrives");
    }

    #[test]
    fn a_skill_out_of_a_lurk_lands_harder_and_ends_it() {
        let landed = |lurking: bool| {
            let mut app = arena();
            let caster = actor(&mut app, Side::PLAYERS, 0);
            app.world_mut().entity_mut(caster).insert(ActorAttributes::new(0, 0, 0, 0, 0, 0, -25, 0, 0));
            let target = actor(&mut app, Side::WILD, 1);
            app.update();
            if lurking {
                ask(&mut app, caster, AbilityType::Lurk, None);
            }
            let said = used(&mut app, caster, AbilityType::Feint, Some(target));
            assert!(!status(&app, caster).is_lurking(), "an attack gives it away");
            ended(&said, caster, AbilityType::Feint).expect("it lands").1
        };
        let spread = 1.0 + 2.0 * Tuning::DEFAULT.damage_spread;
        assert!(landed(true) > landed(false) * spread);
    }

    #[test]
    fn an_npc_strikes_what_stands_in_front_of_its_quarry_and_its_quarry_on_a_tie() {
        let mut app = arena();
        let hunter = npc(&mut app, 0);
        let quarry = actor(&mut app, Side::PLAYERS, 2);
        let guard = actor(&mut app, Side::PLAYERS, 1);
        app.update();
        used(&mut app, hunter, AbilityType::Frenzy, Some(quarry));
        assert!(health(&app, guard) < 1000.0, "the one standing in front takes it");
        assert_eq!(health(&app, quarry), 1000.0, "not the one it chases behind");

        let mut app = arena();
        let hunter = npc(&mut app, 0);
        let quarry = actor(&mut app, Side::PLAYERS, 1);
        let beside = actor(&mut app, Side::PLAYERS, 1);
        app.update();
        used(&mut app, hunter, AbilityType::Frenzy, Some(quarry));
        assert!(health(&app, quarry) < 1000.0, "of two on one tile, the one it chases");
        assert_eq!(health(&app, beside), 1000.0);
    }

    #[test]
    fn a_rune_between_takes_an_npcs_blow() {
        let mut app = arena();
        let ahead = Tuning::DEFAULT.drag_distance.round() as i32;
        let caster = actor(&mut app, Side::PLAYERS, 0);
        app.update();
        used(&mut app, caster, AbilityType::Drag, None);
        app.update();
        let rune = rune_at(&mut app, ahead).expect("the rune");
        // Facing back along the q axis, the rune between it and its quarry
        let hunter = npc(&mut app, ahead + 2);
        turned_to(&mut app, hunter, -1);
        app.update();
        let said = used(&mut app, hunter, AbilityType::Frenzy, Some(caster));
        assert!(said.iter().any(|event| matches!(event, GameEvent::Despawn { ent, died: true } if *ent == rune)), "the rune takes it and is struck down");
        assert_eq!(health(&app, caster), 1000.0, "not its caster beyond");
    }

    #[test]
    fn a_drag_rune_is_cut_ahead_and_each_pulse_winds_up_then_pulls_the_foes_still_inside() {
        let mut app = arena();
        let tuning = Tuning::DEFAULT;
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let ahead = tuning.drag_distance.round() as i32;
        let reach = crate::runes::reach_of(&tuning, &ActorAttributes::default(), Rune::Drag);
        let caught = actor(&mut app, Side::WILD, ahead + reach);
        let leaver = actor(&mut app, Side::WILD, ahead - reach);
        let beyond = actor(&mut app, Side::WILD, ahead + reach + 1);
        app.update();
        let mut said = used(&mut app, caster, AbilityType::Drag, None);
        assert_eq!(outcome(&said, caster, AbilityType::Drag), Some(Outcome::Landed));
        said.extend(frame(&mut app));
        said.extend(frame(&mut app));
        let rune = rune_at(&mut app, ahead).expect("cut on the tile its distance ahead as its activation ends");
        let shown = |on: Entity| said.iter().any(|event| matches!(event, GameEvent::Activate { ent, activation } if *ent == rune && activation.target == Some(on) && !activation.length.is_zero()));
        assert!(shown(caught) && shown(leaver), "its pulse winds up on every foe then inside");
        assert!(!shown(beyond), "and on none beyond its reach");
        assert_eq!(q_of(&app, caught), ahead + reach, "nothing moves while it winds up");

        // One walks out of its reach before the wind-up ends
        app.world_mut().entity_mut(leaver).insert(Loc::new(Qrz { q: ahead, r: -10, z: 1 }));
        let ends = game_now(&app) + Duration::from_secs_f32(tuning.rune_windup);
        clock_to(&mut app, ends);
        let said = frame(&mut app);
        app.update();
        let distance = |app: &App, ent: Entity| app.world().get::<Loc>(ent).unwrap().flat_distance(app.world().get::<Loc>(rune).unwrap());
        assert_eq!(distance(&app, caught), 1, "the one still inside is pulled all the way in");
        assert_eq!(distance(&app, leaver), 10, "the one that left is let go");
        let ended_on = |on: Entity| said.iter().find_map(|event| match event {
            GameEvent::Resolved { ent, target, outcome, .. } if *ent == rune && *target == Some(on) => Some(*outcome),
            _ => None,
        });
        assert_eq!((ended_on(caught), ended_on(leaver)), (Some(Outcome::Landed), Some(Outcome::Missed)));
    }

    #[test]
    fn a_rune_takes_no_tile_a_rune_holds_and_one_of_a_kind_lifts_the_one_before() {
        let mut app = arena();
        let ahead = Tuning::DEFAULT.drag_distance.round() as i32;
        let caster = actor(&mut app, Side::PLAYERS, 0);
        app.update();
        used(&mut app, caster, AbilityType::Drag, None);
        app.update();
        let first = rune_at(&mut app, ahead).expect("the first rune");
        recharged(&mut app, AbilityType::Drag);
        assert_eq!(refused(&mut app, caster, AbilityType::Drag, None), Some(AbilityFailReason::NoRoom), "its tile holds a rune");

        moved_to(&mut app, caster, 1);
        let said = used(&mut app, caster, AbilityType::Drag, None);
        app.update();
        assert!(rune_at(&mut app, ahead + 1).is_some(), "cut a tile further on");
        assert!(said.iter().any(|event| matches!(event, GameEvent::Despawn { ent, died: false } if *ent == first)), "the control rune before is lifted");
    }

    #[test]
    fn a_blow_on_a_rune_strikes_it_down_as_it_lands() {
        let mut app = arena();
        let ahead = Tuning::DEFAULT.drag_distance.round() as i32;
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let foe = actor(&mut app, Side::WILD, ahead + 1);
        turned_to(&mut app, foe, -1);
        app.update();
        used(&mut app, caster, AbilityType::Drag, None);
        app.update();
        let rune = rune_at(&mut app, ahead).expect("the rune");
        let said = ask(&mut app, foe, AbilityType::AutoAttack, Some(rune));
        assert!(said.iter().any(|event| matches!(event, GameEvent::Despawn { ent, died: true } if *ent == rune)), "struck down at once");
    }
}
