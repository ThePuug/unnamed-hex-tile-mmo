//! Building on the client: B opens the build panel on the tile the player
//! faces, which lists each kind the bag can pay for and the tile can take, or
//! the taking down of what stands there, and under them a map drawn of
//! the bag's fibre; a digit asks the server for it.
//! A row's key chooses it and Enter does it. Chosen, a thing to build is
//! described with what it takes, and its ghost stands where it would be
//! built, following the player as it walks and turns, until Enter builds
//! it there.
//! The server finds the tile itself, from where the player stands and
//! faces. On a hub's ground the panel also offers the fixtures a player
//! raises for it by hand, its hitching post and its first tents: the
//! server stands each on the nearest tile the hub takes it on.

use bevy::prelude::*;
use common::building::{self, Construct, Fixture, Kind};
use common_bevy::{
    components::{
        equipment::{Equipment, Inventory, Item, Slot},
        heading::Heading,
        position::Position,
        Actor,
    },
    message::{Do, Event, Try},
    resources::map::Map,
};
use qrz::Qrz;

use crate::systems::{
    equipment_panel::{CURSOR_ROW, OTHER_ROW, UNWORN},
    focus::{NumpadFocus, Panel},
    gathering::{ENTRY_KEYS, KEYCODE_CLOSE},
};

/// The key that opens the build panel, and shuts it.
pub const KEYCODE_BUILD: KeyCode = KeyCode::KeyB;

/// What a row of the build panel asks for: a construct built, the one
/// standing on the tile taken down, or a map drawn, blank or a copy of
/// the one in hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    Build(Construct),
    TakeDown(Construct),
    /// A fixture raised for the hub whose ground the player stands on.
    Raise(Fixture),
    Draw { copy: bool },
}

impl Choice {
    /// What it is called in its row.
    fn name(self) -> String {
        match self {
            Choice::Build(construct) => construct.name(),
            Choice::TakeDown(construct) => format!("Take down the {}", construct.name().to_lowercase()),
            Choice::Raise(fixture) => fixture.name().to_owned(),
            Choice::Draw { copy: false } => "Blank map".to_owned(),
            Choice::Draw { copy: true } => "Copy of the map in hand".to_owned(),
        }
    }

    /// Where its icon is: the thing built, as modelgen pictures it.
    fn icon(self) -> String {
        match self {
            Choice::Build(construct) | Choice::TakeDown(construct) => match construct.kind {
                Kind::Hearth => format!("icons/hearth-{}.png", construct.of.stem()),
                Kind::Blockade => "icons/blockade.png".to_owned(),
            },
            Choice::Raise(fixture) => format!("icons/{}.png", fixture.stem()),
            Choice::Draw { .. } => "icons/map-0.png".to_owned(),
        }
    }

    /// What it takes, a material to a stack: of a class of material, the
    /// one its icon stands for.
    fn takes(self) -> Vec<common::Stack> {
        let of = |material, count| common::Stack { kind: common::Stackable::Material(material), count };
        match self {
            Choice::Build(construct) => vec![construct.cost()],
            Choice::TakeDown(_) => Vec::new(),
            Choice::Raise(fixture) => {
                let needs = fixture.needs();
                [of(common::Material::Hide, needs.hide), of(common::Material::Softwood, needs.wood), of(common::Material::Sandstone, needs.stone)].into_iter().filter(|stack| stack.count > 0).collect()
            }
            Choice::Draw { .. } => vec![building::MAP_COST],
        }
    }

    /// What it takes, in words: a class of material by its name where any
    /// of the class does.
    fn ingredients(self) -> String {
        let listed: Vec<String> = match self {
            Choice::Raise(fixture) => {
                let needs = fixture.needs();
                [(needs.hide, "hide"), (needs.wood, "wood (any)"), (needs.stone, "stone (any)")].into_iter().filter(|&(count, _)| count > 0).map(|(count, of)| format!("{count} {of}")).collect()
            }
            _ => self.takes().iter().map(|stack| format!("{} {}", stack.count, stack.kind.name().to_lowercase())).collect(),
        };
        if listed.is_empty() { "Free".to_owned() } else { format!("Cost: {}", listed.join(", ")) }
    }

    /// Whether it is placed: built where its ghost stands.
    fn placed(self) -> bool {
        matches!(self, Choice::Build(_) | Choice::Raise(_))
    }
}

/// The build panel: whether it is open, and the rows it shows, a digit
/// to each.
#[derive(Resource, Default)]
pub struct BuildPanel {
    pub open: bool,
    choices: Vec<Choice>,
    /// The row chosen and not yet done, and for a thing to build the tile
    /// its ghost stands on: none where it would stand nowhere as the
    /// player stands now.
    placing: Option<(Choice, Option<(i32, i32)>)>,
}

impl BuildPanel {
    /// The tile the ghost of the thing chosen stands on, and the thing.
    pub fn ghost(&self) -> Option<((i32, i32), Choice)> {
        let (choice, tile) = self.placing.filter(|(choice, _)| choice.placed())?;
        Some((tile?, choice))
    }

    /// Whether a row is chosen.
    pub fn placing(&self) -> bool {
        self.placing.is_some()
    }

    /// What the panel offers, in the order of its rows.
    pub fn choices(&self) -> &[Choice] {
        &self.choices
    }
}

/// Where the local player returns when it falls, as the server last
/// said: the tile it stands on at its hearth, or none where that is the
/// haven.
#[derive(Resource, Default)]
pub struct HomeHearth(pub Option<Qrz>);

/// How long a claim waits for the key that confirms it, how long the
/// server has to answer one sent, and how long what came of it is shown.
const CONFIRM: std::time::Duration = std::time::Duration::from_secs(6);
const ANSWER: std::time::Duration = std::time::Duration::from_millis(1500);
const TOLD: std::time::Duration = std::time::Duration::from_secs(4);

/// Where the local player lives, as the server last said; the claim it
/// has asked for and not yet confirmed, the tile, what it was told and
/// when it asked; when a claim was sent that the server has not answered;
/// and what the player was last told of a claim, and when. All on the
/// real clock.
#[derive(Resource, Default)]
pub struct Claims {
    pub lives: Option<Qrz>,
    pending: Option<((i32, i32), &'static str, std::time::Duration)>,
    sent: Option<std::time::Duration>,
    told: Option<(&'static str, std::time::Duration)>,
}

impl Claims {
    /// Whether a claim of `tile` asked for at `now`, which `changes` what
    /// the player has by what [`changes`] says, is to be sent at once: it
    /// is where it changes nothing, and else is kept and shown until it
    /// is confirmed ([`Claims::confirm`]) or [`CONFIRM`] has passed. One
    /// sent waits for its answer.
    pub fn asked(&mut self, tile: (i32, i32), changes: Option<&'static str>, now: std::time::Duration) -> bool {
        self.pending = changes.map(|says| (tile, says, now));
        if self.pending.is_none() {
            self.sent = Some(now);
        }
        self.pending.is_none()
    }

    /// Whether a claim waits at `now` to be confirmed.
    pub fn waits(&self, now: std::time::Duration) -> bool {
        self.pending.is_some_and(|(.., asked)| now.saturating_sub(asked) <= CONFIRM)
    }

    /// The claim waiting at `now` is confirmed: the tile to send it for,
    /// none where none waits.
    pub fn confirm(&mut self, now: std::time::Duration) -> Option<(i32, i32)> {
        let (tile, ..) = self.pending.take().filter(|&(.., asked)| now.saturating_sub(asked) <= CONFIRM)?;
        self.sent = Some(now);
        Some(tile)
    }

    /// The server answered a claim sent: the player is told `says`.
    fn heard(&mut self, says: &'static str, now: std::time::Duration) {
        if self.sent.take().is_some() {
            self.told = Some((says, now));
        }
    }

    /// What the player is shown at `now`: what the claim waiting was
    /// told, or what came of the last one. One sent and not answered in
    /// [`ANSWER`] was refused.
    fn shown(&mut self, now: std::time::Duration) -> Option<&'static str> {
        if self.sent.is_some_and(|sent| now.saturating_sub(sent) > ANSWER) {
            self.sent = None;
            self.told = Some(("No room here", now));
        }
        let waiting = self.pending.filter(|&(.., asked)| now.saturating_sub(asked) <= CONFIRM).map(|(_, says, _)| says);
        waiting.or(self.told.filter(|&(_, at)| now.saturating_sub(at) <= TOLD).map(|(says, _)| says))
    }
}

/// What claiming slot `slot` of `cover` on `tile` would change of where a
/// player that returns to `returns` and lives at `lives` does either, as
/// the player is asked to confirm: none where it uses nothing there, or
/// the player has none yet, or has that one.
pub fn changes(cover: common::Cover, slot: usize, tile: (i32, i32), returns: Option<Qrz>, lives: Option<Qrz>) -> Option<&'static str> {
    if !building::used(cover, slot) {
        return None;
    }
    let (held, says) = match cover.content(slot) {
        common::cover::Content::Hearth(_) => (returns, "Set this hearth as your respawn point? Enter to confirm"),
        _ => (lives, "Move your home here? Enter to confirm"),
    };
    held.filter(|held| (held.q, held.r) != tile).map(|_| says)
}

/// Whether the hearth or tent in slot `slot` of `cover` on `tile` is the
/// one the player already has: the hearth it returns to, or the home it
/// lives in.
pub fn claimed(cover: common::Cover, slot: usize, tile: (i32, i32), returns: Option<Qrz>, lives: Option<Qrz>) -> bool {
    let held = if matches!(cover.content(slot), common::cover::Content::Hearth(_)) { returns } else { lives };
    held.is_some_and(|held| (held.q, held.r) == tile)
}

/// The line a claim waiting to be confirmed is shown as.
#[derive(Component)]
pub struct ClaimLine;

/// Shows what the claim waiting for its second press was told, while it
/// waits, and for a while what came of the last claim sent.
pub fn update_claim(mut commands: Commands, mut claims: ResMut<Claims>, time: Res<Time<Real>>, mut line: Query<(&mut Node, &mut Text), With<ClaimLine>>) {
    let says = claims.bypass_change_detection().shown(time.elapsed());
    match line.single_mut() {
        Ok((mut node, mut text)) => {
            let display = if says.is_some() { Display::Flex } else { Display::None };
            if node.display != display {
                node.display = display;
            }
            if let Some(says) = says.filter(|says| text.0 != *says) {
                text.0 = says.to_owned();
            }
        }
        Err(_) => {
            let Some(says) = says else { return };
            commands.spawn((
                ClaimLine,
                Node { position_type: PositionType::Absolute, top: Val::Percent(38.), left: Val::Percent(60.), padding: UiRect::all(Val::Px(10.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
                Text::new(says),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
            ));
        }
    }
}

/// The tile a player at `position` facing `heading` builds on: the one
/// its heading is nearest, as the server finds it.
pub fn faced(map: &Map, position: &Position, heading: Heading) -> Qrz {
    position.reached(map) + heading.hex_dir()
}

/// What the build panel offers on a tile with `cover` to a player whose
/// bag is `bag`: the taking down of the construct standing there, or else
/// each kind the tile can take, once, in the first of its materials the
/// bag pays for it whole.
pub fn choices(cover: common::Cover, bag: &Inventory) -> Vec<Choice> {
    if let Some(construct) = building::standing(cover) {
        return vec![Choice::TakeDown(construct)];
    }
    let pays = |construct: &Construct| construct.raised(cover).is_some() && bag.count(construct.cost().kind) >= construct.cost().count;
    Kind::ALL.into_iter().filter_map(|kind| Construct::all().filter(|construct| construct.kind == kind).find(pays)).map(Choice::Build).collect()
}

/// The fixtures a player whose bag is `bag` may raise by hand for the hub
/// whose ground it stands on, each one the bag pays for whole. Whether the
/// hub takes one is the server's to say.
pub fn raised(bag: &Inventory) -> Vec<Choice> {
    [Fixture::Post, Fixture::Tent].into_iter().filter(|fixture| fixture.needs().taken(&bag.stock).is_some()).map(Choice::Raise).collect()
}

/// The maps a player whose bag is `bag` may draw, wearing `worn`: a blank
/// one where the bag holds the fibre, and a copy where a map is in hand.
pub fn drawn(bag: &Inventory, worn: &Equipment) -> Vec<Choice> {
    let cost = building::MAP_COST;
    if bag.count(cost.kind) < cost.count {
        return Vec::new();
    }
    let holds = worn.worn(Slot::MainHand).and_then(Item::map).is_some();
    [false, true].into_iter().filter(|&copy| !copy || holds).map(|copy| Choice::Draw { copy }).collect()
}

/// Opens and shuts the panel on B, keeps its rows to the tile faced and
/// the bag, and asks the server for the row whose digit is pressed.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    map: Res<Map>,
    mut panel: ResMut<BuildPanel>,
    player: Query<(Entity, &Position, &Heading, &Inventory, &Equipment), With<Actor>>,
    mut writer: MessageWriter<Try>,
    mut settling: ResMut<crate::systems::input::Settling>,
    predicted: crate::systems::movement::Predicted,
    sheltered: Res<Sheltered>,
    mut ghosts: ResMut<Ghosts>,
    settings: Res<common_bevy::settings::Settings>,
) {
    let Ok((ent, position, heading, bag, worn)) = player.single() else {
        panel.open = false;
        return;
    };
    if menu.open {
        return;
    }
    if keys.pressed(KEYCODE_BUILD, "Open the build panel on the tile you face, or shut it") {
        panel.open = !panel.open;
    }
    if !panel.open {
        ghosts.asked = None;
        if panel.placing.is_some() {
            panel.placing = None;
        }
        return;
    }
    let tile = faced(&map, &predicted.position().unwrap_or(*position), *heading);
    // With materials provided the panel offers what a full bag would, and the server gives what is chosen
    let mut full = bag.clone();
    if settings.get(common::settings::Setting::FreeMaterials) != 0 {
        for (material, count) in [(common::Material::Hide, 4), (common::Material::Softwood, 6), (common::Material::Sandstone, 6)] {
            full.add(common::Stack { kind: common::Stackable::Material(material), count });
        }
        full.add(building::MAP_COST);
    }
    let bag = &full;
    let mut offered = choices(map.cover_at(tile.q, tile.r), bag);
    if sheltered.0 {
        // Where each would stand is asked of the server as the tile faced changes
        if ghosts.asked != Some((tile.q, tile.r)) {
            ghosts.asked = Some((tile.q, tile.r));
            for fixture in [Fixture::Post, Fixture::Tent] {
                writer.write(Try { event: Event::Sited { ent, fixture, q: tile.q, r: tile.r, stands: false } });
            }
        }
        offered.extend(raised(bag).into_iter().filter(|choice| matches!(choice, Choice::Raise(fixture) if ghosts.site(*fixture).is_some())));
    } else if ghosts.asked.take().is_some() {
        ghosts.sites.clear();
    }
    offered.extend(drawn(bag, worn));
    // The row chosen stays while the panel offers it. A thing to build
    // stands where it would be built from here: a construct on the tile
    // faced while that takes it, a hub's fixture where the server last said
    if let Some((choice, stood)) = panel.placing {
        let stands = match choice {
            Choice::Raise(fixture) => ghosts.site(fixture),
            _ => offered.contains(&choice).then_some((tile.q, tile.r)),
        };
        if !choice.placed() && stands.is_none() {
            panel.placing = None;
        } else if stands != stood {
            panel.placing = Some((choice, stands));
        }
    }
    if panel.choices != offered {
        panel.choices = offered;
    }
    if !focus.has(Panel::Build) {
        return;
    }
    if let Some((choice, stood)) = panel.placing {
        if keys.take(KEYCODE_CLOSE, "Let the row chosen go") {
            panel.placing = None;
            return;
        }
        // A thing raised from further off, the server steps the player beside it, as it does one to what it gathers
        let accepted = stood.is_some() && keys.take(KEYCODE_PLACE, "Do the row chosen: build the thing where its ghost stands, take down, or draw");
        if let (true, Some(at)) = (accepted, stood) {
            let event = match choice {
                Choice::Build(construct) => Event::Build { ent, construct: Some(construct) },
                Choice::Raise(fixture) => Event::Raise { ent, fixture, q: at.0, r: at.1 },
                Choice::TakeDown(_) => Event::Build { ent, construct: None },
                Choice::Draw { copy } => Event::Draw { ent, copy },
            };
            if !matches!(event, Event::Draw { .. }) {
                settling.begin();
            }
            writer.write(Try { event });
            panel.placing = None;
            panel.open = false;
            return;
        }
    }
    if panel.placing.is_none() && keys.take(KEYCODE_CLOSE, "Close the build panel") {
        panel.open = false;
        return;
    }
    for (row, key) in ENTRY_KEYS.iter().enumerate().take(panel.choices.len()) {
        if keys.take(*key, "Choose this row") {
            panel.placing = Some((panel.choices[row], None));
        }
    }
}

/// Where each fixture the local player may raise by hand would stand,
/// as the server last said of the tile last asked about.
#[derive(Resource, Default)]
pub struct Ghosts {
    asked: Option<(i32, i32)>,
    sites: Vec<(Fixture, (i32, i32))>,
}

impl Ghosts {
    /// Where `fixture` would stand.
    pub fn site(&self, fixture: Fixture) -> Option<(i32, i32)> {
        self.sites.iter().find(|(of, _)| *of == fixture).map(|&(_, at)| at)
    }

    /// Where the first fixture `choices` offers would stand.
    pub fn shown(&self, choices: &[Choice]) -> Option<(i32, i32)> {
        choices.iter().find_map(|choice| match choice {
            Choice::Raise(fixture) => self.site(*fixture),
            _ => None,
        })
    }
}

/// Keeps where each fixture would stand as the server says.
pub fn do_sited(mut reader: MessageReader<Do>, mut ghosts: ResMut<Ghosts>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Sited { ent, fixture, q, r, stands } } = message else { continue };
        if !player.contains(*ent) {
            continue;
        }
        ghosts.sites.retain(|(of, _)| of != fixture);
        if *stands {
            ghosts.sites.push((*fixture, (*q, *r)));
        }
    }
}

/// Whether the local player stands on ground a hub holds, as the server
/// last said.
#[derive(Resource, Default)]
pub struct Sheltered(pub bool);

/// Keeps whether the local player stands on a hub's ground as the server
/// says.
pub fn do_sheltered(mut reader: MessageReader<Do>, mut sheltered: ResMut<Sheltered>, player: Query<(), With<Actor>>) {
    for message in reader.read() {
        let Do { event: Event::Sheltered { ent, held } } = message else { continue };
        if player.contains(*ent) && sheltered.0 != *held {
            sheltered.0 = *held;
        }
    }
}

/// Keeps where the local player returns, and where it lives, as the
/// server says.
pub fn do_home(mut reader: MessageReader<Do>, mut home: ResMut<HomeHearth>, mut claims: ResMut<Claims>, player: Query<(), With<Actor>>, time: Res<Time<Real>>) {
    for message in reader.read() {
        match message {
            Do { event: Event::Home { ent, home: at } } if player.contains(*ent) => {
                home.0 = *at;
                claims.heard("Respawn point set", time.elapsed());
                info!("home: returns to {}", at.map_or("the haven".to_owned(), |at| format!("the hearth on ({}, {})", at.q, at.r)));
            }
            Do { event: Event::Lives { ent, home: at } } if player.contains(*ent) => {
                claims.lives = *at;
                claims.heard("Home set", time.elapsed());
                info!("home: lives {}", at.map_or("nowhere".to_owned(), |at| format!("on ({}, {})", at.q, at.r)));
            }
            _ => {}
        }
    }
}

/// The key that builds the thing being placed where its ghost stands.
const KEYCODE_PLACE: KeyCode = KeyCode::NumpadEnter;

/// The ghost of the thing being placed, drawn as it would stand.
#[derive(Component)]
pub struct GhostStand;

/// Draws the thing being placed where it would be built, as its model
/// stands, and takes it away when it is built or put away.
#[allow(clippy::too_many_arguments)]
pub fn draw_ghost(
    mut commands: Commands,
    panel: Res<BuildPanel>,
    map: Res<Map>,
    origin: Res<crate::resources::RenderOrigin>,
    kit: Option<Res<crate::plugins::cover::CoverKit>>,
    predicted: crate::systems::movement::Predicted,
    mut stands: Query<(Entity, &mut Transform), With<GhostStand>>,
    mut shown: Local<Option<((i32, i32), Choice)>>,
) {
    use crate::plugins::cover::{CoverInstance, Kind};
    let ghost = panel.ghost();
    let (Some(((q, r), choice)), Some(kit), Some(position)) = (ghost, kit, predicted.position()) else {
        for (stand, _) in &stands {
            commands.entity(stand).despawn();
        }
        *shown = None;
        return;
    };
    let Some((tile, _)) = map.get_by_qr(q, r) else { return };
    // In the frame of the player's tile, as the tile lit under it is (`aim::mark`)
    let at = Vec2::from(common_bevy::geometry::flat_top_tile_center(tile.q - position.tile.q, tile.r - position.tile.r, 1.0));
    let rise = common_bevy::systems::movement::surface_y_from(position.tile, at, tile, &map);
    let foot = origin.render_tile(&map, position.tile) + Vec3::new(at.x, rise, at.y);
    let stand = match stands.single_mut() {
        Ok((stand, mut transform)) => {
            if transform.translation != foot {
                transform.translation = foot;
            }
            stand
        }
        Err(_) => commands.spawn((GhostStand, Transform::from_translation(foot), Visibility::default())).id(),
    };
    if *shown != ghost {
        let kind = match choice {
            Choice::Build(construct) => crate::plugins::cover::built(construct),
            Choice::Raise(fixture) => Kind::Raised(fixture, 0),
            Choice::TakeDown(_) | Choice::Draw { .. } => return,
        };
        let sway = common::cover::boulder_sway(q, r, building::SLOT);
        let yaw = if matches!(choice, Choice::Raise(_)) { sway.yaw as f32 } else { 0.0 };
        let instance = CoverInstance { translation: Vec3::ZERO, yaw, tilt: Quat::IDENTITY, growth: 1.0, kind, variation: 0, far: false };
        crate::plugins::cover::spawn_models(&mut commands, stand, &[instance], &kit);
        *shown = ghost;
    }
}

/// The panel's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct BuildRows;

/// Shows the panel while it is open and lays its rows out again whenever
/// what it offers changes.
pub fn update(mut commands: Commands, panel: Res<BuildPanel>, mut rows: Query<(Entity, &mut Node), With<BuildRows>>, asset_server: Res<AssetServer>) {
    if !panel.is_changed() {
        return;
    }
    let root = match rows.single_mut() {
        Ok((root, mut node)) => {
            node.display = if panel.open { Display::Flex } else { Display::None };
            commands.entity(root).despawn_related::<Children>();
            root
        }
        Err(_) if !panel.open => return,
        Err(_) => commands
            .spawn((
                BuildRows,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Percent(45.),
                    left: Val::Percent(60.),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.),
                    padding: UiRect::all(Val::Px(10.)),
                    border_radius: BorderRadius::all(Val::Px(6.)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
            ))
            .id(),
    };
    if !panel.open {
        return;
    }
    let line = |text: String| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.85, 0.85)));
    let icon = |path: String, side: f32| (ImageNode::new(asset_server.load(path)), Node { width: Val::Px(side), height: Val::Px(side), ..default() });
    let chosen = panel.placing;
    commands.entity(root).with_children(|list| {
        use crate::systems::keycap::{hint_row, Hint};
        if panel.choices.is_empty() {
            list.spawn(line("Nothing you can build here".to_owned()));
        }
        // A row to a choice: its key, its icon and name, and what it takes as the bag shows a stack
        for (row, choice) in panel.choices.iter().enumerate() {
            let lit = chosen.is_some_and(|(of, _)| of == *choice);
            list.spawn((
                Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), padding: UiRect::axes(Val::Px(8.), Val::Px(4.)), border: UiRect::all(Val::Px(1.)), border_radius: BorderRadius::all(Val::Px(4.)), ..default() },
                BackgroundColor(if lit { CURSOR_ROW } else { OTHER_ROW }),
                BorderColor::all(if lit { Color::srgb(0.95, 0.8, 0.3) } else { UNWORN }),
            ))
            .with_children(|cell| {
                crate::systems::keycap::keycap(cell, crate::caps!(ENTRY_KEYS)[row]);
                cell.spawn(icon(choice.icon(), ROW_ICON));
                cell.spawn((line(choice.name()), Node { flex_grow: 1.0, ..default() }));
                for stack in choice.takes() {
                    let common::Stackable::Material(material) = stack.kind;
                    cell.spawn(icon(format!("icons/{}.png", material.stem()), ROW_ICON));
                    cell.spawn(line(stack.count.to_string()));
                }
            });
        }
        // The row chosen, described, with what it takes written out
        match chosen {
            Some((choice, stood)) => {
                list.spawn(line(choice.ingredients()));
                let does = match choice {
                    Choice::Build(_) | Choice::Raise(_) if stood.is_none() => "can't be placed here",
                    Choice::Build(_) | Choice::Raise(_) => "build",
                    Choice::TakeDown(_) => "take down",
                    Choice::Draw { .. } => "draw",
                };
                hint_row(list, None, &[Hint::key(crate::cap!(KEYCODE_PLACE), does), Hint::key(crate::cap!(KEYCODE_CLOSE), "back")]);
            }
            None => hint_row(list, None, &[Hint::range(&crate::caps!(ENTRY_KEYS), "choose"), Hint::key(crate::cap!(KEYCODE_CLOSE), "close")]),
        }
    });
}

/// How large a row's icons are drawn.
const ROW_ICON: f32 = 28.0;

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Cover, Material, Stack, Stackable};

    /// A first claim is sent at once; one in place of another waits for a
    /// second G on the same tile, in time.
    #[test]
    fn a_claim_in_place_of_another_is_asked_for_twice() {
        let secs = std::time::Duration::from_secs;
        let mut claims = Claims::default();
        assert!(claims.asked((1, 1), None, secs(0)), "what changes nothing is sent at once");
        assert!(!claims.asked((1, 1), Some("again"), secs(1)));
        assert_eq!(claims.shown(secs(2)), Some("again"));
        assert!(!claims.asked((2, 2), Some("again"), secs(2)), "asked again, it waits again, for the tile last asked");
        assert_eq!((claims.waits(secs(3)), claims.confirm(secs(3)), claims.confirm(secs(3))), (true, Some((2, 2)), None));
        assert!(!claims.asked((2, 2), Some("again"), secs(4)));
        assert_eq!(claims.confirm(secs(4) + CONFIRM + secs(1)), None, "one confirmed too late is let go");
        assert_eq!(changes(Cover::NONE, building::SLOT, (0, 0), Some(Qrz { q: 5, r: 5, z: 0 }), None), None, "nothing is used on bare ground");
    }

    /// The panel offers what the bag pays for and the tile takes, a kind
    /// once whatever the bag holds to build it of, and on a tile holding
    /// a construct only its taking down.
    #[test]
    fn the_panel_offers_what_the_bag_and_the_tile_allow() {
        let mut bag = Inventory::default();
        assert!(choices(Cover::NONE, &bag).is_empty());
        bag.add(Stack { kind: Stackable::Material(Material::Hardwood), count: 4 });
        let blockade = Construct { kind: Kind::Blockade, of: Material::Hardwood };
        assert_eq!(choices(Cover::NONE, &bag), vec![Choice::Build(blockade)]);
        bag.add(Stack { kind: Stackable::Material(Material::Softwood), count: 4 });
        assert_eq!(choices(Cover::NONE, &bag).len(), 1, "one blockade, of either wood");
        let tree = Cover::NONE.with(0, common::Content::Pine);
        assert!(choices(tree, &bag).is_empty(), "a blockade needs the whole tile");
        bag.add(Stack { kind: Stackable::Material(Material::Basement), count: 6 });
        let hearth = Construct { kind: Kind::Hearth, of: Material::Basement };
        assert!(choices(tree, &bag).is_empty(), "as does a hearth");
        assert_eq!(choices(Cover::NONE, &bag).first(), Some(&Choice::Build(hearth)));
        assert_eq!(choices(Cover::NONE, &bag).len(), Kind::ALL.len());
        let staked = blockade.raised(Cover::NONE).unwrap();
        assert_eq!(choices(staked, &bag), vec![Choice::TakeDown(blockade)]);
    }

    /// A fixture is offered once the bag holds all it takes.
    #[test]
    fn a_fixture_is_offered_for_what_it_takes() {
        let mut bag = Inventory::default();
        assert!(raised(&bag).is_empty());
        bag.add(Stack { kind: Stackable::Material(Material::Softwood), count: 6 });
        assert_eq!(raised(&bag), vec![Choice::Raise(Fixture::Post)], "a tent takes hide besides");
        bag.add(Stack { kind: Stackable::Material(Material::Hide), count: 4 });
        assert_eq!(raised(&bag), vec![Choice::Raise(Fixture::Post), Choice::Raise(Fixture::Tent)]);
    }

    /// A blank map is offered once the bag holds its fibre.
    #[test]
    fn a_map_is_offered_for_its_fibre() {
        let mut bag = Inventory::default();
        let cost = building::MAP_COST;
        bag.add(Stack { kind: cost.kind, count: cost.count - 1 });
        assert!(drawn(&bag, &Equipment::default()).is_empty());
        bag.add(Stack { kind: cost.kind, count: 1 });
        assert_eq!(drawn(&bag, &Equipment::default()), vec![Choice::Draw { copy: false }], "and no copy with no map in hand");
    }
}
