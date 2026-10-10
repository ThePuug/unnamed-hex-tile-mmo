//! Every ability an actor uses goes through one gate, [`Abilities::cast`].
//! The gate asks, in order and the same of every ability: is the caster
//! alive; is it out of recovery, or taking the combo it was offered, or
//! reacting through the recovery (an auto-attack asks its clock instead);
//! does it strike a living hostile within the ability's reach and its arc;
//! does a pin hold it, for a skill that moves its user. Nothing it holds
//! refuses it. Then the ability's own effect runs, its endurance is paid,
//! the clients are told, a strike across the caster's line breaks its
//! stride, an early reaction at Preparation's capstone slips its user, and
//! the recovery starts, longer for an actor whose endurance is spent. A
//! skill costs endurance, more in an Intimidating foe's zone and less for
//! a reaction that answers many at Awareness's facet; an auto-attack is
//! free.
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
pub mod leap;
pub mod npc;
pub mod parry;
pub mod punish;
pub mod stride;
pub mod strike;

use std::time::Duration;

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    components::{
        behaviour::Side,
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
    moment::Moment,
    resources::map::Map,
    systems::{
        combat::{queue::clear_threats, combos::{may_use, recovery_after, timing, Early, Timing}},
        targeting,
    },
};

use crate::{
    behaviour::{chase::Chase, perception::{Sight, Skill}},
    engagement::{Engagement, EngagementMember},
    landing,
};
use common_bevy::tuning::Tuning;

/// Why the gate refused an ability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFailReason {
    NoTargets,
    /// The caster is in a recovery the ability may not be used through
    Recovering,
    OutOfRange,
    /// The target stands outside the caster's arc
    NotFacing,
    /// A skill that moves its user, held in an Intimidating foe's zone
    Pinned,
}

/// One use of an ability the gate has let through: who uses it, from where,
/// at whom and when.
pub struct Cast {
    pub ent: Entity,
    pub ability: AbilityType,
    /// The moment it was used on the game clock: a player's press as its
    /// client stamped it, or as it arrived where that had passed
    /// ([`Press`]); an NPC's now. A reaction's band is judged here.
    pub at: Moment,
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
pub struct LastSkill(pub Moment);

/// What the gate asks of `ability` before it does anything, `reacting`
/// where this use is a reaction (`AbilityType::reacts`), in order: out
/// of `prior`, the recovery its caster is in, or let through it (an
/// auto-attack's own clock is asked apart); for one with a reach out of
/// `reach`, a target at `foe`'s distance and inside its arc, None for no
/// target; for one that moves its user, no pin on it (`pinned`). An NPC's
/// skills channel asks the same of what it perceives, so it never weighs a
/// skill the gate refuses.
pub fn admits(ability: AbilityType, reacting: bool, prior: Option<&GlobalRecovery>, attrs: &ActorAttributes, reach: i32, foe: Option<(i32, bool)>, pinned: bool) -> Result<(), AbilityFailReason> {
    if pinned && ability.moves() {
        return Err(AbilityFailReason::Pinned);
    }
    if ability != AbilityType::AutoAttack && !may_use(ability, reacting, prior, attrs) {
        return Err(AbilityFailReason::Recovering);
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
    pressed: Moment,
    /// The server's clock as it came
    arrived: Moment,
}

impl Press {
    /// The moment it is judged at: the later of when it was pressed and
    /// when it arrived
    fn at(&self) -> Moment {
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
    fn cast(&mut self, ent: Entity, ability: AbilityType, asked: Option<Entity>, pressed: Moment, arrived: Moment) -> Result<(), Option<AbilityFailReason>> {
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
            let due = self.swings.get(ent).is_ok_and(|swing| swing.waited(Moment::ZERO + self.time.elapsed()).is_some());
            if !due || Status::holds(status.as_ref()) {
                return Err(None);
            }
        }

        // A strike needs a living hostile within its reach and its arc;
        // reach is measured as a swing measures it, the first level of
        // height between free
        let mut cast = Cast { ent, ability, at, loc, attrs, side, reach, target: asked, target_loc: None };
        if ability.reach(reach).is_some() {
            cast.target_loc = self.foe(&cast).ok().map(|(_, target_loc)| target_loc);
        }
        let foe = cast.target_loc.map(|target_loc| (loc.distance(&target_loc), in_arc(&tuning, heading.as_ref(), Some(&attrs), &loc, &target_loc)));

        let across = cast.target_loc.is_some_and(|target_loc| targeting::across(heading.as_ref(), &loc, &target_loc));
        // A Leap with its target in reach leaps clear, a reaction
        let reacting = ability.reacts(ability == AbilityType::Leap
            && self.foe(&cast).is_ok_and(|(_, target_loc)| loc.distance(&target_loc) <= reach));
        let pinned = status.is_some_and(|status| status.is_pinned());
        admits(ability, reacting, prior.as_ref(), &attrs, reach, foe, pinned).map_err(Some)?;
        let early = timing(ability, reacting, prior.as_ref(), &attrs);
        let price = self.price(&cast, reacting, status.as_ref());

        // The recovery runs by how spent the actor is as it uses the ability
        let fatigue = self.endurance.get(ent).map_or(0.0, |endurance| endurance.fatigue(&tuning));

        // The ability's own effect, which may still refuse before it
        // changes anything; it names whom its recovery is contested by
        let opponent = match ability {
            AbilityType::AutoAttack => auto_attack::swing(self, &cast),
            AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower => strike::strike(self, &cast),
            AbilityType::Punish => punish::strike(self, &cast),
            AbilityType::Parry => parry::answer(self, &cast),
            AbilityType::Counter => counter::answer(self, &cast),
            AbilityType::Leap => leap::leap(self, &cast),
            AbilityType::PerfectStride => stride::take(self, &cast),
        }.map_err(Some)?;

        // Endurance is spent and refuses nothing; an auto-attack is free
        if ability != AbilityType::AutoAttack {
            self.tire(ent, price);
        }
        // An early reaction at Preparation's capstone slips its user away
        // from what it answered; a Leap clear carries its user already
        if early == Some(Timing::Early(Early::Preparation)) && ability != AbilityType::Leap && !pinned {
            self.slip(&cast, opponent);
        }
        // Every client near draws it
        self.writer.write(Do { event: GameEvent::UseAbility { ent, ability, target: opponent, at: pressed, arrived } });
        // A strike across the caster's line breaks its stride, but in a
        // Perfect Stride
        if across && !self.strides(ent) {
            self.commands.trigger(Try { event: GameEvent::Stumble { ent } });
        }
        if ability != AbilityType::AutoAttack {
            self.commands.entity(ent).try_insert(LastSkill(Moment::ZERO + self.time.elapsed()));
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

    /// What `cast` costs its user: its skill's price, times the toll of an
    /// Intimidating foe's zone it stands in, less what Awareness's facet pays
    /// back for each threat past the first a reaction would take
    fn price(&self, cast: &Cast, reacting: bool, status: Option<&Status>) -> f32 {
        let tuning = *self.tuning;
        let toll = status.map_or(1.0, Status::price);
        let taken = if reacting {
            let (at, span) = self.band(cast);
            self.queues.get(cast.ent).map_or(0, |queue| queue.swept(at, span).count())
        } else {
            0
        };
        let refund = (cast.attrs.awareness_refund(&tuning) * taken.saturating_sub(1) as f32).min(1.0);
        cast.attrs.skill_endurance(&tuning, cast.ability) * toll * (1.0 - refund)
    }

    /// The band a reaction by `cast` takes from, where it starts and how
    /// long it runs: from the press, or with Awareness's capstone from the
    /// threat it snaps to (`ReactionQueue::band`)
    fn band(&self, cast: &Cast) -> (Moment, Duration) {
        let tuning = *self.tuning;
        let span = cast.attrs.span(&tuning);
        let at = self.queues.get(cast.ent).map_or(cast.at, |queue| queue.band(cast.at, cast.attrs.awareness_snap(&tuning)));
        (at, span)
    }

    /// Slips `cast`'s user its Preparation capstone's tiles away from
    /// `from`, the source of what it answered, over the ground as a Leap
    /// clear goes, inside its leash
    fn slip(&mut self, cast: &Cast, from: Option<Entity>) {
        let tuning = *self.tuning;
        let Some(tiles) = cast.attrs.slip(&tuning) else { return };
        let Some((&from, ..)) = from.and_then(|from| self.actors.get(from).ok()) else { return };
        let Some(landing) = crate::leap::away(&self.map, *cast.loc, *from, tiles, self.leash(cast.ent)) else { return };
        crate::leap::slide(cast.ent, landing, crate::leap::LEAP_MS, None, &mut self.commands, &mut self.writer);
    }

    /// Whether `ent` is in a Perfect Stride now
    pub fn strides(&self, ent: Entity) -> bool {
        self.statuses.get(ent).is_ok_and(|status| status.is_striding())
    }

    /// Queues a skill's strike on `target` for `damage`, struck at once.
    /// Every skill that strikes deals its damage through here, and an
    /// auto-attack or a reaction's return never does.
    pub fn strike(&mut self, cast: &Cast, target: Entity, damage: f32, ability: AbilityType) {
        self.deal(cast.ent, target, damage, ability, Duration::ZERO);
    }

    /// Queues a blow of `base_damage` from `source` on `target` as
    /// `ability`'s, struck `delay` after now.
    pub fn deal(&mut self, source: Entity, target: Entity, base_damage: f32, ability: AbilityType, delay: Duration) {
        self.commands.trigger(Try {
            event: GameEvent::DealDamage { source, target, base_damage, ability: Some(ability), dot: 0.0, delay },
        });
    }

    /// Takes the threats in `cast`'s band out of its user's queue, those
    /// landing within its span of where its band starts ([`Self::band`]):
    /// what a reaction clears, and a Leap clear of its target. A reaction
    /// timed to nothing clears nothing, and is paid for all the same.
    pub fn answer(&mut self, cast: &Cast) -> Vec<QueuedThreat> {
        let (at, span) = self.band(cast);
        self.clear(cast.ent, ClearType::Span { at, span })
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
    pub fn game_now(&self) -> Moment {
        self.runtime.now(&self.time)
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
            ReactionQueue::default(),
            Heading::from_hex(Qrz { q: 1, r: 0, z: 0 }),
            CombatState::default(),
        )).id();
        app.world_mut().entity_mut(ent).insert(NearestNeighbor::new(ent, loc));
        ent
    }

    /// The game's clock in `app`, the one a threat's times are on
    fn game_now(app: &App) -> Moment {
        app.world().resource::<crate::RunTime>().now(app.world().resource::<Time>())
    }

    fn ask(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>) -> Vec<GameEvent> {
        let at = game_now(app);
        app.world_mut().resource_mut::<Said>().0.clear();
        app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent, ability, target, at, arrived: Moment::ZERO } });
        app.update();
        std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
    }

    /// Sets the game's clock in `app` to `at`. The clock reads its `Time`
    /// in whole milliseconds (`RunTime::now`), so the offset is taken
    /// against that reading, or it lands a millisecond short.
    fn clock_to(app: &mut App, at: Moment) {
        let elapsed = app.world().resource::<Time>().elapsed().as_millis() as u64;
        app.world_mut().resource_mut::<crate::RunTime>().elapsed_offset = at.since(Moment::from_millis(elapsed)).as_millis();
    }

    /// [`ask`], pressed with the game's clock set to `at`
    fn press(app: &mut App, ent: Entity, ability: AbilityType, target: Option<Entity>, at: Moment) -> Vec<GameEvent> {
        clock_to(app, at);
        ask(app, ent, ability, target)
    }

    /// When the soonest threat in `ent`'s queue lands
    fn soonest(app: &App, ent: Entity) -> Moment {
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
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::Recovering), "recovering, it uses no other ability");

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
        assert_eq!(refused(&mut app, plain, AbilityType::Frenzy, Some(near)), Some(AbilityFailReason::Recovering), "with no Ferocity the next bite waits to unlock");

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
            let at = Moment::from_millis(millis);
            let threat = create_threat(&tuning, attacker, &plain, &plain, damage, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat);
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
        let queued = |app: &mut App, at: Moment| {
            let threat = create_threat(&tuning, attacker, &plain, &plain, 10.0, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat);
            threat.lands_at()
        };
        let stamped = |app: &mut App, at: Moment| {
            app.world_mut().resource_mut::<Said>().0.clear();
            app.world_mut().write_message(Try { event: GameEvent::UseAbility { ent: defender, ability: AbilityType::Parry, target: None, at, arrived: Moment::ZERO } });
            app.update();
            std::mem::take(&mut app.world_mut().resource_mut::<Said>().0)
        };
        let answered = |said: &[GameEvent]| said.iter().any(|event| matches!(event, GameEvent::ClearQueue { clear_type: ClearType::Span { .. }, .. }));

        // Stamped ahead of the server's clock, as a press on time arrives
        let lands = queued(&mut app, Moment::ZERO);
        clock_to(&mut app, lands - Duration::from_millis(400));
        assert!(!used(&stamped(&mut app, lands - EARLY), AbilityType::Parry), "it is held");
        clock_to(&mut app, lands - EARLY);
        app.world_mut().resource_mut::<Said>().0.clear();
        app.update();
        assert!(answered(&app.world().resource::<Said>().0), "and judged at its moment");
        assert!(queue(&app, defender).is_empty());

        // Stamped long before it arrived, its band starts as it arrives
        app.world_mut().entity_mut(defender).remove::<GlobalRecovery>();
        let lands = queued(&mut app, Moment::from_millis(5_000));
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
        for at in [Moment::ZERO, Moment::ZERO + span / 2, Moment::ZERO + span * 4] {
            let threat = create_threat(&tuning, attacker, &plain, &plain, 10.0, Some(AbilityType::Frenzy), at, 0.0, 0.0);
            insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(defender).unwrap(), threat);
        }

        let lands = soonest(&app, defender);
        assert!(used(&press(&mut app, defender, AbilityType::Counter, None, lands), AbilityType::Counter));
        let left: Vec<Moment> = queue(&app, defender).iter().map(|threat| threat.inserted_at).collect();
        assert_eq!(left, vec![Moment::ZERO + span * 4], "the two landing within its band are taken; the later one stands");
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

    /// A blow of `damage` from `source` on `ent`, struck at `at` on the
    /// game's clock as a strike queues one; when it lands
    fn threat_on(app: &mut App, ent: Entity, source: Entity, damage: f32, at: Moment) -> Moment {
        use common_bevy::systems::combat::queue::{create_threat, insert_threat};
        let tuning = Tuning::DEFAULT;
        let plain = ActorAttributes::default();
        let threat = create_threat(&tuning, source, &plain, &plain, damage, Some(AbilityType::Frenzy), at, 0.0, 0.0);
        insert_threat(&mut app.world_mut().get_mut::<ReactionQueue>(ent).unwrap(), threat);
        threat.lands_at()
    }

    #[test]
    fn a_pin_refuses_a_skill_that_moves_its_user_and_nothing_else() {
        let mut app = arena();
        let leaper = actor(&mut app, Side::PLAYERS, 0);
        let far = actor(&mut app, Side::WILD, 6);
        let near = actor(&mut app, Side::WILD, 1);
        let mut pinned = Status::default();
        pinned.pin(5.0);
        app.world_mut().entity_mut(leaper).insert(pinned);
        app.update();
        assert_eq!(refused(&mut app, leaper, AbilityType::Leap, Some(far)), Some(AbilityFailReason::Pinned));
        assert!(used(&ask(&mut app, leaper, AbilityType::Feint, Some(near)), AbilityType::Feint), "a strike still goes");
        assert_eq!(admits(AbilityType::Leap, false, None, &ActorAttributes::default(), 2, None, true), Err(AbilityFailReason::Pinned), "and the minds ask the same");
    }

    #[test]
    fn a_toll_raises_every_price_in_the_zone() {
        let mut app = arena();
        let free = actor(&mut app, Side::PLAYERS, 0);
        let taxed = actor(&mut app, Side::PLAYERS, 2);
        let foe = actor(&mut app, Side::WILD, 1);
        turned_to(&mut app, taxed, -1);
        let mut toll = Status::default();
        toll.tax(1.3, 5.0);
        app.world_mut().entity_mut(taxed).insert(toll);
        app.update();
        let spent = |app: &App, ent| 100.0 - app.world().get::<Endurance>(ent).unwrap().state;
        assert!(used(&ask(&mut app, free, AbilityType::Feint, Some(foe)), AbilityType::Feint));
        assert!(used(&ask(&mut app, taxed, AbilityType::Feint, Some(foe)), AbilityType::Feint));
        assert!((spent(&app, taxed) - 1.3 * spent(&app, free)).abs() < 1e-3, "{} against {}", spent(&app, taxed), spent(&app, free));
    }

    #[test]
    fn awareness_pays_back_for_answering_many_with_one_and_snaps_its_band_at_the_capstone() {
        let mut app = arena();
        let faceted = actor(&mut app, Side::PLAYERS, 0);
        let plain = actor(&mut app, Side::PLAYERS, 3);
        let snapping = actor(&mut app, Side::PLAYERS, -3);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(faceted).insert(ActorAttributes::new(0, 0, 0, 0, 0, 0, 9, 0, 0));
        app.world_mut().entity_mut(snapping).insert(ActorAttributes::new(0, 0, 0, 0, 0, 0, 18, 0, 0));
        app.update();
        let spent = |app: &App, ent| 100.0 - app.world().get::<Endurance>(ent).unwrap().state;

        for ent in [faceted, plain] {
            let lands = [1000, 1050, 1100].map(|millis| threat_on(&mut app, ent, attacker, 10.0, Moment::from_millis(millis)))[0];
            assert!(used(&press(&mut app, ent, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry));
        }
        assert!(spent(&app, faceted) < spent(&app, plain), "three in one band, the facet pays some back");

        // The capstone's band starts at a threat landing just after the
        // press, so it takes one landing past the band from the press
        let next = threat_on(&mut app, snapping, attacker, 10.0, Moment::from_millis(5150));
        threat_on(&mut app, snapping, attacker, 10.0, Moment::from_millis(6000));
        assert!(used(&press(&mut app, snapping, AbilityType::Parry, None, next - Duration::from_millis(150)), AbilityType::Parry));
        assert!(queue(&app, snapping).is_empty(), "both taken, the later 1.0s after the press");
    }

    #[test]
    fn preparations_capstone_slips_an_early_answer_away_from_its_source() {
        let mut app = arena();
        let slipping = actor(&mut app, Side::PLAYERS, 0);
        let attacker = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(slipping).insert(ActorAttributes::new(0, 0, 0, 18, 0, 0, 0, 0, 0));
        app.update();
        let struck = Moment::from_millis(10_000);
        assert!(used(&press(&mut app, slipping, AbilityType::Feint, Some(attacker), struck), AbilityType::Feint), "a strike first");
        // A blow landing while the strike's recovery still runs
        let window = Duration::from_secs_f32(Tuning::DEFAULT.reaction_window);
        let lands = threat_on(&mut app, slipping, attacker, 10.0, struck + Duration::from_millis(300) - window);
        assert!(used(&press(&mut app, slipping, AbilityType::Parry, None, lands - EARLY), AbilityType::Parry), "answered early in the chain");
        app.update();
        let at = app.world().get::<Loc>(slipping).unwrap();
        assert!(at.q < 0, "slipped away from its source: {:?}", **at);
    }

    #[test]
    fn graces_capstone_breaks_a_flanked_foes_stride_and_patiences_spends_its_stacks_on_a_certain_crit() {
        let tuning = Tuning::DEFAULT;
        let mut app = arena();
        let graceful = actor(&mut app, Side::PLAYERS, 0);
        let flanked = actor(&mut app, Side::WILD, 1);
        let patient = actor(&mut app, Side::PLAYERS, -2);
        let pressing = actor(&mut app, Side::WILD, -1);
        app.world_mut().entity_mut(graceful).insert(ActorAttributes::new(18, 0, 0, 0, 0, 0, 0, 0, 0));
        app.world_mut().entity_mut(patient).insert(ActorAttributes::new(0, 0, 0, 0, 0, 0, -18, 0, 0));
        // Of its level, so no level gap weighs the blow
        app.world_mut().entity_mut(pressing).insert(ActorAttributes::default().at_level(18));
        // The flanked foe faces away, its back to the striker
        turned_to(&mut app, flanked, 1);
        let mut stacked = Status::default();
        (0..10).for_each(|_| stacked.overcommit(tuning.overcommit_secs));
        app.world_mut().entity_mut(pressing).insert(stacked);
        turned_to(&mut app, pressing, -1);
        app.update();

        assert!(used(&ask(&mut app, graceful, AbilityType::Feint, Some(flanked)), AbilityType::Feint));
        assert!(queue(&app, flanked)[0].stride > 0.0, "its flank strike will break the foe's stride");
        let threat = app.world_mut().get_mut::<ReactionQueue>(flanked).unwrap().threats.pop_front().unwrap();
        app.world_mut().trigger(Try { event: GameEvent::ResolveThreat { ent: flanked, threat } });
        app.update();
        assert!(app.world().get::<Status>(flanked).is_some_and(|status| status.stride.is_some()), "and does as it lands");

        assert!(used(&ask(&mut app, patient, AbilityType::Feint, Some(pressing)), AbilityType::Feint));
        let feint = tuning.damage(AbilityType::Feint) * tuning.potency_base;
        assert!(queue(&app, pressing)[0].damage > feint * (1.0 + tuning.damage_spread), "a certain crit");
        assert_eq!(app.world().get::<Status>(pressing).unwrap().overcommits(), 0, "spending every stack");
    }

    #[test]
    fn the_dead_use_nothing() {
        let mut app = arena();
        let caster = actor(&mut app, Side::PLAYERS, 0);
        let near = actor(&mut app, Side::WILD, 1);
        app.world_mut().entity_mut(caster).insert(RespawnTimer::new(Moment::ZERO));
        app.update();

        let said = ask(&mut app, caster, AbilityType::Frenzy, Some(near));
        assert!(!used(&said, AbilityType::Frenzy));
        assert_eq!(refused(&mut app, caster, AbilityType::Frenzy, Some(near)), None, "refused without a reason");
    }
}
