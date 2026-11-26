//! Planning a hub: P, on a hub's ground, opens the hub seen from above,
//! its ground tinted by zone, what stands on it and what is planned, and
//! its wall. A cursor moves over it a tile at a time by the arrows, which
//! walk the player nowhere while the plan is open (`input::update_keybits`).
//! Each planner has a tab, plus and minus turning between them, and a
//! digit chooses one of the options of the tab shown ([`listed`]). One list holds every order the hub's planners take
//! at the cursor ([`actions`]): plus and minus choose one, the server
//! says what it would change, which is drawn over the hub, and Enter has
//! it done. The server holds the hub ([`Layout`]) and decides every plan;
//! this only asks and draws.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use common::building::{Fixture, Laid, Layout, Needs, Order, Planned, Wants};
use common_bevy::{
    components::{position::Position, Actor},
    message::{Do, Event, Try},
    resources::map::Map,
};

use crate::systems::{
    character_panel::{PANE, SHUT},
    equipment_panel::{CURSOR_ROW, OTHER_ROW, UNWORN},
    focus::{NumpadFocus, Panel},
    gathering::{ENTRY_KEYS, KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
    settlers::{aims, offers, Offer},
};

/// The key that opens the planning of the hub the player stands in, and
/// shuts it.
pub const KEYCODE_PLAN: KeyCode = KeyCode::KeyP;
const KEYCODE_NEXT: KeyCode = KeyCode::NumpadAdd;
const KEYCODE_BEFORE: KeyCode = KeyCode::NumpadSubtract;

/// The keys that move the cursor and the step each takes over the grid:
/// up and down a tile, and left and right along the one line of tiles
/// that runs up to the left and down to the right, so four keys reach
/// every tile.
const STEPS: [(KeyCode, (i32, i32)); 4] = [
    (KeyCode::ArrowUp, (0, -1)),
    (KeyCode::ArrowDown, (0, 1)),
    (KeyCode::ArrowLeft, (-1, 0)),
    (KeyCode::ArrowRight, (1, 0)),
];

/// How far a glyph is drawn left of where the font lays it, in px at the
/// size the panel draws them: the glyph font's icons are two cells wide
/// and say they are one, so one laid centred stands half a cell right.
const GLYPH_NUDGE: f32 = -4.0;

/// The colour the way in through the wall is drawn in.
const GATE: [f32; 3] = [0.95, 0.92, 0.70];

/// The colour the player's own tile is drawn in.
const PLAYER: [f32; 3] = [0.25, 0.85, 0.95];

/// How many texels a side the hub is drawn at, and how large on screen.
const SIDE: u32 = 384;
const SHOWN: f32 = 480.0;

/// What the server said of the action chosen.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Would(Planned),
    Refused,
}

/// The planning of a hub: whether it is open, the hub as the server last
/// sent it, the tile under the cursor, the action chosen and what the
/// server said it would do, and whether it was asked to do it.
#[derive(Resource, Default)]
pub struct Planning {
    open: bool,
    layout: Option<Layout>,
    cursor: (i32, i32),
    player: (i32, i32),
    chosen: usize,
    /// The planner whose tab is shown, by its zone's number; none until
    /// the hub is known, or where it has no planner.
    planner: Option<u8>,
    answer: Option<Answer>,
    carried: bool,
    /// The occupancy being set, while its box is open: how many the hub
    /// would be planned for.
    counting: Option<u32>,
    /// By how many the occupancy last set differs from what the hub is
    /// planned for: what the occupancy option asks for. None as 0.
    occupancy: i32,
    /// When the hub was asked for, on the real clock, and whether the
    /// server has been too long answering.
    asked: std::time::Duration,
    unanswered: bool,
}

impl Planning {
    pub fn open(&self) -> bool {
        self.open
    }
}

/// The numbers zones and things are kept by, as the server sends them.
const LOGISTICS: u8 = 1;
const HOME: u8 = 4;

/// The zone a thing kept as `what` is of.
fn zone_of(what: u8) -> u8 {
    match what {
        4 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        _ => LOGISTICS,
    }
}

/// The home on `tile`, as the fixture it stands as.
fn home_on(layout: &Layout, tile: (i32, i32)) -> Option<Fixture> {
    let laid = layout.items.iter().find(|laid| laid.tiles.contains(&tile))?;
    match (laid.what, laid.level) {
        (HOME, 1) => Some(Fixture::Tent),
        (HOME, 2) => Some(Fixture::Pavilion),
        (HOME, 3) => Some(Fixture::House),
        (HOME, 4) => Some(Fixture::Tenement),
        _ => None,
    }
}

/// How far apart two tiles are, in tiles.
fn apart(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (q, r) = (a.0 - b.0, a.1 - b.1);
    q.abs().max(r.abs()).max((q + r).abs())
}

/// The zone the ground on `tile` is of: the one whose centre is nearest.
fn zone_at(layout: &Layout, tile: (i32, i32)) -> u8 {
    layout.zones.iter().min_by_key(|&&(zone, q, r)| (apart(tile, (q, r)), zone)).map_or(0, |&(zone, ..)| zone)
}

/// The planners that have a tab, by their zones' numbers, in order.
pub fn tabs(layout: &Layout) -> Vec<u8> {
    (1..=6u8).filter(|zone| layout.hired & 1 << zone != 0).collect()
}

/// The options of `planner`'s tab with the cursor on `cursor`, and of
/// each whether the planner, at work, will not do it yet: its own orders
/// (`settlers::offers`), then the hiring of the hub's next planner where
/// it may have one, which is no one's to wait on. With no planner, the
/// hiring alone.
pub fn listed(layout: &Layout, planner: Option<u8>, cursor: (i32, i32)) -> Vec<(Offer, bool)> {
    let own = planner.into_iter().flat_map(|zone| {
        let waits = layout.busy & 1 << zone != 0;
        offers(Wants { zone, ..Wants::default() }, cursor, cursor, home_on(layout, cursor)).into_iter().map(move |row| (row, waits))
    });
    own.chain(offers(Wants { hires: layout.hires, ..Wants::default() }, cursor, cursor, None).into_iter().map(|row| (row, false))).collect()
}

/// Whether `order` has nothing to do with the cursor on `cursor`: an
/// upgrade or a downgrade, and no home there that has one larger or
/// smaller; the wall, already ordered; a wagon or a storehouse, where the
/// hub has one. Such an option is shown, and its key does nothing.
pub fn aimless(layout: &Layout, order: Order, cursor: (i32, i32)) -> bool {
    use crate::systems::settlers::{larger, smaller};
    let home = home_on(layout, cursor);
    match order {
        Order::Upgrade(..) => home.and_then(larger).is_none(),
        Order::Downgrade(..) => home.and_then(smaller).is_none(),
        Order::Wall => layout.walled,
        Order::Raise(Fixture::Wagon) => layout.items.iter().any(|laid| laid.what == 3),
        Order::Raise(Fixture::Storehouse) => layout.items.iter().any(|laid| laid.what == 5),
        _ => false,
    }
}

/// Opens and shuts the planning on P, moves its cursor, chooses its
/// action and asks the server after it, and has it done on Enter.
#[allow(clippy::too_many_arguments)]
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    mut planning: ResMut<Planning>,
    player: Query<(Entity, &Position), With<Actor>>,
    predicted: crate::systems::movement::Predicted,
    map: Res<Map>,
    time: Res<Time<Real>>,
    mut writer: MessageWriter<Try>,
) {
    let Ok((ent, position)) = player.single() else {
        if planning.open {
            planning.open = false;
        }
        return;
    };
    if menu.open {
        return;
    }
    let here = predicted.position().unwrap_or(*position).reached(&map);
    let here = (here.q, here.r);
    if keys.pressed(KEYCODE_PLAN, "Plan the hub you stand in, seen from above, or put the plan away") {
        planning.open = !planning.open;
        if planning.open {
            (planning.cursor, planning.player, planning.chosen, planning.answer, planning.carried) = (here, here, 0, None, false);
            (planning.planner, planning.chosen, planning.counting) = (None, usize::MAX, None);
            planning.layout = None;
            (planning.asked, planning.unanswered) = (time.elapsed(), false);
            writer.write(Try { event: Event::Layout { ent, layout: None } });
        }
    }
    // A server that knows nothing of planning never answers
    if planning.open && planning.layout.is_none() && !planning.unanswered && time.elapsed().saturating_sub(planning.asked) > std::time::Duration::from_secs(2) {
        planning.unanswered = true;
    }
    if !planning.open || !focus.has(Panel::Planning) {
        return;
    }
    if planning.player != here {
        planning.player = here;
    }
    // The one key backs out of whatever is innermost: the occupancy's box, then the option chosen, then the plan
    if planning.counting.is_some() {
        if keys.take(KEYCODE_CLOSE, "Leave the occupancy as it is") {
            planning.counting = None;
            return;
        }
    } else if planning.chosen != usize::MAX {
        if keys.take(KEYCODE_CLOSE, "Let the option chosen go") {
            (planning.chosen, planning.occupancy, planning.answer) = (usize::MAX, 0, None);
            return;
        }
    } else if keys.take(KEYCODE_CLOSE, "Put the plan away") {
        planning.open = false;
        return;
    }
    let Some(layout) = planning.layout.clone() else { return };
    // An occupancy being set has the keys: plus and minus move it, and
    // Enter shuts its box and shows what it would change
    if let Some(target) = planning.counting {
        use crate::systems::drop_panel::KEYCODE_CANCEL;
        if keys.take(KEYCODE_CANCEL, "Leave the occupancy as it is") {
            planning.counting = None;
            return;
        }
        let more = keys.take(KEYCODE_NEXT, "Plan for one more") as u32;
        let fewer = keys.take(KEYCODE_BEFORE, "Plan for one fewer") as u32;
        let now = (target + more).saturating_sub(fewer).max(1);
        planning.counting = Some(now);
        if keys.take(KEYCODE_TAKE_ALL, "Show what this occupancy would change") {
            let by = now as i32 - layout.wants as i32;
            (planning.counting, planning.occupancy, planning.answer) = (None, by, None);
            if by != 0 {
                writer.write(Try { event: Event::Plan { ent, order: Order::Residents(by), carry: false } });
            }
        }
        return;
    }
    let before = (planning.cursor, planning.chosen, planning.planner);
    for (key, (q, r)) in STEPS {
        if keys.take(key, "Move the cursor over the hub") {
            planning.cursor = (planning.cursor.0 + q, planning.cursor.1 + r);
        }
    }
    // The planners' tabs, one shown: the first until another is turned to
    let tabs = tabs(&layout);
    let at = planning.planner.and_then(|zone| tabs.iter().position(|&tab| tab == zone));
    // From the last tab the next key goes nowhere, nor the key before from the first
    let at = at.unwrap_or(0) + keys.take(KEYCODE_NEXT, "Turn to the next planner's tab") as usize;
    let at = at.saturating_sub(keys.take(KEYCODE_BEFORE, "Turn to the planner's tab before") as usize);
    let shown = tabs.get(at.min(tabs.len().saturating_sub(1))).copied();
    if planning.planner != shown {
        (planning.planner, planning.chosen, planning.answer) = (shown, usize::MAX, None);
    }
    let rows = listed(&layout, planning.planner, planning.cursor);
    // A digit chooses the option it numbers
    let mut pressed = false;
    for (row, key) in ENTRY_KEYS.iter().enumerate().take(rows.len()) {
        // One with nothing under the cursor to act on has no key
        if !aimless(&layout, rows[row].0.order, planning.cursor) && keys.take(*key, "Choose this option") {
            (planning.chosen, pressed) = (row, true);
        }
    }
    // The cursor moved off what the option chosen acted on: it is chosen no more
    if rows.get(planning.chosen).is_some_and(|(row, _)| aimless(&layout, row.order, planning.cursor)) {
        (planning.chosen, planning.answer) = (usize::MAX, None);
    }
    let Some((row, waits)) = rows.get(planning.chosen) else { return };
    let mut order = row.order;
    // The occupancy is set in its own box, from what the hub is planned for
    // now, each time its key is pressed; the option then asks for what was set
    if matches!(order, Order::Residents(_)) {
        if !waits && (pressed || planning.occupancy == 0 && keys.take(KEYCODE_TAKE_ALL, "Set the occupancy")) {
            (planning.counting, planning.occupancy, planning.answer) = (Some(layout.wants), 0, None);
        }
        if planning.counting.is_some() || planning.occupancy == 0 {
            return;
        }
        order = Order::Residents(planning.occupancy);
    }
    // Asked after anew as it is chosen, and as the cursor moves where it acts on the cursor
    let moved = before.0 != planning.cursor;
    if before.1 != planning.chosen || before.2 != planning.planner || (moved && aims(order)) {
        planning.answer = None;
        if !matches!(order, Order::Hire(_)) {
            writer.write(Try { event: Event::Plan { ent, order, carry: false } });
        }
    }
    if keys.take(KEYCODE_TAKE_ALL, "Have the option chosen done") && !waits && !matches!(planning.answer, Some(Answer::Refused)) {
        (planning.carried, planning.occupancy) = (true, 0);
        writer.write(Try { event: Event::Plan { ent, order, carry: true } });
    }
}

/// Keeps the hub and what the server says of the action chosen. A plan
/// done, the hub is asked for again as it stands then.
// The player's own actor is the one with a position: another marked an actor may have none yet
pub fn hear(mut reader: MessageReader<Do>, mut planning: ResMut<Planning>, player: Query<Entity, (With<Actor>, With<Position>)>, mut writer: MessageWriter<Try>) {
    let Ok(me) = player.single() else { return };
    for message in reader.read() {
        match message {
            Do { event: Event::Layout { ent, layout } } if *ent == me => {
                planning.layout = layout.clone();
                if planning.open && layout.is_none() {
                    info!("plan: no hub holds this ground");
                    planning.open = false;
                }
            }
            Do { event: Event::Planned { ent, plan } } if *ent == me && planning.open => match plan {
                // Asked to do it, it answered with what it still lacks: not done
                Some(plan) => (planning.answer, planning.carried) = (Some(Answer::Would(plan.clone())), false),
                None if std::mem::take(&mut planning.carried) => {
                    planning.answer = None;
                    writer.write(Try { event: Event::Layout { ent: me, layout: None } });
                }
                None => planning.answer = Some(Answer::Refused),
            },
            _ => {}
        }
    }
}

/// The colour a zone's ground and things are drawn in.
fn tint(zone: u8) -> [f32; 3] {
    match zone {
        1 => [0.55, 0.55, 0.62],
        2 => [0.80, 0.62, 0.34],
        3 => [0.30, 0.52, 0.82],
        4 => [0.85, 0.45, 0.22],
        5 => [0.36, 0.68, 0.32],
        6 => [0.62, 0.42, 0.74],
        _ => [0.35, 0.35, 0.35],
    }
}

/// The hub drawn from above, [`SIDE`] texels a side, as RGBA rows from
/// the north: its ground in its zone's colour, dim; what stands on it in
/// that colour, bright, and what is planned and not yet raised between
/// the two; each zone's centre, a ringed dot; its wall; over them what `answer` would change, raised green
/// and given up red; and the player and the cursor.
fn texels(layout: &Layout, answer: Option<&Answer>, cursor: (i32, i32), player: (i32, i32)) -> Vec<u8> {
    let mut colours: HashMap<(i32, i32), [f32; 3]> = HashMap::new();
    let shade = |of: [f32; 3], by: f32| [of[0] * by, of[1] * by, of[2] * by];
    for &tile in &layout.ground {
        colours.insert(tile, shade(tint(zone_at(layout, tile)), 0.28));
    }
    // Ground nothing is planned on: a tree, a boulder, water
    for &tile in &layout.barred {
        if colours.contains_key(&tile) {
            colours.insert(tile, [0.16, 0.10, 0.10]);
        }
    }
    for &tile in &layout.wall {
        colours.insert(tile, if layout.walled { [0.55, 0.42, 0.20] } else { [0.20, 0.20, 0.20] });
    }
    // The way in: the one tile of the wall left open
    if let Some(gate) = layout.gate {
        colours.insert(gate, GATE);
    }
    for laid in &layout.items {
        let colour = if laid.what == 1 { [0.95, 0.45, 0.15] } else { shade(tint(zone_of(laid.what)), if laid.stands { 1.0 } else { 0.6 }) };
        for &tile in &laid.tiles {
            colours.insert(tile, colour);
        }
    }
    if let Some(Answer::Would(plan)) = answer {
        for (tiles, colour) in [(&plan.gone, [0.85, 0.22, 0.20]), (&plan.pulled, [0.60, 0.20, 0.35]), (&plan.raised, [0.95, 0.85, 0.30]), (&plan.built, [0.35, 0.90, 0.45])] {
            for &tile in tiles {
                colours.insert(tile, colour);
            }
        }
    }
    colours.insert(player, PLAYER);
    // The window: every tile drawn, and the cursor, with a margin
    let centre = |(q, r): (i32, i32)| Vec2::from(common_bevy::geometry::flat_top_tile_center(q, r, 1.0));
    let (mut low, mut high) = (centre(cursor), centre(cursor));
    for &tile in colours.keys() {
        low = low.min(centre(tile));
        high = high.max(centre(tile));
    }
    let span = (high - low).max_element() + 6.0;
    let middle = (low + high) / 2.0;
    let mut out = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for down in 0..SIDE {
        for across in 0..SIDE {
            let at = middle + (Vec2::new(across as f32 + 0.5, down as f32 + 0.5) / SIDE as f32 - 0.5) * span;
            // The tile a point lies on: its place over the grid, rounded as a cube's
            let (q, r) = (at.x * 2.0 / 3.0, at.y / 3f32.sqrt() - at.x / 3.0);
            let (mut rq, mut rr, rs) = (q.round(), r.round(), (-q - r).round());
            if (rq - q).abs() > (rr - r).abs() && (rq - q).abs() > (rs + q + r).abs() {
                rq = -rr - rs;
            } else if (rr - r).abs() > (rs + q + r).abs() {
                rr = -rq - rs;
            }
            let tile = (rq as i32, rr as i32);
            let edge = at.distance(centre(tile)) > 0.80;
            let mut colour = colours.get(&tile).copied();
            // A zone's centre: a dot of the zone's colour, ringed dark so it shows on any ground.
            // Where two zones share a centre, the dot is halved between them
            let near = at.distance(centre(tile));
            let mut here = layout.zones.iter().filter(|&&(_, q, r)| (q, r) == tile).map(|&(zone, ..)| zone);
            if let (true, Some(first)) = (near < 0.45, here.next()) {
                let zone = if at.x < centre(tile).x { first } else { here.last().unwrap_or(first) };
                colour = Some(if near < 0.30 { tint(zone) } else { [0.04, 0.04, 0.04] });
            }
            // The cursor: its tile ringed white whatever lies there
            if tile == cursor && edge {
                colour = Some([1.0, 1.0, 1.0]);
            }
            match colour {
                Some([r, g, b]) => out.extend([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255]),
                None => out.extend([12, 12, 12, 200]),
            }
        }
    }
    out
}

/// The planning's root, spawned with its first open and shown while open.
#[derive(Component)]
pub struct PlanRows;

/// Shows the planning while it is open and draws it again whenever it
/// changes.
pub fn update(mut commands: Commands, planning: Res<Planning>, mut rows: Query<(Entity, &mut Node), With<PlanRows>>, mut images: ResMut<Assets<Image>>, asset_server: Res<AssetServer>) {
    if !planning.is_changed() {
        return;
    }
    let root = match rows.single_mut() {
        Ok((root, mut node)) => {
            node.display = if planning.open { Display::Flex } else { Display::None };
            commands.entity(root).despawn_related::<Children>();
            root
        }
        Err(_) if !planning.open => return,
        Err(_) => commands
            .spawn((PlanRows, Node { position_type: PositionType::Absolute, width: Val::Percent(100.), height: Val::Percent(100.), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
            .id(),
    };
    if !planning.open {
        return;
    }
    let text = |text: String, size: f32, shade: f32| (Text::new(text), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(Color::srgb(shade, shade, shade)));
    let Some(layout) = &planning.layout else {
        commands.entity(root).with_children(|screen| {
            screen.spawn((Node { padding: UiRect::all(Val::Px(14.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() }, BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)))).with_children(|panel| {
                panel.spawn(text(if planning.unanswered { "Hub planning: the server did not answer. It may be an older build; restart it".to_owned() } else { "Hub planning: asking the server".to_owned() }, 16.0, 0.95));
            });
        });
        return;
    };
    let size = Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 };
    let drawn = Image::new(size, TextureDimension::D2, texels(layout, planning.answer.as_ref(), planning.cursor, planning.player), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    let drawn = images.add(drawn);
    let rows = listed(layout, planning.planner, planning.cursor);
    let icon = |path: String| (ImageNode::new(asset_server.load(path)), Node { width: Val::Px(24.), height: Val::Px(24.), ..default() });
    // The panel is read at a glance and in any tongue: signs from the glyph font, and pictures
    let font: Handle<Font> = asset_server.load("fonts/IosevkaNerdFont-Regular.ttf");
    let sign = |sign: String, size: f32, shade: f32| (Text::new(sign), TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(Color::srgb(shade, shade, shade)));
    // A zone's colour, as the hub is drawn in it; faint for a planner that takes nothing yet
    let swatch = |zone: u8, faint: bool| {
        let [r, g, b] = tint(zone);
        let (r, g, b) = if faint { (r * 0.4, g * 0.4, b * 0.4) } else { (r, g, b) };
        (Node { width: Val::Px(24.), height: Val::Px(24.), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::all(Val::Px(4.)), ..default() }, BackgroundColor(Color::srgb(r, g, b)))
    };
    let chosen = rows.get(planning.chosen);
    commands.entity(root).with_children(|screen| {
        let panel = Node { column_gap: Val::Px(14.), padding: UiRect::all(Val::Px(14.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() };
        screen.spawn((panel, BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)))).with_children(|panel| {
            // The hub, and under it what its one colour that is no zone's means: where the player stands
            panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.), ..default() }).with_children(|left| {
                // Over the hub: who lives in it, how many its homes house and it is planned for, and whether they defend its wall
                left.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), width: Val::Px(SHOWN), ..default() }).with_children(|hub| {
                    hub.spawn((sign(format!("Residents {} / Housing {} / Planned {}", layout.residents, layout.room, layout.wants), 13.0, 0.9), Node { flex_grow: 1.0, ..default() }));
                    // Green where those who live here defend the wall planned for, red where they do not
                    let colour = if layout.defended { Color::srgb(0.35, 0.90, 0.45) } else { Color::srgb(0.85, 0.22, 0.20) };
                    hub.spawn((Text::new(WALL), TextFont { font: font.clone().into(), font_size: FontSize::Px(15.0), ..default() }, TextColor(colour)));
                    hub.spawn((Text::new(if layout.defended { "Defensible" } else { "Undefended" }), TextFont { font: font.clone().into(), font_size: FontSize::Px(13.0), ..default() }, TextColor(colour)));
                });
                left.spawn((ImageNode::new(drawn), Node { width: Val::Px(SHOWN), height: Val::Px(SHOWN), ..default() }));
                left.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), ..default() }).with_children(|legend| {
                    let [r, g, b] = PLAYER;
                    legend.spawn((Node { width: Val::Px(14.), height: Val::Px(14.), border_radius: BorderRadius::all(Val::Px(3.)), ..default() }, BackgroundColor(Color::srgb(r, g, b))));
                    legend.spawn((sign("You are here".to_owned(), 13.0, 0.8), Node { margin: UiRect::right(Val::Px(14.)), ..default() }));
                    let [r, g, b] = GATE;
                    legend.spawn((Node { width: Val::Px(14.), height: Val::Px(14.), border_radius: BorderRadius::all(Val::Px(3.)), ..default() }, BackgroundColor(Color::srgb(r, g, b))));
                    legend.spawn(sign("Entry".to_owned(), 13.0, 0.8));
                });
            });
            // The pane and, down its right side, the planners' tabs, as the character panel's stand: the one shown opens into the pane.
            // Laid from the right, so the tabs, first, stand last
            panel.spawn(Node { flex_direction: FlexDirection::RowReverse, ..default() }).with_children(|panel| {
            panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.), align_items: AlignItems::FlexStart, padding: UiRect::vertical(Val::Px(8.)), ..default() }).with_children(|strip| {
                crate::systems::keycap::keycap_at(strip, crate::cap!(KEYCODE_BEFORE), Node { margin: UiRect::left(Val::Px(8.)), ..default() });
                for zone in tabs(layout) {
                    let (shown, busy) = (planning.planner == Some(zone), layout.busy & 1 << zone != 0);
                    let frame = Node {
                        padding: UiRect::axes(Val::Px(10.), Val::Px(6.)),
                        border_radius: BorderRadius::right(Val::Px(6.)),
                        ..default()
                    };
                    strip.spawn((frame, BackgroundColor(if shown { PANE } else { SHUT }))).with_children(|tab| {
                        tab.spawn(swatch(zone, busy)).with_children(|on| drop(on.spawn((sign(zone_sign(zone).to_owned(), 15.0, 0.05), TextLayout::justify(Justify::Center), Node { width: Val::Px(24.), left: Val::Px(GLYPH_NUDGE), ..default() }))));
                    });
                }
                crate::systems::keycap::keycap_at(strip, crate::cap!(KEYCODE_NEXT), Node { margin: UiRect::left(Val::Px(8.)), ..default() });
            });
            let pane = Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.), width: Val::Px(320.), padding: UiRect::all(Val::Px(10.)), border_radius: BorderRadius::all(Val::Px(4.)), ..default() };
            panel.spawn((pane, BackgroundColor(PANE))).with_children(|side| {
                let line = Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), min_height: Val::Px(24.), ..default() };
                // What lies under the cursor: the thing, pictured, and who lives in it; or the ground's zone
                side.spawn(line.clone()).with_children(|under| {
                    under.spawn(sign(CURSOR.to_owned(), 15.0, 0.9));
                    match layout.items.iter().find(|laid| laid.tiles.contains(&planning.cursor)) {
                        Some(laid) => {
                            if let Some(path) = thing_icon(laid) {
                                under.spawn(icon(path));
                            }
                            under.spawn(sign(thing_name(laid).to_owned(), 13.0, 0.9));
                            if !laid.stands {
                                under.spawn(sign(WAITS.to_owned(), 15.0, 0.6));
                            }
                            if laid.players + laid.settlers > 0 {
                                under.spawn(sign(format!("{PEOPLE} {}", laid.players + laid.settlers), 15.0, 0.9));
                            }
                        }
                        None if layout.barred.contains(&planning.cursor) => drop(under.spawn(sign(BARRED.to_owned(), 15.0, 0.9))),
                        None if layout.ground.contains(&planning.cursor) => {
                            let zone = zone_at(layout, planning.cursor);
                            under.spawn(swatch(zone, false)).with_children(|on| drop(on.spawn((sign(zone_sign(zone).to_owned(), 15.0, 0.05), TextLayout::justify(Justify::Center), Node { width: Val::Px(24.), left: Val::Px(GLYPH_NUDGE), ..default() }))));
                            under.spawn(sign(zone_name(zone).to_owned(), 13.0, 0.9));
                        }
                        None => drop(under.spawn(sign(NO.to_owned(), 15.0, 0.6))),
                    }
                });
                // The planner whose tab is shown, named; one at work, with why
                side.spawn(line.clone()).with_children(|named| {
                    if let Some(zone) = planning.planner {
                        named.spawn(sign(zone_name(zone).to_owned(), 15.0, 0.95));
                        if layout.busy & 1 << zone != 0 {
                            named.spawn(sign(WAITS.to_owned(), 15.0, 0.9));
                        }
                    }
                });
                // A row to each option of the tab shown, its key first; the one chosen lit
                for (at, (row, waits)) in rows.iter().enumerate().take(ENTRY_KEYS.len()) {
                    let lit = at == planning.chosen;
                    side.spawn((
                        Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), padding: UiRect::axes(Val::Px(6.), Val::Px(2.)), border: UiRect::all(Val::Px(1.)), border_radius: BorderRadius::all(Val::Px(4.)), min_height: Val::Px(30.), ..default() },
                        BackgroundColor(if lit { CURSOR_ROW } else { OTHER_ROW }),
                        BorderColor::all(if lit { Color::srgb(0.95, 0.8, 0.3) } else { UNWORN }),
                    ))
                    .with_children(|cell| {
                        // One its planner will not do yet, or with nothing under the cursor to act on, is drawn dim, and marked
                        let aimless = aimless(layout, row.order, planning.cursor);
                        let shade = if *waits || aimless { 0.45 } else { 0.9 };
                        if aimless {
                            cell.spawn(Node { width: Val::Px(crate::systems::keycap::KEY_WIDTH), ..default() });
                        } else {
                            crate::systems::keycap::keycap(cell, crate::caps!(ENTRY_KEYS)[at]);
                        }
                        cell.spawn(sign(order_sign(row.order), 15.0, shade));
                        if let Some(path) = &row.icon {
                            cell.spawn(icon(path.clone()));
                        }
                        cell.spawn((sign(order_name(row), 13.0, shade), Node { flex_grow: 1.0, ..default() }));
                        let Needs { hide, wood, stone } = row.takes;
                        for (material, count) in [(common::Material::Hide, hide), (common::Material::Softwood, wood), (common::Material::Sandstone, stone)] {
                            if count > 0 {
                                cell.spawn(icon(format!("icons/{}.png", material.stem())));
                                cell.spawn(sign(count.to_string(), 13.0, shade));
                            }
                        }
                        if aimless {
                            cell.spawn(sign(BARRED.to_owned(), 15.0, 0.7));
                        } else if *waits {
                            cell.spawn(sign(WAITS.to_owned(), 15.0, 0.7));
                        }
                    });
                }
                // What the action chosen would change: built, removed, stakes driven and pulled, and what it still needs
                side.spawn(line.clone()).with_children(|would| match &planning.answer {
                    Some(Answer::Would(plan)) => {
                        for (count, mark, colour) in [(plan.built.len(), ADDS, [0.35, 0.90, 0.45]), (plan.gone.len(), REMOVES, [0.85, 0.22, 0.20]), (plan.raised.len(), WALL, [0.95, 0.85, 0.30]), (plan.pulled.len(), WALL, [0.60, 0.20, 0.35])] {
                            if count > 0 {
                                would.spawn((Text::new(format!("{mark} {count}")), TextFont { font: font.clone().into(), font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(colour[0], colour[1], colour[2]))));
                            }
                        }
                        let Needs { hide, wood, stone } = plan.needs;
                        for (material, count) in [(common::Material::Hide, hide), (common::Material::Softwood, wood), (common::Material::Sandstone, stone)] {
                            if count > 0 {
                                would.spawn(icon(format!("icons/{}.png", material.stem())));
                                would.spawn(sign(count.to_string(), 13.0, 0.9));
                            }
                        }
                    }
                    Some(Answer::Refused) => drop(would.spawn(sign(BARRED.to_owned(), 15.0, 0.9))),
                    None => {}
                });
                // The keys: each a cap and the sign of what it does
                let does = |row: &mut ChildSpawnerCommands, caps: &[crate::systems::keycap::Cap], what: &str| {
                    for &cap in caps {
                        crate::systems::keycap::keycap(row, cap);
                    }
                    row.spawn((sign(what.to_owned(), 15.0, 0.8), Node { margin: UiRect::right(Val::Px(10.)), ..default() }));
                };
                // The occupancy being set: the one count box
                if let Some(target) = planning.counting {
                    crate::systems::drop_panel::spawn_entry(side, target, "Preview");
                }
                let waits = chosen.is_some_and(|(_, waits)| *waits) || matches!(planning.answer, Some(Answer::Refused));
                side.spawn(line.clone()).with_children(|keys| {
                    if chosen.is_some() && planning.counting.is_none() {
                        does(keys, &[crate::cap!(KEYCODE_TAKE_ALL)], if waits { BARRED } else { "Confirm" });
                    }
                    does(keys, &[crate::cap!(KEYCODE_CLOSE)], if chosen.is_some() { "Cancel" } else { "Close" });
                });
                // The keys that move the cursor: the four in a row, named for what they do, and the one that brings it to the player
                side.spawn(line.clone()).with_children(|moves| {
                    for cap in crate::caps!([KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight]) {
                        crate::systems::keycap::keycap(moves, cap);
                    }
                    moves.spawn(sign("Move cursor".to_owned(), 13.0, 0.8));
                });

            });
            });
        });
    });
}

/// The signs the panel says things by, from the glyph font: people, a
/// wall, yes and no, the cursor, what waits on work, what cannot be,
/// what a plan adds and removes, and what the keys do.
const PEOPLE: &str = "\u{f0c0}";
const WALL: &str = "\u{f132}";
const NO: &str = "\u{f00d}";
const CURSOR: &str = "\u{f05b}";
const WAITS: &str = "\u{f252}";
const BARRED: &str = "\u{f05e}";
const ADDS: &str = "\u{f067}";
const REMOVES: &str = "\u{f068}";

/// A zone's sign: a cart for logistics, a home, a market's basket, a
/// workshop's wrench, a farm's leaf, a plaza's flag.
fn zone_sign(zone: u8) -> &'static str {
    match zone {
        1 => "\u{f0d1}",
        2 => "\u{f015}",
        3 => "\u{f07a}",
        4 => "\u{f0ad}",
        5 => "\u{f06c}",
        6 => "\u{f024}",
        _ => "\u{f05e}",
    }
}

/// An order's sign: residents more and fewer, the hub drawn in, its
/// wall, a thing raised, a home made larger or smaller, a zone's centre
/// set here, a planner hired.
fn order_sign(order: Order) -> String {
    match order {
        Order::Residents(_) => PEOPLE.to_owned(),
        Order::Shrink => "\u{f066}".to_owned(),
        Order::Wall => WALL.to_owned(),
        Order::Raise(_) => ADDS.to_owned(),
        Order::Upgrade(..) => "\u{f062}".to_owned(),
        Order::Downgrade(..) => "\u{f063}".to_owned(),
        Order::Zone(..) => "\u{f041}".to_owned(),
        Order::Hire(_) => "\u{f234}".to_owned(),
    }
}

/// A zone's name, the one word a row or the cursor says of it.
fn zone_name(zone: u8) -> &'static str {
    match zone {
        1 => "Logistics",
        2 => "Residential",
        3 => "Market",
        4 => "Crafting",
        5 => "Farming",
        6 => "Plaza",
        _ => "",
    }
}

/// A thing's name.
fn thing_name(laid: &Laid) -> &'static str {
    match (laid.what, laid.level) {
        (1, _) => "Hearth",
        (2, _) => "Hitching post",
        (3, _) => "Wagon",
        (HOME, 1) => "Tent",
        (HOME, 2) => "Pavilion",
        (HOME, 3) => "House",
        (HOME, 4) => "Tenement",
        (5, _) => "Storehouse",
        _ => "Building",
    }
}

/// An order's name: a word or two beside its sign.
fn order_name(row: &Offer) -> String {
    match row.order {
        Order::Residents(_) => "Change occupancy".to_owned(),
        Order::Shrink => "Shrink".to_owned(),
        Order::Wall => "Wall".to_owned(),
        Order::Zone(..) => "Move zone".to_owned(),
        Order::Hire(_) => "Hire".to_owned(),
        // What a home becomes is its picture's to say
        Order::Upgrade(..) => "Upgrade".to_owned(),
        Order::Downgrade(..) => "Downgrade".to_owned(),
        Order::Raise(_) => row.name.clone(),
    }
}

/// Where a thing's picture is, where it has one.
fn thing_icon(laid: &Laid) -> Option<String> {
    let stem = match (laid.what, laid.level) {
        (1, _) => "hearth-sandstone",
        (2, _) => "hitching-post",
        (3, _) => "wagon",
        (HOME, 1) => "tent",
        (HOME, 2) => "pavilion",
        (HOME, 3) => "house",
        (HOME, 4) => "tenement",
        (5, _) => "storehouse",
        _ => return None,
    };
    Some(format!("icons/{stem}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> Layout {
        Layout {
            wants: 4,
            hired: 1 << 1 | 1 << 2,
            hires: 3,
            ground: vec![(0, 0), (1, 0), (0, 1), (5, 5)],
            zones: vec![(1, 0, 0), (2, 5, 5)],
            gate: None,
            items: vec![Laid { what: 1, level: 0, tiles: vec![(0, 0)], stands: true, ..Laid::default() }, Laid { what: HOME, level: 1, tiles: vec![(1, 0)], stands: true, ..Laid::default() }],
            ..Layout::default()
        }
    }

    /// Each hired planner has a tab; a tab's options are that planner's
    /// orders, a home under the cursor upgraded to what it becomes, and
    /// the hiring last; and one at work shows its options and does none.
    #[test]
    fn a_tab_holds_its_planners_options() {
        assert_eq!(tabs(&hub()), vec![1, 2]);
        let orders = |planner, layout: &Layout| -> Vec<Offer> { listed(layout, Some(planner), (1, 0)).into_iter().map(|(row, _)| row).collect() };
        let homes = orders(2, &hub());
        assert_eq!(homes.iter().find(|row| row.order == Order::Upgrade(1, 0)).unwrap().takes, Fixture::Pavilion.needs());
        assert!(!homes.iter().any(|row| row.order == Order::Wall) && orders(1, &hub()).iter().any(|row| row.order == Order::Wall), "each planner has its own");
        assert!(matches!(homes.last(), Some(row) if matches!(row.order, Order::Hire(_))) && homes.len() <= ENTRY_KEYS.len(), "the hiring comes last, and every option has a key");
        let busy = Layout { busy: 1 << 2, ..hub() };
        let waits: Vec<bool> = listed(&busy, Some(2), (1, 0)).into_iter().map(|(_, waits)| waits).collect();
        assert!(orders(2, &busy) == homes && waits.iter().rev().skip(1).all(|waits| *waits) && waits.last() == Some(&false), "one at work shows its options and does none, but a hire is no one's to wait on");
        assert_eq!(listed(&Layout { hired: 0, ..hub() }, None, (1, 0)).len(), 1, "with no planner, the hiring alone");
    }

    /// Ground is of the zone whose centre is nearest, and the hub is
    /// drawn whole, every texel of it.
    #[test]
    fn the_hub_is_drawn_whole() {
        let hub = hub();
        assert_eq!((zone_at(&hub, (1, 0)), zone_at(&hub, (5, 4))), (1, 2));
        let drawn = texels(&hub, None, (0, 1), (0, 0));
        assert_eq!(drawn.len(), (SIDE * SIDE * 4) as usize);
        assert!(drawn.chunks(4).any(|texel| texel[..3] == [255, 255, 255]), "the cursor is ringed");
    }
}
