use bevy::{ecs::{query::QueryData, system::SystemParam}, prelude::*};

use common_bevy::{
    components::{
        activation::{Activating, Recharges}, entity_type::EntityType, heading::Heading, loadout::Loadout, resources::Endurance,
        status::Status, target::Target, Actor, ActorAttributes, AttackRange, Loc,
    },
    message::AbilityType,
    moment::Moment,
    systems::{combat::gate::{admits, AbilityFailReason, Standing}, targeting},
};
use common_bevy::tuning::Tuning;

use crate::systems::{skill_text, struck};

/// Marker component for individual ability slot UI
#[derive(Component)]
pub struct AbilitySlot {
    pub ability: Option<AbilityType>,
}

/// The row the viewed actor's ability slots stand in
#[derive(Component)]
pub struct AbilitySlots;

/// Marker for ability slot icon
#[derive(Component)]
pub struct SlotIcon;

/// Marker for ability slot keybind label
#[derive(Component)]
pub struct SlotKeybind;

/// The shade over the part of a slot its recharge has yet to light: hung
/// from the top, so the slot fills from the bottom as the recharge runs
#[derive(Component)]
pub struct CooldownOverlay;

/// Side of an ability slot and its border, in px.
pub const SLOT_PX: f32 = 80.;
const SLOT_BORDER_PX: f32 = 3.;
/// Room between the bar's slots, along a line and between its lines
const SLOT_GAP_PX: f32 = 10.;

/// Setup action bar UI, hung below the resource bars' line
/// Creates the ability slots, four to a row
pub fn setup(
    mut commands: Commands,
    query: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let camera = query.single().expect("query did not return exactly one result");

    commands.spawn((
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.),
            height: Val::Vw(crate::systems::resource_bars::LINE_VW),
            bottom: Val::Px(0.),
            left: Val::Px(0.),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            padding: UiRect::top(Val::Px(10.)),
            ..default()
        },
        Pickable::IGNORE,
        crate::components::ViewHud,
    ))
    .with_children(|parent| {
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(10.),
                ..default()
            },
        ))
        .with_children(|parent| {
            // The viewed actor's loadout, filled in by `sync_loadout`
            parent.spawn((bar_grid(), AbilitySlots));
        });  // Close .with_children from line 73 (action bar children)
    });  // Close outer .with_children
}

/// The bar's keys, a slot each of a `Loadout`'s eight.
pub const KEYS: [KeyCode; 8] = [
    KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR,
    KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::KeyF,
];

/// What stands on each of the bar's keys for an actor of `typ`: a player's
/// own bar, `held`; an NPC's skills in its bar's order from the first key.
pub fn loadout(typ: &EntityType, held: Option<&Loadout>) -> [Option<AbilityType>; 8] {
    use common_bevy::components::entity_type::actor::ActorIdentity;
    match typ {
        EntityType::Actor(actor) => match actor.identity {
            ActorIdentity::Player => held.copied().unwrap_or_default().0.map(Some),
            ActorIdentity::Npc(npc) => {
                let mut bar = [None; 8];
                for (slot, ability) in bar.iter_mut().zip(npc.bar()) {
                    *slot = Some(ability);
                }
                bar
            }
            ActorIdentity::Nest(_) | ActorIdentity::Settler => [None; 8],
        },
        _ => [None; 8],
    }
}

/// Fills the bar with the loadout of the actor the client sees as, anew
/// whenever that actor or its bar changes.
pub fn sync_loadout(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    seer: Query<(Entity, &EntityType, Option<&Loadout>), With<crate::components::Viewed>>,
    slots: Query<Entity, With<AbilitySlots>>,
    mut shown: Local<Option<(Entity, Option<Loadout>)>>,
) {
    let Ok(container) = slots.single() else { return };
    let seer = seer.single().ok();
    let seen = seer.map(|(ent, _, held)| (ent, held.copied()));
    if seen == *shown {
        return;
    }
    *shown = seen;
    commands.entity(container).despawn_related::<Children>();
    let Some((_, typ, held)) = seer else { return };
    commands.entity(container).with_children(|parent| {
        for (keybind, ability) in crate::caps!(KEYS).into_iter().zip(loadout(typ, held)) {
            spawn_slot(parent, keybind, ability, ability.map(|a| asset_server.load(icon_path(a))));
        }
    });
}

/// Where a skill's icon is: `icons/skills/` and its name in kebab case,
/// `PerfectStride` at `perfect-stride.png`, as the assets repo's icongen
/// names what it draws.
pub fn icon_path(ability: AbilityType) -> String {
    let mut stem = String::new();
    for (i, c) in format!("{ability:?}").chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            stem.push('-');
        }
        stem.push(c.to_ascii_lowercase());
    }
    format!("icons/skills/{stem}.png")
}


pub const READY: Color = Color::srgb(0.3, 0.8, 0.3);
const EMPTY: Color = Color::srgb(0.2, 0.2, 0.2);

/// How the bar lays its slots: four to a line.
pub fn bar_grid() -> Node {
    Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::px(4, SLOT_PX),
        column_gap: Val::Px(SLOT_GAP_PX),
        row_gap: Val::Px(SLOT_GAP_PX),
        ..default()
    }
}

/// A slot `side` px across under `parent`: its border and fill, `icon`
/// (`SlotIcon`) filling its inside when it holds a skill, and the cap of
/// `keybind` (`SlotKeybind`) in its corner. A slot `named` is the key's
/// own, where help shows what the key does; one not is a picture of a
/// slot, as a panel shows the bar. Returns the slot, for what else it
/// holds.
pub fn slot_frame<'a>(parent: &'a mut ChildSpawnerCommands, keybind: crate::systems::keycap::Cap, icon: Option<Handle<Image>>, side: f32, named: bool) -> EntityCommands<'a> {
    let mut slot = parent.spawn((
        Node {
            width: Val::Px(side),
            height: Val::Px(side),
            border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(READY),
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
    ));
    slot.with_children(|parent| {
        if let Some(icon) = icon {
            let inside = Val::Px(side - 2. * SLOT_BORDER_PX);
            parent.spawn((ImageNode::new(icon), Node { width: inside, height: inside, ..default() }, SlotIcon));
        }
        let mut cap = crate::systems::keycap::corner_keycap(parent, keybind);
        cap.insert(SlotKeybind);
        if !named {
            cap.remove::<crate::systems::keycap::Names>();
        }
    });
    slot
}

fn spawn_slot(parent: &mut ChildSpawnerCommands, keybind: crate::systems::keycap::Cap, ability: Option<AbilityType>, icon: Option<Handle<Image>>) {
    slot_frame(parent, keybind, icon, SLOT_PX, true).insert(AbilitySlot { ability }).with_children(|parent| {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(0.),
                top: Val::Px(0.),
                left: Val::Px(0.),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            CooldownOverlay,
        ));
    });
}

/// What the client holds of an actor that the gate asks of.
#[derive(QueryData)]
pub struct Stands {
    loc: &'static Loc,
    heading: Option<&'static Heading>,
    status: Option<&'static Status>,
    activating: Option<&'static Activating>,
    recharges: Option<&'static Recharges>,
    endurance: Option<&'static Endurance>,
    reach: Option<&'static AttackRange>,
    target: Option<&'static Target>,
    attrs: Option<&'static ActorAttributes>,
    /// The client's own character, who picks its targets here; another
    /// actor viewed has targets only the server knows
    controlled: Has<Actor>,
}

impl StandsItem<'_, '_> {
    /// The target a press would name (`input::update_keybits`)
    fn targets(&self) -> Option<Entity> {
        self.target.and_then(|target| target.entity)
    }

    /// The gate's answer for `ability` at `now` on the game clock, as far
    /// as the client can give it (`gate::admits`), with its target standing
    /// at `target`. A pool the client holds none of is never short; an
    /// actor viewed and not controlled is taken to have its target in
    /// reach.
    fn asks(&self, tuning: &Tuning, ability: AbilityType, now: Moment, target: Option<&Loc>) -> Result<(), AbilityFailReason> {
        let status = self.status.copied().unwrap_or_default();
        let standing = Standing {
            held: status.is_held(),
            pinned: status.is_pinned(),
            activating: self.activating.is_some_and(|activating| activating.0.is_some()),
            recharged: self.recharges.is_none_or(|recharges| recharges.is_ready(ability, now)),
            endurance: self.endurance.map_or(f32::INFINITY, |endurance| endurance.state),
        };
        let foe = if self.controlled {
            target.map(|target| (self.loc.distance(target), targeting::faces(self.heading, self.loc, target)))
        } else {
            Some((0, true))
        };
        admits(tuning, ability, &standing, &self.attrs.copied().unwrap_or_default(), self.reach.copied().unwrap_or_default().0, foe)
    }
}

/// The gate as the client asks it: the shared check, on the game clock,
/// of what the client holds.
#[derive(SystemParam)]
pub struct Gate<'w, 's> {
    tuning: Res<'w, Tuning>,
    server: Res<'w, crate::resources::Server>,
    time: Res<'w, Time>,
    locs: Query<'w, 's, &'static Loc>,
}

impl Gate<'_, '_> {
    /// Now, on the game clock recharges are stamped in
    fn now(&self) -> Moment {
        self.server.now(self.time.elapsed().as_millis())
    }

    /// The gate's answer for `ability` used by the actor that `stands` now
    fn asks(&self, stands: &StandsItem<'_, '_>, ability: AbilityType) -> Result<(), AbilityFailReason> {
        let target = stands.targets().and_then(|target| self.locs.get(target).ok());
        stands.asks(&self.tuning, ability, self.now(), target)
    }
}

/// How long a slot flashes after a press the gate would refuse
const REFUSED_FLASH: f32 = 0.25;
/// The least time between two reason words, so a key drummed raises one
const WORD_GAP: f32 = 0.5;
const FLASHED: Color = Color::WHITE;

/// When a press on a slot was last refused, on the client's clock, while
/// its flash lasts
#[derive(Component)]
pub struct Refused(Moment);

/// The player's presses on the bar, asked of the gate before the server
/// answers: what a press would be refused for, and the showing of it.
#[derive(SystemParam)]
pub struct Presses<'w, 's> {
    gate: Gate<'w, 's>,
    player: Query<'w, 's, (Stands, &'static Transform), With<Actor>>,
    slots: Query<'w, 's, (Entity, &'static AbilitySlot)>,
    commands: Commands<'w, 's>,
    said: Local<'s, Option<Moment>>,
}

impl Presses<'_, '_> {
    /// Why the gate would refuse the player's `ability` now, None where it
    /// would not
    pub fn refusal(&self, ability: AbilityType) -> Option<AbilityFailReason> {
        let (stands, _) = self.player.single().ok()?;
        self.gate.asks(&stands, ability).err()
    }

    /// Shows a press on `ability` refused for `reason`: its slot flashes,
    /// and the reason's word rises over the player as the refusal sounds.
    pub fn refuse(&mut self, ability: AbilityType, reason: AbilityFailReason) {
        let now = Moment::ZERO + self.gate.time.elapsed();
        for (slot, held) in &self.slots {
            if held.ability == Some(ability) {
                self.commands.entity(slot).try_insert(Refused(now));
            }
        }
        if self.said.is_some_and(|said| now.since(said).as_secs_f32() < WORD_GAP) {
            return;
        }
        *self.said = Some(now);
        self.commands.write_message(crate::plugins::sfx::Play::refused());
        if let Ok((_, body)) = self.player.single() {
            struck::float(&mut self.commands, skill_text::word(reason), struck::REFUSED, body, now);
        }
    }
}

/// What a slot shows of its skill: the gate's own answer, ready or why not
#[derive(Clone, Copy, Debug, PartialEq)]
struct AbilityState(Result<(), AbilityFailReason>);

/// The three ways a slot is drawn
#[derive(Debug, PartialEq)]
enum Look {
    Lit,
    /// Nothing its holder does by moving or turning changes it: held,
    /// busy, pinned, recharging, too dear
    Greyed,
    /// Its target is what refuses it: none, too far, or outside the arc
    Reaching,
}

impl AbilityState {
    fn look(&self) -> Look {
        match self.0 {
            Ok(()) => Look::Lit,
            Err(AbilityFailReason::NoTargets | AbilityFailReason::OutOfRange | AbilityFailReason::NotFacing) => Look::Reaching,
            Err(_) => Look::Greyed,
        }
    }

    fn border(&self) -> Color {
        match self.look() {
            Look::Lit => READY,
            Look::Greyed => Color::srgb(0.5, 0.5, 0.5),
            Look::Reaching => Color::srgb(0.8, 0.5, 0.1),
        }
    }

    /// What its icon is drawn through: greyed, or dim while reaching
    fn tint(&self) -> Color {
        match self.look() {
            Look::Lit => Color::WHITE,
            Look::Greyed => Color::srgb(0.35, 0.35, 0.35),
            Look::Reaching => Color::srgb(0.6, 0.6, 0.6),
        }
    }
}

/// The share of `ability`'s recharge still to run at `now`, 0 once ready
fn recharge_left(recharges: &Recharges, ability: AbilityType, now: Moment) -> f32 {
    recharges.of(ability)
        .filter(|recharge| !recharge.length.is_zero())
        .map_or(0.0, |recharge| (recharge.left(now).as_secs_f32() / recharge.length.as_secs_f32()).clamp(0.0, 1.0))
}

/// Shows each slot's skill as the gate would answer the viewed actor for
/// it now: lit where it would be used; lit from the bottom as its recharge
/// runs; greyed while its holder is held or busy, while it recharges,
/// costs more endurance than its holder has, or is a move and its holder
/// pinned; orange-bordered and dim while its target is none, out of reach
/// or outside the arc. A slot whose press was refused flashes its border.
pub fn update(
    mut commands: Commands,
    gate: Gate,
    mut slot_query: Query<(Entity, &AbilitySlot, &mut BorderColor, &Children, Option<&Refused>)>,
    mut overlay_query: Query<&mut Node, With<CooldownOverlay>>,
    mut icon_query: Query<&mut ImageNode, With<SlotIcon>>,
    viewed: Query<Stands, With<crate::components::Viewed>>,
) {
    let Ok(stands) = viewed.single() else {
        return;
    };
    let now = gate.now();
    let clock = Moment::ZERO + gate.time.elapsed();

    for (ent, slot, mut border_color, children, refused) in &mut slot_query {
        let (state, left) = match slot.ability {
            None => (None, 0.0),
            Some(ability) => (
                Some(AbilityState(gate.asks(&stands, ability))),
                stands.recharges.map_or(0.0, |recharges| recharge_left(recharges, ability, now)),
            ),
        };
        let flashing = refused.is_some_and(|refused| clock.since(refused.0).as_secs_f32() < REFUSED_FLASH);
        if refused.is_some() && !flashing {
            commands.entity(ent).remove::<Refused>();
        }
        let border = BorderColor::all(if flashing { FLASHED } else { state.as_ref().map_or(EMPTY, AbilityState::border) });
        if *border_color != border {
            *border_color = border;
        }
        for child in children.iter() {
            if let Ok(mut node) = overlay_query.get_mut(child) {
                node.height = Val::Percent(left * 100.0);
            }
            if let (Ok(mut icon), Some(state)) = (icon_query.get_mut(child), &state) {
                if icon.color != state.tint() {
                    icon.color = state.tint();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use common_bevy::components::activation::{Activation, Recharge};

    /// The tile ahead of an actor at the origin heading north, the one
    /// behind it, and one ahead beyond any reach
    const AHEAD: (i32, i32) = (0, -1);
    const BEHIND: (i32, i32) = (0, 1);
    const FAR: (i32, i32) = (0, -9);

    /// What a slot holding `ability` shows for the client's own character
    /// standing free at the origin, heading north, after `change`, with
    /// its target at `target`
    fn shown(ability: AbilityType, target: Option<(i32, i32)>, change: impl FnOnce(&mut EntityWorldMut)) -> AbilityState {
        let mut world = World::new();
        let mut actor = world.spawn((
            Loc::from_qrz(0, 0, 0), Heading::NORTH, Status::default(), Activating(None), Recharges::default(),
            Endurance::full(1_000.0), Actor,
        ));
        change(&mut actor);
        let ent = actor.id();
        let target = target.map(|(q, r)| Loc::from_qrz(q, r, 0));
        let mut query = world.query::<Stands>();
        let stands = query.get(&world, ent).unwrap();
        AbilityState(stands.asks(&Tuning::DEFAULT, ability, Moment::from_millis(1_000), target.as_ref()))
    }

    fn refused(reason: AbilityFailReason) -> AbilityState {
        AbilityState(Err(reason))
    }

    #[test]
    fn a_slot_shows_the_gates_own_answer_for_each_reason() {
        let strike = AbilityType::Frenzy;
        assert_eq!(shown(strike, Some(AHEAD), |_| {}), AbilityState(Ok(())));
        assert_eq!(shown(strike, Some(AHEAD), |actor| { actor.get_mut::<Status>().unwrap().hold(1.0); }), refused(AbilityFailReason::Held));
        let busy = Activating(Some(Activation { ability: AbilityType::Overpower, target: None, began: Moment::ZERO, length: Duration::from_secs(2) }));
        assert_eq!(shown(strike, Some(AHEAD), |actor| { actor.insert(busy); }), refused(AbilityFailReason::Activating));
        let recharging = |actor: &mut EntityWorldMut| {
            actor.get_mut::<Recharges>().unwrap().start(strike, Recharge { began: Moment::ZERO, length: Duration::from_secs(5) });
        };
        assert_eq!(shown(strike, Some(AHEAD), recharging), refused(AbilityFailReason::Recharging));
        assert_eq!(shown(strike, Some(AHEAD), |actor| { actor.insert(Endurance { state: 0.0, max: 100.0 }); }), refused(AbilityFailReason::NoEndurance));
        assert_eq!(shown(AbilityType::Leap, Some(AHEAD), |actor| { actor.get_mut::<Status>().unwrap().pin(1.0); }), refused(AbilityFailReason::Pinned));
        assert_eq!(shown(strike, None, |_| {}), refused(AbilityFailReason::NoTargets));
        assert_eq!(shown(strike, Some(FAR), |_| {}), refused(AbilityFailReason::OutOfRange));
        assert_eq!(shown(strike, Some(BEHIND), |_| {}), refused(AbilityFailReason::NotFacing));
        assert_eq!(shown(strike, Some(FAR), recharging), refused(AbilityFailReason::Recharging), "its recharge before its range");
    }

    #[test]
    fn a_stance_stays_lit_mid_activation() {
        let busy = Activating(Some(Activation { ability: AbilityType::Overpower, target: None, began: Moment::ZERO, length: Duration::from_secs(2) }));
        assert_eq!(shown(AbilityType::Brace, None, |actor| { actor.insert(busy); }).look(), Look::Lit);
    }

    #[test]
    fn an_actor_viewed_and_not_controlled_is_never_shown_out_of_reach() {
        assert_eq!(shown(AbilityType::Frenzy, None, |actor| { actor.remove::<Actor>(); }), AbilityState(Ok(())));
    }

    #[test]
    fn every_refusal_is_drawn_apart_from_ready_and_not_facing_as_out_of_range() {
        let reasons = [
            AbilityFailReason::NoTargets, AbilityFailReason::Held, AbilityFailReason::Activating, AbilityFailReason::Recharging,
            AbilityFailReason::NoEndurance, AbilityFailReason::OutOfRange, AbilityFailReason::NotFacing, AbilityFailReason::NotHeld,
            AbilityFailReason::Pinned, AbilityFailReason::NoRoom,
        ];
        let ready = AbilityState(Ok(()));
        for reason in reasons {
            let state = refused(reason);
            assert!(state.border() != ready.border() && state.tint() != ready.tint(), "{reason:?} shows in its border and its dim");
        }
        assert_eq!(refused(AbilityFailReason::NotFacing).look(), refused(AbilityFailReason::OutOfRange).look());
        assert_eq!(refused(AbilityFailReason::Held).look(), refused(AbilityFailReason::Recharging).look());
    }

    #[test]
    fn a_recharge_runs_down_for_its_own_skill_alone() {
        let at = Moment::from_millis;
        let mut recharges = Recharges::default();
        recharges.start(AbilityType::Frenzy, Recharge { began: at(1_000), length: Duration::from_secs(4) });
        let left = |ms| recharge_left(&recharges, AbilityType::Frenzy, at(ms));
        assert_eq!(left(1_000), 1.0);
        assert!(left(2_000) > left(3_000), "it runs down");
        assert_eq!(left(5_000), 0.0);
        assert_eq!(left(9_000), 0.0, "and stays run");
        assert_eq!(recharge_left(&recharges, AbilityType::Overpower, at(2_000)), 0.0, "no other skill with it");
    }
}

#[cfg(test)]
mod loadout_tests {
    use super::*;
    use common_bevy::{components::entity_type::actor::*, archetype::EnemyArchetype};

    fn npc(npc: EnemyArchetype) -> EntityType {
        EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Npc(npc)))
    }

    #[test]
    fn a_player_fills_every_key_from_its_own_bar() {
        let player = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player));
        assert_eq!(loadout(&player, None), Loadout::DEFAULT.0.map(Some));
        let held = Loadout::DEFAULT.placed(AbilityType::Lurk, 0);
        assert_eq!(loadout(&player, Some(&held)), held.0.map(Some));
    }

    #[test]
    fn an_npc_shows_its_own_skills_and_no_other() {
        for archetype in EnemyArchetype::ALL {
            let bar = loadout(&npc(archetype), None);
            assert!(archetype.bar().iter().all(|&skill| bar.contains(&Some(skill))), "{archetype:?}: every skill of its stands on the bar");
            assert_eq!(bar.iter().flatten().count(), archetype.bar().len(), "{archetype:?} shows only its own");
        }
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    /// A skill without its icon shows an empty slot; icongen draws one for
    /// every skill, any of which a player may put on its bar.
    #[test]
    fn every_skill_on_a_bar_has_its_icon() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for ability in AbilityType::ALL {
            let path = assets.join(icon_path(ability));
            assert!(path.exists(), "{ability:?} has no icon at {}", path.display());
        }
    }

    #[test]
    fn an_icon_is_named_in_kebab_case() {
        assert_eq!(icon_path(AbilityType::PerfectStride), "icons/skills/perfect-stride.png");
        assert_eq!(icon_path(AbilityType::Leap), "icons/skills/leap.png");
    }
}
