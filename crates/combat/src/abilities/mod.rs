//! Every ability an actor uses goes through one gate, [`Abilities::cast`].
//! The gate asks, in order and the same of every ability: is the caster
//! alive; is it out of recovery, or taking the combo it was offered, or
//! reacting through the recovery (an auto-attack asks its clock instead);
//! does it strike a living hostile within the ability's reach and its arc.
//! Nothing it holds refuses it. Then the ability's own effect runs, its
//! endurance is paid, the clients are told, a strike across
//! the caster's line breaks its stride, and the recovery starts, longer
//! for an actor whose endurance is spent. A skill costs endurance; an
//! auto-attack is free.
//!
//! What each ability costs, how long its recovery runs, how far it
//! reaches, what it offers next and whether it is a reaction are the
//! ability's own to say (`Tuning::cost`, `Tuning::recovery`,
//! `AbilityType::reach`, `combo`, `is_reaction`). Its module here holds
//! only what it does.
//!
//! One system, [`use_abilities`], runs all of it in a stated order: the
//! auto-attacks come due, then what players asked for, then what each
//! NPC's skills channel chooses. An ability is handled the frame it is
//! asked for, whoever asks, and a due swing the frame its target comes
//! within its reach.

pub mod auto_attack;
pub mod counter;
pub mod feint;
pub mod frenzy;
pub mod leap;
pub mod npc;
pub mod overpower;
pub mod parry;
pub mod punish;
pub mod stride;

use std::time::Duration;

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    components::{
        behaviour::Side,
        engagement::{Engagement, EngagementMember},
        intimidation::Intimidation,
        heading::Heading,
        reaction_queue::{QueuedThreat, ReactionQueue},
        recovery::GlobalRecovery,
        resources::{Endurance, Health, RespawnTimer},
        status::Status,
        target::Target,
        ActorAttributes, AttackRange, Loc, Swing,
    },
    components::entity_type::EntityType,
    message::{AbilityType, ClearType, Component as MessageComponent, Do, Event as GameEvent, Try},
    resources::map::Map,
    systems::{
        combat::{queue::clear_threats, combos::{may_use, recovery_after}},
        targeting,
    },
};

use crate::{
    behaviour::{chase::Chase, perception::{Sight, Skill}},
    landing,
};
use common_bevy::tuning::Tuning;

/// Why the gate refused an ability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFailReason {
    NoTargets,
    OnCooldown,
    OutOfRange,
    /// The target stands outside the caster's arc
    NotFacing,
}

/// One use of an ability the gate has let through: who uses it, from where,
/// at whom and when.
pub struct Cast {
    pub ent: Entity,
    /// The moment it was used on the game clock: a player's press as its
    /// client stamped it, or as it arrived where that had passed
    /// ([`Press`]); an NPC's now. A reaction's band is judged here.
    pub at: Duration,
    pub loc: Loc,
    pub attrs: ActorAttributes,
    pub side: Option<Side>,
    /// The caster's own reach, in tiles
    pub reach: i32,
    /// A strike's target, a living hostile within reach and arc; for any
    /// other ability, whoever it was asked about, unchecked
    pub target: Option<Entity>,
    pub target_loc: Option<Loc>,
}

impl Cast {
    /// The actor a strike lands on and where it stands
    pub fn struck(&self) -> Result<(Entity, Loc), AbilityFailReason> {
        self.target.zip(self.target_loc).ok_or(AbilityFailReason::NoTargets)
    }
}

/// When an actor last used a skill, on the server's clock: what anyone
/// watching sees, its clip starting. An auto-attack is no skill.
#[derive(Clone, Component, Copy, Debug)]
pub struct LastSkill(pub Duration);

/// A skill's strike made whole and at once ([`Abilities::strike`])
pub const WHOLE: [(f32, Duration); 1] = [(1.0, Duration::ZERO)];

/// A strike of `damage`, and the share of its target's speed it binds away:
/// `Tuning::intimidation_share` harder and `intimidation_slow` binding with a full Intimidation
/// bank `released` into it, as it is without.
fn released_into(tuning: &Tuning, damage: f32, released: bool) -> (f32, f32) {
    if released { (damage * (1.0 + tuning.intimidation_share), tuning.intimidation_slow) } else { (damage, 0.0) }
}

/// What the gate asks of `ability` before it does anything, `reacting`
/// where this use is a reaction (`AbilityType::reacts`), in order: out
/// of `prior`, the recovery its caster is in, or let through it (an
/// auto-attack's own clock is asked apart); for one with a reach out of
/// `reach`, a target at `foe`'s distance and inside its arc, None for no
/// target. An NPC's skills channel asks the
/// same of what it perceives, so it never weighs a skill the gate refuses.
pub fn admits(ability: AbilityType, reacting: bool, prior: Option<&GlobalRecovery>, attrs: &ActorAttributes, reach: i32, foe: Option<(i32, bool)>) -> Result<(), AbilityFailReason> {
    if ability != AbilityType::AutoAttack && !may_use(ability, reacting, prior, attrs) {
        return Err(AbilityFailReason::OnCooldown);
    }
    if let Some(within) = ability.reach(reach) {
        let (distance, in_arc) = foe.ok_or(AbilityFailReason::NoTargets)?;
        if !within.contains(&distance) {
            return Err(AbilityFailReason::OutOfRange);
        }
        if !in_arc {
            return Err(AbilityFailReason::NotFacing);
        }
    }
    Ok(())
}

/// Everything an ability reads or changes as it is used.
#[derive(SystemParam)]
pub struct Abilities<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub writer: MessageWriter<'w, Do>,
    pub actors: Query<'w, 's, (
        &'static Loc,
        &'static ActorAttributes,
        &'static Health,
        Option<&'static Heading>,
        Option<&'static Side>,
        Option<&'static AttackRange>,
        Has<RespawnTimer>,
    )>,
    pub endurance: Query<'w, 's, &'static mut Endurance>,
    pub recoveries: Query<'w, 's, &'static GlobalRecovery>,
    pub queues: Query<'w, 's, &'static mut ReactionQueue>,
    pub statuses: Query<'w, 's, &'static mut Status>,
    pub swings: Query<'w, 's, &'static mut Swing>,
    pub intimidations: Query<'w, 's, &'static mut Intimidation>,
    pub targets: Query<'w, 's, (Entity, &'static Target)>,
    pub npcs: Query<'w, 's, (Entity, &'static EntityType, &'static crate::behaviour::Bar), With<Chase>>,
    pub minds: Query<'w, 's, (&'static Skill, &'static mut Sight)>,
    pub leashed: Query<'w, 's, (&'static Chase, &'static EngagementMember)>,
    pub dens: Query<'w, 's, &'static Loc, With<Engagement>>,
    pub engagements: Query<'w, 's, &'static Engagement>,
    pub last_skills: Query<'w, 's, &'static LastSkill>,
    pub kinds: Query<'w, 's, &'static EntityType>,
    pub map: Res<'w, Map>,
    pub time: Res<'w, Time>,
    pub runtime: Res<'w, crate::RunTime>,
    pub dice: Res<'w, crate::dice::Dice>,
    pub tuning: Res<'w, common_bevy::tuning::Tuning>,
    pub mind_set: Res<'w, crate::behaviour::mind::Minds>,
    pub rolls: Query<'w, 's, &'static mut crate::dice::Rolls>,
    pub decisions: Option<ResMut<'w, crate::behaviour::Decisions>>,
}

/// Uses every ability due or asked for this frame: the auto-attacks that
/// have come due, then what players asked, in the order asked, then what
/// each NPC chooses.
pub fn use_abilities(mut reader: MessageReader<Try>, mut held: Local<Vec<Press>>, mut abilities: Abilities) {
    // Swings go first: a Leap moves its user as the frame ends, so a swing
    // after it would still strike from where it stood
    abilities.swing();
    let now = abilities.game_now();
    for message in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target, at, .. } } = message else { continue };
        held.retain(|press| (press.ent, press.ability) != (*ent, *ability));
        held.push(Press { ent: *ent, ability: *ability, target: *target, pressed: *at, arrived: now });
    }
    held.sort_by_key(Press::at);
    let due = held.partition_point(|press| press.at() <= now);
    for press in held.drain(..due).collect::<Vec<_>>() {
        abilities.cast(press.ent, press.ability, press.target, press.pressed, press.arrived).ok();
    }
    abilities.skills();
}

/// A player's press the server holds until its clock reaches [`Press::at`].
/// A client's clock leads the server's by the trip there, so a press made
/// on time arrives ahead of its moment and is judged at it, and one made
/// late is judged as late as it came, never earlier. One is held for each
/// actor's ability, the latest asked.
pub struct Press {
    ent: Entity,
    ability: AbilityType,
    target: Option<Entity>,
    /// The moment its client stamped
    pressed: Duration,
    /// The server's clock as it came
    arrived: Duration,
}

impl Press {
    /// The moment it is judged at: the later of when it was pressed and
    /// when it arrived
    fn at(&self) -> Duration {
        self.pressed.max(self.arrived)
    }
}

impl Abilities<'_, '_> {
    /// Uses `ability` for `ent` now if the gate lets it. Returns whether
    /// it was used; a refusal is told to no one.
    pub fn ask(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>) -> bool {
        let now = self.game_now();
        self.cast(ent, ability, asked, now, now).is_ok()
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

    /// The gate. `asked` is the actor the ability is used on: a strike's
    /// target, a Leap's; `pressed` the moment it was used, and `arrived`
    /// when the server heard of it, judged at the later ([`Press`]). Errs
    /// with the reason it was refused, or with none where there is nothing
    /// to tell: a dead caster, a swing not yet due.
    fn cast(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>, pressed: Duration, arrived: Duration) -> Result<(), Option<AbilityFailReason>> {
        let at = pressed.max(arrived);
        let tuning = *self.tuning;
        let Ok((&loc, &attrs, _, heading, side, range, dead)) = self.actors.get(ent) else { return Err(None) };
        if dead {
            return Err(None);
        }
        let (heading, side) = (heading.copied(), side.copied());
        let reach = range.copied().unwrap_or_default().0;
        let status = self.statuses.get(ent).ok().copied();
        let prior = self.recoveries.get(ent).ok().copied();

        // An auto-attack comes due on its own clock, whatever the recovery,
        // and a held actor swings at nothing
        if ability == AbilityType::AutoAttack {
            let due = self.swings.get(ent).is_ok_and(|swing| swing.waited(self.time.elapsed()).is_some());
            if !due || Status::holds(status.as_ref()) {
                return Err(None);
            }
        }

        // A strike needs a living hostile within its reach and its arc;
        // reach is measured as a swing measures it, the first level of
        // height between free
        let mut cast = Cast { ent, at, loc, attrs, side, reach, target: asked, target_loc: None };
        if ability.reach(reach).is_some() {
            cast.target_loc = self.foe(&cast).ok().map(|(_, target_loc)| target_loc);
        }
        let foe = cast.target_loc.map(|target_loc| (loc.distance(&target_loc), in_arc(&tuning, heading.as_ref(), Some(&attrs), &loc, &target_loc)));

        let across = cast.target_loc.is_some_and(|target_loc| targeting::across(heading.as_ref(), &loc, &target_loc));
        // A Leap with its target in reach leaps clear, a reaction
        let reacting = ability.reacts(ability == AbilityType::Leap
            && self.foe(&cast).is_ok_and(|(_, target_loc)| loc.distance(&target_loc) <= reach));
        admits(ability, reacting, prior.as_ref(), &attrs, reach, foe).map_err(Some)?;

        // The recovery runs by how spent the actor is as it uses the ability
        let fatigue = self.endurance.get(ent).map_or(0.0, |endurance| endurance.fatigue(&tuning));

        // The ability's own effect, which may still refuse before it
        // changes anything; it names whom its recovery is contested by
        let opponent = match ability {
            AbilityType::AutoAttack => auto_attack::swing(self, &cast),
            AbilityType::Frenzy => frenzy::strike(self, &cast),
            AbilityType::Feint => feint::strike(self, &cast),
            AbilityType::Overpower => overpower::strike(self, &cast),
            AbilityType::Punish => punish::strike(self, &cast),
            AbilityType::Parry => parry::answer(self, &cast),
            AbilityType::Counter => counter::answer(self, &cast),
            AbilityType::Leap => leap::leap(self, &cast),
            AbilityType::PerfectStride => stride::take(self, &cast),
        }.map_err(Some)?;

        // Endurance is spent and refuses nothing; an auto-attack is free
        if ability != AbilityType::AutoAttack {
            self.tire(ent, attrs.skill_endurance(&tuning, ability));
        }
        // Every client near draws it
        self.writer.write(Do { event: GameEvent::UseAbility { ent, ability, target: opponent, at: pressed, arrived } });
        // A strike across the caster's line breaks its stride, but in a
        // Perfect Stride
        if across && !self.strides(ent) {
            self.commands.trigger(Try { event: GameEvent::Stumble { ent } });
        }
        if ability != AbilityType::AutoAttack {
            self.commands.entity(ent).try_insert(LastSkill(self.time.elapsed()));
            let against = opponent.and_then(|opponent| self.actors.get(opponent).ok()).map(|(_, attrs, ..)| *attrs);
            landing::recover(ent, recovery_after(&tuning, ability, reacting, prior.as_ref(), &attrs, against.as_ref(), fatigue), &mut self.commands, &mut self.writer);
        }
        Ok(())
    }

    /// The living hostile `cast` was asked about, and where it stands
    pub fn foe(&self, cast: &Cast) -> Result<(Entity, Loc), AbilityFailReason> {
        let target = cast.target.ok_or(AbilityFailReason::NoTargets)?;
        let Ok((&loc, _, _, _, side, _, dead)) = self.actors.get(target) else {
            return Err(AbilityFailReason::NoTargets);
        };
        let hostile = cast.side.zip(side.copied()).is_some_and(|(own, theirs)| own.is_hostile_to(theirs));
        if dead || !hostile {
            return Err(AbilityFailReason::NoTargets);
        }
        Ok((target, loc))
    }

    /// The leash `ent`'s moves keep inside: an NPC's, round its
    /// engagement's place. A player has none.
    pub fn leash(&self, ent: Entity) -> Option<crate::leap::Leash> {
        let (chase, member) = self.leashed.get(ent).ok()?;
        let den = self.dens.get(member.0).ok()?;
        Some(crate::leap::Leash { den: **den, reach: chase.leash_distance })
    }

    /// Spends `spent` of `ent`'s endurance, as far as it has any, and tells
    /// its clients
    pub fn tire(&mut self, ent: Entity, spent: f32) {
        if spent <= 0.0 {
            return;
        }
        let Ok(mut endurance) = self.endurance.get_mut(ent) else { return };
        endurance.state = (endurance.state - spent).max(0.0);
        self.writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Endurance(*endurance) } });
    }

    /// Whether `ent` is in a Perfect Stride now
    pub fn strides(&self, ent: Entity) -> bool {
        self.statuses.get(ent).is_ok_and(|status| status.is_striding())
    }

    /// Queues a skill's strike on `target` for `damage` in all, in `parts`:
    /// each a share of it, struck its delay after now ([`WHOLE`] for one
    /// strike made at once). A full Intimidation bank is released into it
    /// (`Intimidation::release`): it lands `Tuning::intimidation_share`
    /// harder, and its last part binds its target as it lands, slowing it
    /// or rooting one already slowed (`resolve_threat`). Every skill that
    /// strikes deals its damage through here,
    /// and an auto-attack or a reaction's return never does.
    pub fn strike(&mut self, cast: &Cast, target: Entity, damage: f32, ability: AbilityType, parts: &[(f32, Duration)]) {
        let tuning = *self.tuning;
        let released = self.intimidations.get_mut(cast.ent).is_ok_and(|mut intimidation| intimidation.release(&tuning));
        let (damage, bind) = released_into(&tuning, damage, released);
        for (i, &(share, delay)) in parts.iter().enumerate() {
            let bind = if i + 1 == parts.len() { bind } else { 0.0 };
            self.deal(cast.ent, target, damage * share, ability, bind, delay);
        }
    }

    /// Queues a blow of `base_damage` from `source` on `target` as
    /// `ability`'s, struck `delay` after now, dazing away `bind` of its
    /// pace as it lands.
    pub fn deal(&mut self, source: Entity, target: Entity, base_damage: f32, ability: AbilityType, bind: f32, delay: Duration) {
        self.commands.trigger(Try {
            event: GameEvent::DealDamage { source, target, base_damage, ability: Some(ability), dot: 0.0, bind, delay },
        });
    }

    /// Takes the threats in `cast`'s band out of its user's queue, those
    /// landing within its span after it was pressed: what a reaction
    /// clears, and a Leap clear of its target. A reaction timed to nothing
    /// clears nothing, and is paid for all the same.
    pub fn answer(&mut self, cast: &Cast) -> Vec<QueuedThreat> {
        let span = cast.attrs.span(&self.tuning);
        self.clear(cast.ent, ClearType::Span { at: cast.at, span })
    }

    /// Takes the threats `clear_type` names out of `ent`'s queue, tells its
    /// clients where any stood there, and returns them
    pub fn clear(&mut self, ent: Entity, clear_type: ClearType) -> Vec<QueuedThreat> {
        let Ok(mut queue) = self.queues.get_mut(ent) else { return Vec::new() };
        let cleared = clear_threats(&mut queue, clear_type);
        if !cleared.is_empty() {
            self.writer.write(Do { event: GameEvent::ClearQueue { ent, clear_type } });
        }
        cleared
    }

    /// The game's clock, the one a threat's times are on
    pub fn game_now(&self) -> Duration {
        let now_ms = self.time.elapsed().as_millis() + self.runtime.elapsed_offset;
        Duration::from_millis(now_ms.min(u64::MAX as u128) as u64)
    }
}

/// Whether a striker facing `heading` with `attrs` may strike from `from`
/// at `to`: within the arc its Grace opens (`ActorAttributes::arc`).
pub fn in_arc(tuning: &Tuning, heading: Option<&Heading>, attrs: Option<&ActorAttributes>, from: &Loc, to: &Loc) -> bool {
    targeting::faces(heading, targeting::arc_of(tuning, attrs), from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{
        components::{entity_type::EntityType, resources::CombatState},
        plugins::nntree::{NNTreePlugin, NearestNeighbor},
    };
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
        let mut tiles = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        for q in -12..=12 {
            for r in -12..=12 {
                tiles.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        app.insert_resource(Map::new(tiles));
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
            ReactionQueue::default(),
            Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }),
            CombatState::default(),
        )).id();
        app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, loc));
        ent
    }

    /// The game's clock in `app`, the one a threat's times are on
    fn game_now(app: &App) -> Duration {
        let elapsed = app.world().resource::<Time>().elapsed().as_millis();
        Duration::from_millis((elapsed + app.world().resource::<crate::RunTime>().elapsed_offset) as u64)
    }

    fn ask(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Vec<GameEvent> {
        let at = game_now(app);
        app.world_mut().resource_mut::<Said>().0.clear();
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent, ability, target, at, arrived: Duration::ZERO } });
        app.update();
        std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
    }

    /// [`ask`], pressed with the game's clock set to `at`
    fn press(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>, at: Duration) -> Vec<GameEvent> {
        let elapsed = app.world().resource::<Time>().elapsed().as_millis();
        app.world_mut().resource_mut::<crate::RunTime>().elapsed_offset = at.as_millis().saturating_sub(elapsed);
        ask(app, ent, ability, target)
    }

    /// When the soonest threat in `ent`'s queue lands
    fn soonest(app: &App, ent: Entity) -> Duration {
        queue(app, ent)[0].lands_at()
    }

    /// How long before a threat lands the tests press for it
    const EARLY: Duration = Duration::from_millis(10);

    /// Why the gate refuses `ent` the use of `ability`, asked of the gate
    /// itself; None where it has nothing to say, or lets it through.
    fn refused(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Option<AbilityFailReason> {
        use bevy::ecs::system::RunSystemOnce;
        app.world_mut()
            .run_system_once(move |mut abilities: Abilities| {
                let now = abilities.game_now();
                abilities.cast(ent, ability, target, now, now)
            })
            .unwrap()
            .err()
            .flatten()
    }

    fn used(said: &[GameEvent], wanted: AbilityType) -> bool {
        said.iter().any(|event| matches!(event, GameEvent::UseAbility { ability, .. } if *ability == wanted))
    }

    fn queue(app: &App, ent: Entity) -> Vec<QueuedThreat> {
        app.world().get::<ReactionQueue>(ent).unwrap().threats.iter().copied().collect()
    }

    fn turned_to(app: &mut App, ent: Entity, q: i32) {
        app.world_mut().entity_mut(ent).insert(Heading::from_hex(Qrz { q, r: 0, z: 0 }));
    }

    #[test]
    fn a_strike_needs_a_living_hostile_within_reach_and_arc() {
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
        assert!(used(&said, AbilityType::Frenzy));
        let world = app.world();
        assert!(world.get::<Endurance>(caster).unwrap().state < 100.0, "it is paid for in endurance");
        assert!(world.get::<GlobalRecovery>(caster).is_some(), "and leaves its user recovering");
        let told = |event: &GameEvent| matches!(event, GameEvent::Incremental { ent, component: MessageComponent::Recovery(_) } if *ent == caster);
        assert!(said.iter().any(told), "a recovery its clients are sent whole");
        assert_eq!(queue(&app, near).len(), 1, "its blow waits in the target's queue");
        assert!(queue(&app, far).is_empty(), "a refused one queued nothing");
    }

    #[test]
    fn a_recovery_refuses_the_next_ability_and_a_swing_keeps_its_own_clock() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        assert!(used(&ask(&mut app, caster, AbilityType::Feint, Some(near)), AbilityType::Feint));
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::OnCooldown), "recovering, it uses no other ability");

        let said = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert!(used(&said, AbilityType::AutoAttack), "an auto-attack is outside the recovery");
        let spent = app.world().get::<Endurance>(caster).unwrap().state;
        let again = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert!(!used(&again, AbilityType::AutoAttack), "the next is not due yet");
        assert_eq!(refused(&mut app, caster, AbilityType::AutoAttack, Some(near)), None, "and a swing not due is refused without a reason");
        assert_eq!(app.world().get::<Endurance>(caster).unwrap().state, spent, "a swing costs no endurance");
    }

    #[test]
    fn a_bite_offers_another_and_ferocity_fires_it_before_it_unlocks() {
        let mut app = arena();
        let plain = actor(&mut app, Side::PLAYERS, 0);
        let fierce = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(fierce).insert(ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0));
        app.update();

        assert!(used(&ask(&mut app, plain, AbilityType::Frenzy, Some(near)), AbilityType::Frenzy));
        assert_eq!(refused(&mut app, plain, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::OnCooldown), "with no Ferocity the next bite waits to unlock");

        assert!(used(&ask(&mut app, fierce, AbilityType::Frenzy, Some(near)), AbilityType::Frenzy));
        assert!(used(&ask(&mut app, fierce, AbilityType::Frenzy, Some(near)), AbilityType::Frenzy), "Ferocity bites again at once");
        assert_eq!(queue(&app, near).iter().filter(|threat| threat.source == fierce).count(), 2);
    }

    #[test]
    fn a_feint_queues_one_light_strike() {
        let tuning = Tuning::DEFAULT;
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        assert!(used(&ask(&mut app, caster, AbilityType::Feint, Some(near)), AbilityType::Feint));
        let [feint] = queue(&app, near)[..] else { panic!("one strike") };
        let attrs = ActorAttributes::default();
        assert!(feint.damage < attrs.base_potency(&tuning) * tuning.frenzy_damage, "lighter than a bite");
    }

    #[test]
    fn a_counter_answers_what_lands_in_its_band() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, attacker, -1);
        app.update();

        assert!(used(&ask(&mut app, attacker, AbilityType::Frenzy, Some(defender)), AbilityType::Frenzy));
        assert_eq!(queue(&app, defender).len(), 1);
        let before = app.world().get::<Health>(attacker).unwrap().state;
        let lands = soonest(&app, defender);
        let said = press(&mut app, defender, AbilityType::Counter, None, lands - EARLY);
        assert!(used(&said, AbilityType::Counter));
        assert!(queue(&app, defender).is_empty(), "the blow is cleared");
        assert!(said.iter().any(|event| matches!(event, GameEvent::UseAbility { target, .. } if *target == Some(attacker))), "and answered to its source");
        assert!(app.world().get::<Health>(attacker).unwrap().state < before, "which takes a share of it back");
    }

    #[test]
    fn a_parry_clears_its_band_and_sends_nothing_back() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, attacker, -1);
        app.update();

        assert!(used(&ask(&mut app, attacker, AbilityType::Feint, Some(defender)), AbilityType::Feint));
        let before = app.world().get::<Health>(attacker).unwrap().state;
        let lands = soonest(&app, defender);
        assert!(used(&press(&mut app, defender, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry));
        assert!(queue(&app, defender).is_empty(), "the feint is parried");
        assert_eq!(app.world().get::<Health>(attacker).unwrap().state, before, "and nothing goes back");
    }

    #[test]
    fn a_reaction_pays_its_flat_cost_whatever_it_clears_or_misses_and_without_it_is_spent_not_refused() {
        let tuning = Tuning::DEFAULT;
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.update();

        let plain = ActorAttributes::default();
        let endurance = |app: &App| app.world().get::<Endurance>(defender).unwrap().state;
        let queued = |app: &mut App, damage: f32, millis: u64| {
            let at = Duration::from_millis(millis);
            let threat = create_threat(&tuning, attacker, &plain, &plain, damage, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
            threat.lands_at()
        };
        let flat = plain.skill_endurance(&tuning, AbilityType::Parry);
        let before = endurance(&app);
        let lands = queued(&mut app, 10.0, 0);
        assert!(used(&press(&mut app, defender, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry));
        let one = before - endurance(&app);
        assert!((one - flat).abs() < 1e-3, "its flat cost as any skill: {one}");

        // Three heavy blows in one band cost what one light one did
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        let before = endurance(&app);
        let lands = [1000, 1050, 1100].map(|millis| queued(&mut app, 100.0, millis))[0];
        assert!(used(&press(&mut app, defender, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry));
        assert!(queue(&app, defender).is_empty(), "it clears its whole band");
        assert!((before - endurance(&app) - flat).abs() < 1e-3, "and what it clears costs nothing more");

        // Short of its price, it is used all the same and its user spent
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        app.world_mut().get_mut::<Endurance>(defender).unwrap().state = flat / 2.0;
        let lands = queued(&mut app, 10.0, 2000);
        assert!(used(&press(&mut app, defender, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry), "endurance refuses no reaction");
        assert_eq!(endurance(&app), 0.0, "and leaves its user spent");

        // Timed to nothing, it misses and is paid for all the same
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        app.world_mut().get_mut::<Endurance>(defender).unwrap().state = 100.0;
        let lands = queued(&mut app, 10.0, 3000);
        assert!(used(&press(&mut app, defender, AbilityType::Parry, None, lands - Duration::from_secs(1)), AbilityType::Parry), "a reaction pressed early is used");
        assert_eq!(queue(&app, defender).len(), 1, "and takes nothing");
        assert!((100.0 - endurance(&app) - flat).abs() < 1e-3, "at its flat cost");
        assert!(app.world().get::<GlobalRecovery>(defender).is_some(), "and its recovery");
    }

    #[test]
    fn a_press_is_judged_at_its_moment_and_never_before_it_arrived() {
        let tuning = Tuning::DEFAULT;
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.update();

        let plain = ActorAttributes::default();
        let queued = |app: &mut App, at: Duration| {
            let threat = create_threat(&tuning, attacker, &plain, &plain, 10.0, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
            threat.lands_at()
        };
        let clock_to = |app: &mut App, at: Duration| {
            let elapsed = app.world().resource::<Time>().elapsed().as_millis();
            app.world_mut().resource_mut::<crate::RunTime>().elapsed_offset = at.as_millis().saturating_sub(elapsed);
        };
        let stamped = |app: &mut App, at: Duration| {
            app.world_mut().resource_mut::<Said>().0.clear();
            app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent: defender, ability: AbilityType::Parry, target: None, at, arrived: Duration::ZERO } });
            app.update();
            std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
        };
        let answered = |said: &[GameEvent]| said.iter().any(|event| matches!(event, GameEvent::ClearQueue { clear_type: ClearType::Span { .. }, .. }));

        // Stamped ahead of the server's clock, as a press on time arrives
        let lands = queued(&mut app, Duration::ZERO);
        clock_to(&mut app, lands - Duration::from_millis(400));
        assert!(!used(&stamped(&mut app, lands - EARLY), AbilityType::Parry), "it is held");
        clock_to(&mut app, lands - EARLY);
        app.world_mut().resource_mut::<Said>().0.clear();
        app.update();
        assert!(answered(&app.world().resource::<Said>().0), "and judged at its moment");
        assert!(queue(&app, defender).is_empty());

        // Stamped long before it arrived, its band starts as it arrives
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        let lands = queued(&mut app, Duration::from_secs(5));
        clock_to(&mut app, lands + EARLY);
        let said = stamped(&mut app, lands - EARLY);
        assert!(used(&said, AbilityType::Parry), "a late press is used");
        assert!(!answered(&said), "and takes nothing that landed before it came");
    }

    #[test]
    fn a_reaction_takes_what_lands_in_its_band_and_leaves_the_rest() {
        let tuning = Tuning::DEFAULT;
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.update();

        let plain = ActorAttributes::default();
        let span = plain.span(&tuning);
        for at in [Duration::ZERO, span / 2, span * 4] {
            let threat = create_threat(&tuning, attacker, &plain, &plain, 10.0, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
        }

        let lands = soonest(&app, defender);
        assert!(used(&press(&mut app, defender, AbilityType::Counter, None, lands), AbilityType::Counter));
        let left: Vec<Duration> = queue(&app, defender).iter().map(|threat| threat.inserted_at).collect();
        assert_eq!(left, vec![span * 4], "the two landing within its band are taken; the later one stands");
    }

    #[test]
    fn a_leap_carries_clear_of_a_target_in_reach_and_onto_one_out_of_it() {
        let mut app = arena();
        let leaper = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, near, -1);
        // The leaper swings at what it faces, as a player does
        app.world_mut().entity_mut(leaper).insert(Target::default());
        app.update();
        let distance = |app: &App| app.world().get::<Loc>(leaper).unwrap().flat_distance(app.world().get::<Loc>(near).unwrap());
        let reach = AttackRange::default().0;

        assert_eq!(refused(&mut app, leaper, AbilityType::Leap, None), Some(AbilityFailReason::NoTargets), "a leap needs someone to leap from or onto");

        // It swings at what it faces as soon as it targets it
        app.update();
        assert_eq!(queue(&app, near).len(), 1, "its first swing waits in its target's queue");

        // In reach, with a blow queued on it: clear of both
        assert!(used(&ask(&mut app, near, AbilityType::Frenzy, Some(leaper)), AbilityType::Frenzy));
        let lands = soonest(&app, leaper);
        assert!(used(&press(&mut app, leaper, AbilityType::Leap, Some(near), lands - EARLY), AbilityType::Leap));
        app.update();
        assert!(distance(&app) > reach, "it leaps out of reach");
        assert!(queue(&app, leaper).iter().all(|threat| threat.ability != Some(AbilityType::Frenzy)), "and the blow misses");
        assert!(queue(&app, near).iter().all(|threat| threat.ability != Some(AbilityType::Leap)), "a leap clear strikes nothing");

        // Out of reach, once its recovery has run: onto it
        app.world_mut().entity_mut(leaper).remove::<GlobalRecovery>();
        assert!(used(&ask(&mut app, leaper, AbilityType::Leap, Some(near)), AbilityType::Leap));
        app.update();
        app.update();
        assert_eq!(distance(&app), 1, "it leaps to beside its target");
        assert_eq!(queue(&app, near).iter().filter(|threat| threat.ability == Some(AbilityType::Leap)).count(), 1, "striking it as it lands");
    }

    #[test]
    fn a_perfect_stride_strikes_across_its_line_without_breaking_stride() {
        let tuning = Tuning::DEFAULT;
        let graceful = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let broken = |app: &App, ent: Entity| app.world().get::<Status>(ent).is_some_and(|status| status.stride.is_some());
        let mut app = arena();
        let plain = actor(&mut app, Side::PLAYERS, 0);
        let striding = actor(&mut app, Side::PLAYERS, 0);
        let beside = actor(&mut app, Side::WILD, 0);
        for ent in [plain, striding] {
            app.world_mut().entity_mut(ent).insert(graceful);
        }
        // Their target stands in reach, across their line and inside the arc Grace opens
        let from = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let across = (-2..=2).flat_map(|q| (-2..=2).map(move |r| Loc::new(Qrz { q, r, z: 1 })))
            .find(|to| *to != from && from.distance(to) <= 2 && targeting::across(Some(&heading), &from, to) && in_arc(&tuning, Some(&heading), Some(&graceful), &from, to))
            .expect("a tile across the line that Grace reaches");
        app.world_mut().entity_mut(beside).insert(across);
        app.update();

        assert!(used(&ask(&mut app, striding, AbilityType::PerfectStride, None), AbilityType::PerfectStride));
        app.world_mut().entity_mut(striding).remove::<GlobalRecovery>();
        for ent in [plain, striding] {
            assert!(used(&ask(&mut app, ent, AbilityType::Frenzy, Some(beside)), AbilityType::Frenzy), "Grace strikes past the forward faces");
        }
        app.update();
        assert!(broken(&app, plain), "which breaks a stride");
        assert!(!broken(&app, striding), "but a Perfect Stride");
    }

    #[test]
    fn a_full_bank_is_released_into_the_next_skill_slowing_its_target_or_rooting_one_slowed() {
        let tuning = Tuning::DEFAULT;
        let mut app = arena();
        let imposing = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(imposing).insert(ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0));
        turned_to(&mut app, attacker, -1);
        app.update();
        let filled = |app: &App| app.world().get::<Intimidation>(imposing).unwrap().filled;
        let feints = |app: &App| queue(app, attacker).into_iter().filter(|threat| threat.ability == Some(AbilityType::Feint)).collect::<Vec<_>>();
        let release = |app: &mut App| {
            app.world_mut().entity_mut(imposing).remove::<GlobalRecovery>();
            app.world_mut().get_mut::<Intimidation>(imposing).unwrap().filled = Intimidation::size(&tuning);
            assert!(used(&ask(app, imposing, AbilityType::Feint, Some(attacker)), AbilityType::Feint));
        };
        let land = |app: &mut App| {
            let threat = app.world_mut().get_mut::<ReactionQueue>(attacker).unwrap().threats.pop_front().unwrap();
            app.world_mut().trigger(Try { event: GameEvent::ResolveThreat { ent: attacker, threat } });
            app.update();
        };
        let status = |app: &App| app.world().get::<Status>(attacker).copied().unwrap_or_default();

        assert!(used(&ask(&mut app, imposing, AbilityType::Feint, Some(attacker)), AbilityType::Feint));
        assert_eq!(feints(&app)[0].bind, 0.0, "short of full, a skill releases nothing");

        release(&mut app);
        assert!(filled(&app) < Intimidation::size(&tuning), "full, the next skill releases it");
        assert!(feints(&app)[1].bind > 0.0, "binding");
        assert_eq!(released_into(&tuning, 100.0, true), (100.0 * (1.0 + tuning.intimidation_share), tuning.intimidation_slow), "and landing harder");
        assert_eq!(released_into(&tuning, 100.0, false), (100.0, 0.0));
        land(&mut app);
        assert!(status(&app).slow.is_none(), "queued, it binds nothing yet");
        land(&mut app);
        assert!(status(&app).slow.is_some() && status(&app).root.is_none(), "landed on a target fighting it, it slows");

        release(&mut app);
        land(&mut app);
        assert!(status(&app).root.is_some(), "landed on a target already slowed, it roots");
    }

    #[test]
    fn the_dead_use_nothing() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(caster).insert(RespawnTimer::new(Duration::ZERO));
        app.update();

        let said = ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        assert!(!used(&said, AbilityType::Frenzy));
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), None, "refused without a reason");
    }
}
