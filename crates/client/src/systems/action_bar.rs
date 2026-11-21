use bevy::prelude::*;

use common_bevy::{
    components::{activation::Recharges, entity_type::EntityType, loadout::Loadout, resources::Endurance, target::Target, Actor, ActorAttributes, AttackRange, Loc},
    message::AbilityType,
    moment::Moment,
};
use common_bevy::tuning::Tuning;

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
        for (keybind, ability) in KEYS.into_iter().zip(loadout(typ, held)) {
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
pub fn slot_frame<'a>(parent: &'a mut ChildSpawnerCommands, keybind: KeyCode, icon: Option<Handle<Image>>, side: f32, named: bool) -> EntityCommands<'a> {
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

fn spawn_slot(parent: &mut ChildSpawnerCommands, keybind: KeyCode, ability: Option<AbilityType>, icon: Option<Handle<Image>>) {
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

/// What a slot shows of its skill
#[derive(Debug, PartialEq)]
enum AbilityState {
    Ready,
    /// Recharging, or costing more endurance than its holder has
    Unusable,
    /// A strike whose target stands beyond its reach, or with none
    OutOfRange,
}

impl AbilityState {
    /// The gate's order: a skill it would refuse for its recharge or its
    /// price shows that, whatever its range.
    fn of(recharged: bool, afforded: bool, in_range: bool) -> AbilityState {
        if !recharged || !afforded {
            AbilityState::Unusable
        } else if !in_range {
            AbilityState::OutOfRange
        } else {
            AbilityState::Ready
        }
    }

    fn border(&self) -> Color {
        match self {
            AbilityState::Ready => READY,
            AbilityState::Unusable => Color::srgb(0.5, 0.5, 0.5),
            AbilityState::OutOfRange => Color::srgb(0.8, 0.5, 0.1),
        }
    }

    /// What its icon is drawn through: greyed where unusable, dim out of
    /// range
    fn tint(&self) -> Color {
        match self {
            AbilityState::Ready => Color::WHITE,
            AbilityState::Unusable => Color::srgb(0.35, 0.35, 0.35),
            AbilityState::OutOfRange => Color::srgb(0.6, 0.6, 0.6),
        }
    }
}

/// The share of `ability`'s recharge still to run at `now`, 0 once ready
fn recharge_left(recharges: &Recharges, ability: AbilityType, now: Moment) -> f32 {
    recharges.of(ability)
        .filter(|recharge| !recharge.length.is_zero())
        .map_or(0.0, |recharge| (recharge.left(now).as_secs_f32() / recharge.length.as_secs_f32()).clamp(0.0, 1.0))
}

/// Shows each slot's skill as the viewed actor has it: lit from the
/// bottom as its recharge runs, greyed while it recharges or costs more
/// endurance than its holder has, orange-bordered and dim while a strike's
/// target is out of reach.
pub fn update(
    tuning: Res<Tuning>,
    mut slot_query: Query<(&AbilitySlot, &mut BorderColor, &Children)>,
    mut overlay_query: Query<&mut Node, With<CooldownOverlay>>,
    mut icon_query: Query<&mut ImageNode, With<SlotIcon>>,
    player_query: Query<(&Loc, Option<&Recharges>, Option<&Endurance>, Option<&AttackRange>, Option<&Target>, Has<Actor>, Option<&ActorAttributes>), With<crate::components::Viewed>>,
    locs: Query<&Loc>,
    server: Res<crate::resources::Server>,
    time: Res<Time>,
) {
    let Ok((loc, recharges, endurance, own_reach, target, controlled, attrs)) = player_query.single() else {
        return;
    };
    let now = server.now(time.elapsed().as_millis());
    let own_reach = own_reach.copied().unwrap_or_default().0;
    let attrs = attrs.copied().unwrap_or_default();
    // The target a press would name (`input::update_keybits`)
    let target = target.and_then(|target| target.entity).and_then(|target| locs.get(target).ok());

    for (slot, mut border_color, children) in &mut slot_query {
        let (state, left) = match slot.ability {
            None => (None, 0.0),
            Some(ability) => {
                let left = recharges.map_or(0.0, |recharges| recharge_left(recharges, ability, now));
                let afforded = endurance.is_none_or(|endurance| endurance.state >= tuning.cost(ability));
                // Range is judged for the client's own character, who picks
                // its targets here; a viewed actor's targets are the server's
                let in_range = !controlled || ability.reach(&tuning, &attrs, own_reach)
                    .is_none_or(|reach| target.is_some_and(|target| reach.contains(&loc.flat_distance(target))));
                (Some(AbilityState::of(left <= 0.0, afforded, in_range)), left)
            }
        };
        let border = BorderColor::all(state.as_ref().map_or(EMPTY, AbilityState::border));
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
    use common_bevy::components::activation::Recharge;

    #[test]
    fn a_slot_shows_what_the_gate_would_refuse_it_for_first() {
        assert_eq!(AbilityState::of(true, true, true), AbilityState::Ready);
        assert_eq!(AbilityState::of(false, true, true), AbilityState::Unusable, "recharging");
        assert_eq!(AbilityState::of(true, false, true), AbilityState::Unusable, "too dear");
        assert_eq!(AbilityState::of(true, true, false), AbilityState::OutOfRange);
        assert_eq!(AbilityState::of(false, true, false), AbilityState::Unusable, "its recharge before its range");
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
