//! Every ability an actor uses goes through one gate, [`Abilities::cast`].
//! The gate asks, in order and the same of every ability: is the caster
//! alive; is it out of recovery, or taking the combo it was offered, or
//! reacting through the recovery (an auto-attack asks its cadence instead);
//! does it strike a living hostile within the ability's reach and its arc;
//! can it pay. Then the ability's own effect runs, the
//! stamina and endurance are paid, the clients are told, a strike across
//! the caster's line breaks its stride, and the recovery starts, longer
//! for an actor whose endurance is spent.
//!
//! What each ability costs, how long its recovery runs, how far it
//! reaches, what it offers next and whether it is a reaction are the
//! ability's own to say (`Tuning::cost`, `Tuning::recovery`,
//! `AbilityType::reach`, `combo`, `is_reaction`). Its module here holds
//! only what it does.
//!
//! One system, [`use_abilities`], runs all of it in a stated order: what
//! players asked for, then the auto-attacks come due, then each NPC's
//! signature. An ability is handled the frame it is asked for, whoever
//! asks, and a due swing the frame its target comes within its reach.

pub mod auto_attack;
pub mod counter;
pub mod disengage;
pub mod flank;
pub mod kick;
pub mod lunge;
pub mod npc;
pub mod overpower;
pub mod rattle;
pub mod volley;

use std::time::Duration;

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    components::{
        behaviour::Side,
        engagement::{Engagement, EngagementMember},
        grit::Grit,
        heading::Heading,
        hex_assignment::AssignedHex,
        npc_recovery::NpcRecovery,
        reaction_queue::{QueuedThreat, ReactionQueue},
        recovery::GlobalRecovery,
        resources::{Endurance, Health, RespawnTimer, Stamina},
        status::Status,
        target::Target,
        ActorAttributes, AttackRange, Loc, Swing,
    },
    components::entity_type::EntityType,
    message::{AbilityType, ClearType, Component as MessageComponent, Do, Event as GameEvent, Try},
    plugins::nntree::NNTree,
    resources::map::Map,
    systems::{
        combat::{queue::clear_threats, combos::{may_use, reacts_through, recovery_after}},
        targeting,
    },
};

use crate::systems::{behaviour::chase::Chase, combat::landing};

/// How often NPC signatures are looked at.
const CHECK: Duration = Duration::from_millis(500);

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
    /// The caster's own reach, in tiles
    pub reach: i32,
    /// The caster's full health
    pub health_max: f32,
    /// A strike's target, a living hostile within reach and arc; for a
    /// reaction, whoever it was asked about, unchecked
    pub target: Option<Entity>,
    pub target_loc: Option<Loc>,
}

impl Cast {
    /// The actor a strike lands on and where it stands
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
    pub poised: Query<'w, 's, &'static disengage::Poised>,
    pub targets: Query<'w, 's, (Entity, &'static Target)>,
    pub npcs: Query<'w, 's, (Entity, &'static EntityType, &'static mut NpcRecovery), With<Chase>>,
    pub members: Query<'w, 's, &'static EngagementMember>,
    pub engagements: Query<'w, 's, &'static Engagement>,
    pub assigned: Query<'w, 's, &'static AssignedHex>,
    pub map: Res<'w, Map>,
    pub nntree: Res<'w, NNTree>,
    pub time: Res<'w, Time>,
    pub runtime: Res<'w, crate::resources::RunTime>,
}

/// Uses every ability asked for this frame: what players asked, in the
/// order asked, then the auto-attacks that have come due, then each
/// [`CHECK`] each NPC's signature.
pub fn use_abilities(mut reader: MessageReader<Try>, mut abilities: Abilities, mut since_check: Local<Duration>) {
    for message in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target } } = message else { continue };
        abilities.ask(*ent, *ability, *target);
    }
    abilities.swing();
    *since_check += abilities.time.delta();
    if *since_check < CHECK {
        return;
    }
    *since_check -= CHECK;
    abilities.signatures();
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

    /// The gate. `asked` is the target a strike names, or for a reaction
    /// whoever it answers. Errs with the reason it was refused, or with none
    /// where there is nothing to tell: a dead caster, a swing not yet due.
    fn cast(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>) -> Result<(), Option<AbilityFailReason>> {
        let tuning = common_bevy::tuning::tuning();
        let Ok((&loc, &attrs, health, heading, side, range, dead)) = self.actors.get(ent) else { return Err(None) };
        if dead {
            return Err(None);
        }
        let (heading, side) = (heading.copied(), side.copied());
        let reach = range.copied().unwrap_or_default().0;
        let health_max = health.max;
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
        let mut cast = Cast { ent, loc, attrs, reach, health_max, target: asked, target_loc: None };
        if let Some(within) = ability.reach(reach) {
            let target = asked.ok_or(Some(AbilityFailReason::NoTargets))?;
            let Ok((&target_loc, _, _, _, target_side, _, target_dead)) = self.actors.get(target) else {
                return Err(Some(AbilityFailReason::NoTargets));
            };
            let hostile = side.zip(target_side.copied()).is_some_and(|(own, theirs)| own.is_hostile_to(theirs));
            if target_dead || !hostile {
                return Err(Some(AbilityFailReason::NoTargets));
            }
            if !within.contains(&loc.distance(&target_loc)) {
                return Err(Some(AbilityFailReason::OutOfRange));
            }
            if !in_arc(heading.as_ref(), Some(&attrs), &loc, &target_loc) {
                return Err(Some(AbilityFailReason::NotFacing));
            }
            cast.target_loc = Some(target_loc);
        } else {
            cast.target_loc = asked.and_then(|asked| self.actors.get(asked).ok()).map(|(&loc, ..)| loc);
        }

        let cost = tuning.cost(ability);
        if self.stamina.get(ent).map_or(true, |stamina| stamina.state < cost) {
            return Err(Some(AbilityFailReason::InsufficientStamina));
        }

        // The ability's own effect, which may still refuse before it
        // changes anything; it names whom its recovery is contested by
        let opponent = match ability {
            AbilityType::AutoAttack => auto_attack::swing(self, &cast),
            AbilityType::Lunge => lunge::strike(self, &cast),
            AbilityType::Overpower => overpower::strike(self, &cast),
            AbilityType::Rattle => rattle::strike(self, &cast),
            AbilityType::Volley => volley::strike(self, &cast),
            AbilityType::Flank => flank::strike(self, &cast),
            AbilityType::Counter => counter::answer(self, &cast),
            AbilityType::Kick => kick::answer(self, &cast),
            AbilityType::Disengage => disengage::answer(self, &cast),
        }.map_err(Some)?;

        if cost > 0.0 {
            if let Ok(mut stamina) = self.stamina.get_mut(ent) {
                stamina.state -= cost;
                self.writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Stamina(*stamina) } });
            }
        }
        // Endurance is spent beside the stamina and refuses nothing: the
        // recovery runs by how spent the actor was as it used the ability
        let mut fatigue = 0.0;
        if ability != AbilityType::AutoAttack {
            if let Ok(mut endurance) = self.endurance.get_mut(ent) {
                fatigue = endurance.fatigue();
                endurance.state = (endurance.state - attrs.skill_endurance()).max(0.0);
                self.writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Endurance(*endurance) } });
            }
        }
        // Every client near draws it
        self.writer.write(Do { event: GameEvent::UseAbility { ent, ability, target: opponent } });
        if let Some(target_loc) = cast.target_loc.filter(|_| !ability.is_reaction()) {
            stride(ent, heading.as_ref(), &loc, &target_loc, &mut self.commands);
        }
        if ability != AbilityType::AutoAttack {
            let against = opponent.and_then(|opponent| self.actors.get(opponent).ok()).map(|(_, attrs, ..)| *attrs);
            landing::recover(ent, recovery_after(ability, prior.as_ref(), &attrs, against.as_ref(), fatigue), &mut self.commands, &mut self.writer);
        }
        Ok(())
    }

    /// Queues a skill's strike of `damage` from `cast`'s caster on `target`.
    /// What the caster's Grit has banked strikes with it and is spent: every
    /// skill that strikes deals its damage through here, and an auto-attack
    /// or a reaction's return never does.
    pub fn strike(&mut self, cast: &Cast, target: Entity, damage: f32, ability: AbilityType) {
        let banked = self.grits.get_mut(cast.ent).map_or(0.0, |mut grit| grit.spend());
        self.deal(cast.ent, target, damage + banked, ability, 0.0);
    }

    /// Queues a blow of `base_damage` from `source` on `target` as
    /// `ability`'s, a wound where `dot` is the damage of each of its ticks.
    pub fn deal(&mut self, source: Entity, target: Entity, base_damage: f32, ability: AbilityType, dot: f32) {
        self.commands.trigger(Try {
            event: GameEvent::DealDamage { source, target, base_damage, ability: Some(ability), dot },
        });
    }

    /// Takes the threats `clear_type` names out of `ent`'s queue, tells its
    /// clients where any stood there, and returns them: a reaction's are
    /// `ClearType::Span` of its user's span (`ActorAttributes::span`).
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

/// Breaks `ent`'s stride where the strike it just made from `from` at `to`
/// crossed its line (`targeting::across`).
pub fn stride(ent: Entity, heading: Option<&Heading>, from: &Loc, to: &Loc, commands: &mut Commands) {
    if targeting::across(heading, from, to) {
        commands.trigger(Try { event: GameEvent::Stumble { ent } });
    }
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

    #[test]
    fn a_strike_needs_a_living_hostile_within_reach_and_arc() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let ally = actor(&mut app, Side::PLAYERS, 1);
        let far = actor(&mut app, Side::WILD, 3);
        let behind = actor(&mut app, Side::WILD, -1);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        let overpower = |app: &mut App, target| refused(app, caster, AbilityType::Overpower, target);
        assert_eq!(overpower(&mut app, None), Some(AbilityFailReason::NoTargets), "no target named");
        assert_eq!(overpower(&mut app, Some(ally)), Some(AbilityFailReason::NoTargets), "an ally is no target");
        assert_eq!(overpower(&mut app, Some(far)), Some(AbilityFailReason::OutOfRange));
        assert_eq!(overpower(&mut app, Some(behind)), Some(AbilityFailReason::NotFacing));

        let said = ask(&mut app, caster, AbilityType::Overpower, Some(near));
        assert!(used(&said, AbilityType::Overpower));
        let world = app.world();
        assert!(world.get::<Stamina>(caster).unwrap().state < 100.0, "it is paid for");
        assert!(world.get::<Endurance>(caster).unwrap().state < 100.0, "in endurance too");
        assert!(world.get::<GlobalRecovery>(caster).is_some(), "and leaves its user recovering");
        let told = |event: &GameEvent| matches!(event, GameEvent::Incremental { ent, component: MessageComponent::Recovery(_) } if *ent == caster);
        assert!(said.iter().any(told), "a recovery its clients are sent whole");
        assert_eq!(world.get::<ReactionQueue>(near).unwrap().threats.len(), 1, "its blow waits in the target's queue");
        assert!(world.get::<ReactionQueue>(far).unwrap().threats.is_empty(), "a refused one queued nothing");
    }

    #[test]
    fn a_recovery_refuses_the_next_ability_and_a_swing_keeps_its_own_cadence() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.update();

        assert!(used(&ask(&mut app, caster, AbilityType::Overpower, Some(near)), AbilityType::Overpower));
        assert_eq!(refused(&mut app, caster, AbilityType::Lunge, Some(near)), Some(AbilityFailReason::OnCooldown), "recovering, it uses no other ability");

        let said = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert!(used(&said, AbilityType::AutoAttack), "an auto-attack is outside the recovery");
        let spent = app.world().get::<Endurance>(caster).unwrap().state;
        let again = ask(&mut app, caster, AbilityType::AutoAttack, Some(near));
        assert!(!used(&again, AbilityType::AutoAttack), "the next is not due yet");
        assert_eq!(refused(&mut app, caster, AbilityType::AutoAttack, Some(near)), None, "and a swing not due is refused without a reason");
        assert_eq!(app.world().get::<Endurance>(caster).unwrap().state, spent, "a swing costs no endurance");
    }

    #[test]
    fn a_reaction_answers_the_queue_and_needs_something_in_it() {
        let mut app = arena();
        let defender = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(attacker).insert(Heading::from_hex(Qrz { q: -1, r: 0, z: 0 }));
        app.update();

        assert_eq!(refused(&mut app, defender, AbilityType::Counter, None), Some(AbilityFailReason::NoTargets), "nothing queued, nothing to counter");

        assert!(used(&ask(&mut app, attacker, AbilityType::Overpower, Some(defender)), AbilityType::Overpower));
        assert_eq!(app.world().get::<ReactionQueue>(defender).unwrap().threats.len(), 1);
        let said = ask(&mut app, defender, AbilityType::Counter, None);
        assert!(used(&said, AbilityType::Counter));
        assert!(app.world().get::<ReactionQueue>(defender).unwrap().threats.is_empty(), "the blow is cleared");
        assert!(said.iter().any(|event| matches!(event, GameEvent::UseAbility { target, .. } if *target == Some(attacker))), "and answered to its source");
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
            let threat = create_threat(attacker, &plain, &plain, 10.0, Some(AbilityType::Overpower), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat, at);
        }

        assert!(used(&ask(&mut app, defender, AbilityType::Counter, None), AbilityType::Counter));
        let left: Vec<Duration> = app.world().get::<ReactionQueue>(defender).unwrap().threats.iter().map(|threat| threat.inserted_at).collect();
        assert_eq!(left, vec![span * 4], "the front and the one landing within its span are taken; the later one stands");
    }

    #[test]
    fn grit_banks_a_blow_let_land_and_the_next_skill_spends_it() {
        let mut app = arena();
        let gritty = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(gritty).insert(ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0));
        app.world_mut().entity_mut(attacker).insert(Heading::from_hex(Qrz { q: -1, r: 0, z: 0 }));
        app.update();
        let bank = |app: &App| app.world().get::<Grit>(gritty).unwrap().bank;

        assert!(used(&ask(&mut app, attacker, AbilityType::Overpower, Some(gritty)), AbilityType::Overpower));
        assert_eq!(bank(&app), 0.0, "a blow still in the queue banks nothing");
        app.world_mut().write_message(Try { event: GameEvent::Dismiss { ent: gritty } });
        app.update();
        let banked = bank(&app);
        assert!(banked > 0.0, "let land, a share of it is banked");

        assert!(used(&ask(&mut app, gritty, AbilityType::AutoAttack, Some(attacker)), AbilityType::AutoAttack));
        assert_eq!(bank(&app), banked, "a swing leaves the bank be");
        assert!(used(&ask(&mut app, gritty, AbilityType::Overpower, Some(attacker)), AbilityType::Overpower));
        assert_eq!(bank(&app), 0.0, "a skill strikes with it");
    }

    #[test]
    fn the_dead_use_nothing() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(caster).insert(RespawnTimer::new(Duration::ZERO));
        app.update();

        let said = ask(&mut app, caster, AbilityType::Overpower, Some(near));
        assert!(!used(&said, AbilityType::Overpower));
        assert_eq!(refused(&mut app, caster, AbilityType::Overpower, Some(near)), None, "refused without a reason");
    }
}
