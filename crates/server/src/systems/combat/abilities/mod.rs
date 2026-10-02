//! Every ability an actor uses goes through one gate, [`Abilities::cast`].
//! The gate asks, in order and the same of every ability: is the caster
//! alive; is it out of recovery, or taking the combo it was offered, or
//! reacting through the recovery (an auto-attack asks its clock instead);
//! does it strike a living hostile within the ability's reach and its arc;
//! can it pay. Then the ability's own effect runs, the
//! stamina and endurance are paid, the clients are told, a strike across
//! the caster's line breaks its stride, and the recovery starts, longer
//! for an actor whose endurance is spent. A skill costs endurance, and so
//! does a swing struck across the caster's line; a swing within its
//! forward faces is free.
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
        grit::Grit,
        heading::Heading,
        reaction_queue::{QueuedThreat, ReactionQueue},
        recovery::GlobalRecovery,
        resources::{Endurance, Health, RespawnTimer, Stamina},
        status::Status,
        target::Target,
        ActorAttributes, AttackRange, Loc, Swing,
    },
    components::entity_type::EntityType,
    message::{AbilityType, ClearType, Component as MessageComponent, Do, Event as GameEvent, Try},
    resources::map::Map,
    systems::{
        combat::{queue::clear_threats, combos::{may_use, reacts_through, recovery_after}},
        targeting,
    },
};

use crate::systems::{
    behaviour::{chase::Chase, perception::{Sight, Skill}},
    combat::landing,
};

/// Why the gate refused an ability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFailReason {
    InsufficientStamina,
    NoTargets,
    OnCooldown,
    OutOfRange,
    /// The target stands outside the caster's arc
    NotFacing,
}

/// One use of an ability the gate has let through: who uses it, from where,
/// and at whom.
pub struct Cast {
    pub ent: Entity,
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
/// `Tuning::grit_share` harder and `grit_bind` binding with a full Grit
/// bank `released` into it, as it is without.
fn released_into(damage: f32, released: bool) -> (f32, f32) {
    let tuning = common_bevy::tuning::tuning();
    if released { (damage * (1.0 + tuning.grit_share), tuning.grit_bind) } else { (damage, 0.0) }
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
    pub stamina: Query<'w, 's, &'static mut Stamina>,
    pub endurance: Query<'w, 's, &'static mut Endurance>,
    pub recoveries: Query<'w, 's, &'static GlobalRecovery>,
    pub queues: Query<'w, 's, &'static mut ReactionQueue>,
    pub statuses: Query<'w, 's, &'static mut Status>,
    pub swings: Query<'w, 's, &'static mut Swing>,
    pub grits: Query<'w, 's, &'static mut Grit>,
    pub targets: Query<'w, 's, (Entity, &'static Target)>,
    pub npcs: Query<'w, 's, (Entity, &'static EntityType, &'static crate::systems::behaviour::Bar), With<Chase>>,
    pub minds: Query<'w, 's, (&'static Skill, &'static mut Sight)>,
    pub leashed: Query<'w, 's, (&'static Chase, &'static EngagementMember)>,
    pub dens: Query<'w, 's, &'static Loc, With<Engagement>>,
    pub engagements: Query<'w, 's, &'static Engagement>,
    pub last_skills: Query<'w, 's, &'static LastSkill>,
    pub kinds: Query<'w, 's, &'static EntityType>,
    pub map: Res<'w, Map>,
    pub time: Res<'w, Time>,
    pub runtime: Res<'w, crate::resources::RunTime>,
    pub dice: ResMut<'w, crate::resources::Dice>,
    pub decisions: Option<ResMut<'w, crate::systems::behaviour::Decisions>>,
}

/// Uses every ability due or asked for this frame: the auto-attacks that
/// have come due, then what players asked, in the order asked, then what
/// each NPC chooses.
pub fn use_abilities(mut reader: MessageReader<Try>, mut abilities: Abilities) {
    // Swings go first: a Leap moves its user as the frame ends, so a swing
    // after it would still strike from where it stood
    abilities.swing();
    for message in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target } } = message else { continue };
        abilities.ask(*ent, *ability, *target);
    }
    abilities.skills();
}

impl Abilities<'_, '_> {
    /// Uses `ability` for `ent` if the gate lets it. Returns whether it was
    /// used; a refusal is told to no one.
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

    /// The gate. `asked` is the actor the ability is used on: a strike's
    /// target, a Leap's. Errs with the reason it was refused, or with none
    /// where there is nothing to tell: a dead caster, a swing not yet due.
    fn cast(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>) -> Result<(), Option<AbilityFailReason>> {
        let tuning = common_bevy::tuning::tuning();
        let Ok((&loc, &attrs, _, heading, side, range, dead)) = self.actors.get(ent) else { return Err(None) };
        if dead {
            return Err(None);
        }
        let (heading, side) = (heading.copied(), side.copied());
        let reach = range.copied().unwrap_or_default().0;
        let status = self.statuses.get(ent).ok().copied();
        let prior = self.recoveries.get(ent).ok().copied();

        // An auto-attack comes due on its own clock, whatever the recovery,
        // and a held actor swings at nothing. Every other ability waits on
        // the recovery.
        if ability == AbilityType::AutoAttack {
            let due = self.swings.get(ent).is_ok_and(|swing| swing.waited(self.time.elapsed()).is_some());
            if !due || Status::holds(status.as_ref()) {
                return Err(None);
            }
        } else if !may_use(ability, prior.as_ref()) && !reacts_through(ability, prior.as_ref(), Some(&attrs)) {
            return Err(Some(AbilityFailReason::OnCooldown));
        }

        // A strike needs a living hostile within its reach and its arc;
        // reach is measured as a swing measures it, the first level of
        // height between free
        let mut cast = Cast { ent, loc, attrs, side, reach, target: asked, target_loc: None };
        if let Some(within) = ability.reach(reach) {
            let (_, target_loc) = self.foe(&cast).map_err(Some)?;
            if !within.contains(&loc.distance(&target_loc)) {
                return Err(Some(AbilityFailReason::OutOfRange));
            }
            if !in_arc(heading.as_ref(), Some(&attrs), &loc, &target_loc) {
                return Err(Some(AbilityFailReason::NotFacing));
            }
            cast.target_loc = Some(target_loc);
        }

        // A swing struck across the caster's line is a skill's effort: it
        // costs stamina, the more the further round its arc, and waits
        // without it. A Perfect Stride waives it
        let across = cast.target_loc.is_some_and(|target_loc| targeting::across(heading.as_ref(), &loc, &target_loc));
        let share = if self.strides(ent) { 0.0 } else {
            heading.as_ref().zip(cast.target_loc).map_or(0.0, |(heading, target_loc)| targeting::across_share(heading, &loc, &target_loc))
        };
        let cost = tuning.cost(ability) + if ability == AbilityType::AutoAttack { tuning.off_arc_stamina * share } else { 0.0 };
        if self.stamina.get(ent).map_or(true, |stamina| stamina.state < cost) {
            return Err(Some(AbilityFailReason::InsufficientStamina));
        }

        // The recovery runs by how spent the actor is as it uses the ability
        let fatigue = self.endurance.get(ent).map_or(0.0, |endurance| endurance.fatigue());

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

        if cost > 0.0 {
            if let Ok(mut stamina) = self.stamina.get_mut(ent) {
                stamina.state -= cost;
                self.writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Stamina(*stamina) } });
            }
        }
        // Endurance is spent beside the stamina and refuses nothing: a
        // skill's, or for a swing struck across the caster's line a share
        // of the Force it strikes with, the more the further round its arc.
        // A reaction has paid besides for what it cleared (`react`)
        self.tire(ent, match ability {
            AbilityType::AutoAttack if across => tuning.off_arc_cost * share * attrs.force(),
            AbilityType::AutoAttack => 0.0,
            _ => attrs.skill_endurance(ability),
        });
        // Every client near draws it
        self.writer.write(Do { event: GameEvent::UseAbility { ent, ability, target: opponent } });
        // A strike across the caster's line breaks its stride, but in a
        // Perfect Stride
        if across && !self.strides(ent) {
            self.commands.trigger(Try { event: GameEvent::Stumble { ent } });
        }
        if ability != AbilityType::AutoAttack {
            self.commands.entity(ent).try_insert(LastSkill(self.time.elapsed()));
            let against = opponent.and_then(|opponent| self.actors.get(opponent).ok()).map(|(_, attrs, ..)| *attrs);
            landing::recover(ent, recovery_after(ability, prior.as_ref(), &attrs, against.as_ref(), fatigue), &mut self.commands, &mut self.writer);
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
    pub fn leash(&self, ent: Entity) -> Option<crate::systems::combat::leap::Leash> {
        let (chase, member) = self.leashed.get(ent).ok()?;
        let den = self.dens.get(member.0).ok()?;
        Some(crate::systems::combat::leap::Leash { den: **den, reach: chase.leash_distance })
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
    /// strike made at once). A full Grit bank is released into it
    /// (`Grit::release`): it lands `Tuning::grit_share` harder, and its last
    /// part binds its target, slowed out of `Tuning::grit_bind` of its speed
    /// as it lands. Every skill that strikes deals its damage through here,
    /// and an auto-attack or a reaction's return never does.
    pub fn strike(&mut self, cast: &Cast, target: Entity, damage: f32, ability: AbilityType, parts: &[(f32, Duration)]) {
        let released = self.grits.get_mut(cast.ent).is_ok_and(|mut grit| grit.release());
        let (damage, bind) = released_into(damage, released);
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

    /// The threats a reaction by `cast` clears, taken out of its user's
    /// queue: the front one and those landing within its span behind it.
    /// Each is paid for in endurance (`ActorAttributes::reaction_effort`),
    /// which refuses nothing: a user without the endurance clears them all
    /// the same and is spent, its fatigue the price. Errs with nothing
    /// queued.
    pub fn react(&mut self, cast: &Cast) -> Result<Vec<QueuedThreat>, AbilityFailReason> {
        let cleared = self.answer_span(cast);
        if cleared.is_empty() {
            return Err(AbilityFailReason::NoTargets);
        }
        Ok(cleared)
    }

    /// Takes the threats in `cast`'s user's span out of its queue, the
    /// front one and those landing within its span behind it, and pays
    /// endurance for each (`ActorAttributes::reaction_effort`): what a
    /// reaction clears, and a Leap clear of its target
    pub fn answer_span(&mut self, cast: &Cast) -> Vec<QueuedThreat> {
        let cleared = self.clear(cast.ent, ClearType::Span(cast.attrs.span()));
        let price: f32 = cleared.iter().map(|threat| cast.attrs.reaction_effort(threat.damage)).sum();
        self.tire(cast.ent, price);
        cleared
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
pub fn in_arc(heading: Option<&Heading>, attrs: Option<&ActorAttributes>, from: &Loc, to: &Loc) -> bool {
    targeting::faces(heading, targeting::arc_of(attrs), from, to)
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
        app.add_plugins((MinimalPlugins, NNTreePlugin, crate::plugins::combat::CombatPlugin));
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
            Stamina { state: 100.0, max: 100.0, regen_rate: 0.0, last_update: Duration::ZERO },
            Endurance::full(100.0),
            ReactionQueue::default(),
            Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }),
            CombatState::default(),
        )).id();
        app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, loc));
        ent
    }

    fn ask(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Vec<GameEvent> {
        app.world_mut().resource_mut::<Said>().0.clear();
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent, ability, target } });
        app.update();
        std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
    }

    /// Why the gate refuses `ent` the use of `ability`, asked of the gate
    /// itself; None where it has nothing to say, or lets it through.
    fn refused(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Option<AbilityFailReason> {
        use bevy::ecs::system::RunSystemOnce;
        app.world_mut()
            .run_system_once(move |mut abilities: Abilities| abilities.cast(ent, ability, target))
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
        assert!(world.get::<Stamina>(caster).unwrap().state < 100.0, "it is paid for");
        assert!(world.get::<Endurance>(caster).unwrap().state < 100.0, "in endurance too");
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
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        assert!(used(&ask(&mut app, caster, AbilityType::Feint, Some(near)), AbilityType::Feint));
        let [feint] = queue(&app, near)[..] else { panic!("one strike") };
        let attrs = ActorAttributes::default();
        assert!(feint.damage < attrs.base_potency() * common_bevy::tuning::tuning().frenzy_damage, "lighter than a bite");
    }

    #[test]
    fn a_counter_answers_the_queue_and_needs_something_in_it() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, attacker, -1);
        app.update();

        assert_eq!(refused(&mut app, defender, AbilityType::Counter, None), Some(AbilityFailReason::NoTargets), "nothing queued, nothing to counter");

        assert!(used(&ask(&mut app, attacker, AbilityType::Frenzy, Some(defender)), AbilityType::Frenzy));
        assert_eq!(queue(&app, defender).len(), 1);
        let before = app.world().get::<Health>(attacker).unwrap().state;
        let said = ask(&mut app, defender, AbilityType::Counter, None);
        assert!(used(&said, AbilityType::Counter));
        assert!(queue(&app, defender).is_empty(), "the blow is cleared");
        assert!(said.iter().any(|event| matches!(event, GameEvent::UseAbility { target, .. } if *target == Some(attacker))), "and answered to its source");
        assert!(app.world().get::<Health>(attacker).unwrap().state < before, "which takes a share of it back");
    }

    #[test]
    fn a_parry_clears_its_span_and_sends_nothing_back() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, attacker, -1);
        app.update();

        assert_eq!(refused(&mut app, defender, AbilityType::Parry, None), Some(AbilityFailReason::NoTargets), "nothing queued, nothing to parry");

        assert!(used(&ask(&mut app, attacker, AbilityType::Feint, Some(defender)), AbilityType::Feint));
        let before = app.world().get::<Health>(attacker).unwrap().state;
        assert!(used(&ask(&mut app, defender, AbilityType::Parry, None), AbilityType::Parry));
        assert!(queue(&app, defender).is_empty(), "the feint is parried");
        assert_eq!(app.world().get::<Health>(attacker).unwrap().state, before, "and nothing goes back");
    }

    #[test]
    fn a_reaction_pays_endurance_for_what_it_clears_and_without_it_is_spent_not_refused() {
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.update();

        let plain = ActorAttributes::default();
        let endurance = |app: &App| app.world().get::<Endurance>(defender).unwrap().state;
        let pool = endurance(&app);
        let queued = |app: &mut App, damage: f32, millis: u64| {
            let at = Duration::from_millis(millis);
            let threat = create_threat(attacker, &plain, &plain, damage, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
        };
        let flat = plain.skill_endurance(AbilityType::Parry);
        queued(&mut app, 10.0, 0);
        assert!(used(&ask(&mut app, defender, AbilityType::Parry, None), AbilityType::Parry));
        let paid = pool - endurance(&app);
        assert!((paid - flat - plain.reaction_effort(10.0)).abs() < 1e-3, "its flat cost as any skill, and a price for what it cleared: {paid}");

        // More than the pool holds, in one span: all of it is cleared, and its user spent
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        for millis in [0, 50, 100] {
            queued(&mut app, pool * 10.0, millis);
        }
        assert!(used(&ask(&mut app, defender, AbilityType::Parry, None), AbilityType::Parry), "endurance refuses no reaction");
        assert!(queue(&app, defender).is_empty(), "it clears its whole span");
        assert_eq!(endurance(&app), 0.0, "and leaves its user spent");

        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        let full = app.world().get::<Stamina>(defender).unwrap().max;
        app.world_mut().get_mut::<Stamina>(defender).unwrap().state = full;
        assert_eq!(refused(&mut app, defender, AbilityType::Parry, None), Some(AbilityFailReason::NoTargets), "with nothing queued there is nothing to answer");
    }

    #[test]
    fn a_reaction_takes_the_front_threat_and_its_span_and_leaves_what_lands_later() {
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.update();

        let plain = ActorAttributes::default();
        let span = plain.span();
        for at in [Duration::ZERO, span / 2, span * 4] {
            let threat = create_threat(attacker, &plain, &plain, 10.0, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
        }

        assert!(used(&ask(&mut app, defender, AbilityType::Counter, None), AbilityType::Counter));
        let left: Vec<Duration> = queue(&app, defender).iter().map(|threat| threat.inserted_at).collect();
        assert_eq!(left, vec![span * 4], "the front and the one landing within its span are taken; the later one stands");
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
        assert!(used(&ask(&mut app, leaper, AbilityType::Leap, Some(near)), AbilityType::Leap));
        app.update();
        assert!(distance(&app) > reach, "it leaps out of reach");
        assert!(queue(&app, leaper).iter().all(|threat| threat.ability != Some(AbilityType::Frenzy)), "and the blow misses");
        assert!(queue(&app, near).iter().all(|threat| threat.ability != Some(AbilityType::Leap)), "a leap clear strikes nothing");

        // Out of reach, once its recovery has run and its stamina is back: onto it
        app.world_mut().entity_mut(leaper).remove::<GlobalRecovery>();
        let full = app.world().get::<Stamina>(leaper).unwrap().max;
        app.world_mut().get_mut::<Stamina>(leaper).unwrap().state = full;
        assert!(used(&ask(&mut app, leaper, AbilityType::Leap, Some(near)), AbilityType::Leap));
        app.update();
        app.update();
        assert_eq!(distance(&app), 1, "it leaps to beside its target");
        assert_eq!(queue(&app, near).iter().filter(|threat| threat.ability == Some(AbilityType::Leap)).count(), 1, "striking it as it lands");
    }

    #[test]
    fn a_perfect_stride_strikes_across_its_line_without_breaking_stride() {
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
            .find(|to| *to != from && from.distance(to) <= 2 && targeting::across(Some(&heading), &from, to) && in_arc(Some(&heading), Some(&graceful), &from, to))
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
    fn a_swing_across_its_line_costs_stamina_and_endurance_but_ahead_or_in_a_perfect_stride() {
        let graceful = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut app = arena();
        let [ahead, across, striding, tired] = [0, 0, 0, 0].map(|q| actor(&mut app, Side::PLAYERS, q));
        let (front, side) = (actor(&mut app, Side::WILD, 1), actor(&mut app, Side::WILD, 0));
        let pool = graceful.max_endurance();
        for ent in [ahead, across, striding, tired] {
            app.world_mut().entity_mut(ent).insert((graceful, Endurance::full(pool)));
        }
        let from = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let heading = Heading::from_hex(Qrz { q: 1, r: 0, z: 0 });
        let off = (-2..=2).flat_map(|q| (-2..=2).map(move |r| Loc::new(Qrz { q, r, z: 1 })))
            .find(|to| *to != from && from.distance(to) <= 2 && targeting::across(Some(&heading), &from, to) && in_arc(Some(&heading), Some(&graceful), &from, to))
            .expect("a tile across the line that Grace reaches");
        app.world_mut().entity_mut(side).insert(off);
        app.update();
        let spent = |app: &App, ent: Entity| pool - app.world().get::<Endurance>(ent).unwrap().state;

        assert!(used(&ask(&mut app, ahead, AbilityType::AutoAttack, Some(front)), AbilityType::AutoAttack));
        assert_eq!(spent(&app, ahead), 0.0, "a swing within the forward faces is free");
        let stamina = |app: &App, ent: Entity| app.world().get::<Stamina>(ent).unwrap().state;
        let before = stamina(&app, across);
        assert!(used(&ask(&mut app, across, AbilityType::AutoAttack, Some(side)), AbilityType::AutoAttack));
        assert!(spent(&app, across) > 0.0, "one struck across its line costs endurance");
        assert!(stamina(&app, across) < before, "and stamina, as a skill would");

        app.world_mut().get_mut::<Stamina>(tired).unwrap().state = 0.0;
        assert!(!used(&ask(&mut app, tired, AbilityType::AutoAttack, Some(side)), AbilityType::AutoAttack), "without the stamina it waits");
        assert!(used(&ask(&mut app, tired, AbilityType::AutoAttack, Some(front)), AbilityType::AutoAttack), "while one ahead is free");

        assert!(used(&ask(&mut app, striding, AbilityType::PerfectStride, None), AbilityType::PerfectStride));
        let (stride, before) = (spent(&app, striding), stamina(&app, striding));
        assert!(used(&ask(&mut app, striding, AbilityType::AutoAttack, Some(side)), AbilityType::AutoAttack));
        assert_eq!((spent(&app, striding), stamina(&app, striding)), (stride, before), "a Perfect Stride waives it");
    }

    #[test]
    fn grit_fills_by_its_tier_and_only_a_full_bank_is_released() {
        let mut app = arena();
        let gritty = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        let attrs = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        app.world_mut().entity_mut(gritty).insert(attrs);
        turned_to(&mut app, attacker, -1);
        app.update();
        let filled = |app: &App| app.world().get::<Grit>(gritty).unwrap().filled;
        let feints = |app: &App| queue(app, attacker).into_iter().filter(|threat| threat.ability == Some(AbilityType::Feint)).collect::<Vec<_>>();

        assert!(used(&ask(&mut app, attacker, AbilityType::Frenzy, Some(gritty)), AbilityType::Frenzy));
        assert_eq!(filled(&app), 0, "a blow still in the queue fills nothing");
        app.world_mut().write_message(Try { event: GameEvent::Dismiss { ent: gritty } });
        app.update();
        assert_eq!(filled(&app), attrs.grit_fill(), "let land, it fills the bank by the tier");

        assert!(used(&ask(&mut app, gritty, AbilityType::Feint, Some(attacker)), AbilityType::Feint));
        assert_eq!(filled(&app), attrs.grit_fill(), "short of full, a skill leaves it be");
        let plain = feints(&app)[0];
        assert_eq!(plain.bind, 0.0);

        app.world_mut().entity_mut(gritty).remove::<GlobalRecovery>();
        app.world_mut().get_mut::<Grit>(gritty).unwrap().filled = Grit::size();
        assert!(used(&ask(&mut app, gritty, AbilityType::Feint, Some(attacker)), AbilityType::Feint));
        assert_eq!(filled(&app), 0, "full, the next skill releases it");
        assert!(feints(&app)[1].bind > 0.0, "binding");
        let tuning = common_bevy::tuning::tuning();
        assert_eq!(released_into(100.0, true), (100.0 * (1.0 + tuning.grit_share), tuning.grit_bind), "and landing harder");
        assert_eq!(released_into(100.0, false), (100.0, 0.0));
        app.world_mut().write_message(Try { event: GameEvent::Dismiss { ent: attacker } });
        app.update();
        let slowed = |app: &App| app.world().get::<Status>(attacker).is_some_and(|status| status.slow.is_some());
        assert!(!slowed(&app), "queued, it binds nothing yet");
        app.world_mut().write_message(Try { event: GameEvent::Dismiss { ent: attacker } });
        app.update();
        assert!(slowed(&app), "landed, it slows its target");
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
