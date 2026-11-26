//! Planning a hub: P, on a hub's ground, opens the hub seen from above,
//! its ground tinted by zone, what stands on it and what is planned, and
//! its wall. A cursor moves over it a tile at a time by six of the
//! numpad's keys, one to each side of a hex the panel draws them round.
//! A planner is chosen first, then one of its actions ([`listed`]). One list holds every order the hub's planners take
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
    equipment_panel::{CURSOR_ROW, OTHER_ROW, UNWORN},
    focus::{NumpadFocus, Panel},
    gathering::{KEYCODE_CLOSE, KEYCODE_TAKE_ALL},
    settlers::{aims, offers, Offer},
};

/// The key that opens the planning of the hub the player stands in, and
/// shuts it.
pub const KEYCODE_PLAN: KeyCode = KeyCode::KeyP;
const KEYCODE_NEXT: KeyCode = KeyCode::NumpadAdd;
const KEYCODE_BEFORE: KeyCode = KeyCode::NumpadSubtract;
const KEYCODE_HERE: KeyCode = KeyCode::NumpadDecimal;

/// The keys that move the cursor and the step each takes over the grid,
/// one to each of the six tiles beside: the numpad's top row to the
/// three northward, the row under it to the three southward, as the
/// panel's hex shows them.
const STEPS: [(KeyCode, (i32, i32)); 6] = [
    (KeyCode::Numpad8, (0, -1)),
    (KeyCode::Numpad9, (1, -1)),
    (KeyCode::Numpad6, (1, 0)),
    (KeyCode::Numpad5, (0, 1)),
    (KeyCode::Numpad4, (-1, 1)),
    (KeyCode::Numpad7, (-1, 0)),
];

/// A flat-topped hex, `side` texels wide, as RGBA rows: what the keys
/// that move the cursor stand round in the panel.
fn hex(side: u32) -> Vec<u8> {
    let (half, mut out) = (side as f32 / 2.0, Vec::with_capacity((side * side * 4) as usize));
    for down in 0..side {
        for across in 0..side {
            let (x, y) = (((across as f32 + 0.5) - half).abs() / half, ((down as f32 + 0.5) - half).abs() / half);
            // Inside the six sides, and how near the nearest of them
            let within = (3f32.sqrt() / 2.0 - y).min((3f32.sqrt() - 3f32.sqrt() * x - y) / 2.0);
            out.extend(if within < 0.0 { [0, 0, 0, 0] } else if within < 0.10 { [230, 230, 230, 255] } else { [70, 70, 70, 255] });
        }
    }
    out
}

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
    /// The planner whose actions are shown, by its zone's number; none
    /// while a planner is being chosen.
    planner: Option<u8>,
    answer: Option<Answer>,
    carried: bool,
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

/// A row of the planning's menu: a planner to choose, by its zone's
/// number, and whether it is at work; or an action of the planner
/// chosen, and whether that planner, at work, will not do it yet.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Planner(u8, bool),
    Act(Offer, bool),
}

/// The planning's menu with the cursor on `cursor`. With no planner
/// chosen, the hub's planners, then the hiring of the next where it may
/// have one. With one chosen, that planner's own orders
/// (`settlers::offers`): shown, and what each would do, while it is at
/// work, but not done.
pub fn listed(layout: &Layout, planner: Option<u8>, cursor: (i32, i32)) -> Vec<Row> {
    let busy = |zone: u8| layout.busy & 1 << zone != 0;
    match planner {
        None => {
            let planners = (1..=6u8).filter(|zone| layout.hired & 1 << zone != 0).map(|zone| Row::Planner(zone, busy(zone)));
            planners.chain(offers(Wants { hires: layout.hires, ..Wants::default() }, cursor, cursor, None).into_iter().map(|row| Row::Act(row, false))).collect()
        }
        Some(zone) => offers(Wants { zone, ..Wants::default() }, cursor, cursor, home_on(layout, cursor)).into_iter().map(|row| Row::Act(row, busy(zone))).collect(),
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
            planning.planner = None;
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
    // Back from a planner's actions to the planners, and from them out
    if keys.take(KEYCODE_CLOSE, "Go back to the planners, or put the plan away") {
        match planning.planner.take() {
            Some(zone) => {
                let at = listed(planning.layout.as_ref().unwrap_or(&Layout::default()), None, planning.cursor).iter().position(|row| matches!(row, Row::Planner(of, _) if *of == zone));
                (planning.chosen, planning.answer) = (at.unwrap_or(0), None);
            }
            None => planning.open = false,
        }
        return;
    }
    let Some(layout) = planning.layout.clone() else { return };
    let before = (planning.cursor, planning.chosen, planning.planner);
    for (key, (q, r)) in STEPS {
        if keys.take(key, "Move the cursor over the hub") {
            planning.cursor = (planning.cursor.0 + q, planning.cursor.1 + r);
        }
    }
    if keys.take(KEYCODE_HERE, "Put the cursor where you stand") {
        planning.cursor = here;
    }
    let rows = listed(&layout, planning.planner, planning.cursor);
    if rows.is_empty() {
        return;
    }
    if keys.take(KEYCODE_NEXT, "Choose the next row") {
        planning.chosen = (planning.chosen + 1) % rows.len();
    }
    if keys.take(KEYCODE_BEFORE, "Choose the row before") {
        planning.chosen = (planning.chosen + rows.len() - 1) % rows.len();
    }
    if planning.chosen >= rows.len() {
        planning.chosen = 0;
    }
    let accepted = keys.take(KEYCODE_TAKE_ALL, "Choose the planner, or have its action done");
    match &rows[planning.chosen] {
        // A planner chosen: its actions are the menu from then
        Row::Planner(zone, _) => {
            if accepted {
                (planning.planner, planning.chosen, planning.answer) = (Some(*zone), 0, None);
                // Its first action is asked after as its menu opens
                if let Some(Row::Act(row, _)) = listed(&layout, Some(*zone), planning.cursor).first() {
                    writer.write(Try { event: Event::Plan { ent, order: row.order, carry: false } });
                }
            }
        }
        Row::Act(row, waits) => {
            let order = row.order;
            // Asked after anew as it is chosen, and as the cursor moves where it acts on the cursor
            let moved = before.0 != planning.cursor;
            if before.1 != planning.chosen || (moved && aims(order)) {
                planning.answer = None;
                if !matches!(order, Order::Hire(_)) {
                    writer.write(Try { event: Event::Plan { ent, order, carry: false } });
                }
            }
            if accepted && !waits && !matches!(planning.answer, Some(Answer::Refused)) {
                planning.carried = true;
                writer.write(Try { event: Event::Plan { ent, order, carry: true } });
            }
        }
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
/// the two; its wall; over them what `answer` would change, raised green
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
    colours.insert(player, [0.25, 0.85, 0.95]);
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
            panel.spawn((ImageNode::new(drawn), Node { width: Val::Px(SHOWN), height: Val::Px(SHOWN), ..default() }));
            panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(5.), width: Val::Px(300.), ..default() }).with_children(|side| {
                let line = Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), min_height: Val::Px(24.), ..default() };
                // The hub: who lives in it of how many planned, and its wall
                side.spawn(line.clone()).with_children(|hub| {
                    if let Some(zone) = planning.planner {
                        hub.spawn(swatch(zone, false)).with_children(|on| drop(on.spawn(sign(zone_sign(zone).to_owned(), 15.0, 0.05))));
                    }
                    hub.spawn(sign(format!("{PEOPLE} {}/{}", layout.residents, layout.wants), 15.0, 0.9));
                    hub.spawn(sign(format!("{WALL} {}", if layout.walled { YES } else { NO }), 15.0, 0.9));
                });
                // What lies under the cursor: the thing, pictured, and who lives in it; or the ground's zone
                side.spawn(line.clone()).with_children(|under| {
                    under.spawn(sign(CURSOR.to_owned(), 15.0, 0.9));
                    match layout.items.iter().find(|laid| laid.tiles.contains(&planning.cursor)) {
                        Some(laid) => {
                            if let Some(path) = thing_icon(laid) {
                                under.spawn(icon(path));
                            }
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
                            under.spawn(swatch(zone, false)).with_children(|on| drop(on.spawn(sign(zone_sign(zone).to_owned(), 15.0, 0.05))));
                        }
                        None => drop(under.spawn(sign(NO.to_owned(), 15.0, 0.6))),
                    }
                });
                // A row to each planner, or to each action of the one chosen; the row chosen lit
                for (at, row) in rows.iter().enumerate() {
                    let lit = at == planning.chosen;
                    side.spawn((
                        Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), padding: UiRect::axes(Val::Px(6.), Val::Px(2.)), border: UiRect::all(Val::Px(1.)), border_radius: BorderRadius::all(Val::Px(4.)), min_height: Val::Px(30.), ..default() },
                        BackgroundColor(if lit { CURSOR_ROW } else { OTHER_ROW }),
                        BorderColor::all(if lit { Color::srgb(0.95, 0.8, 0.3) } else { UNWORN }),
                    ))
                    .with_children(|cell| match row {
                        Row::Planner(zone, busy) => {
                            // One at work is drawn faint, with why at the row's end
                            cell.spawn(swatch(*zone, *busy)).with_children(|on| drop(on.spawn(sign(zone_sign(*zone).to_owned(), 15.0, 0.05))));
                            cell.spawn(Node { flex_grow: 1.0, ..default() });
                            if *busy {
                                cell.spawn(sign(WAITS.to_owned(), 15.0, 0.9));
                            }
                        }
                        Row::Act(row, waits) => {
                            // One its planner will not do yet is drawn dim, and marked
                            let shade = if *waits { 0.45 } else { 0.9 };
                            cell.spawn(sign(order_sign(row.order), 15.0, shade));
                            if let Some(path) = &row.icon {
                                cell.spawn(icon(path.clone()));
                            }
                            cell.spawn(Node { flex_grow: 1.0, ..default() });
                            let Needs { hide, wood, stone } = row.takes;
                            for (material, count) in [(common::Material::Hide, hide), (common::Material::Softwood, wood), (common::Material::Sandstone, stone)] {
                                if count > 0 {
                                    cell.spawn(icon(format!("icons/{}.png", material.stem())));
                                    cell.spawn(sign(count.to_string(), 13.0, shade));
                                }
                            }
                            if *waits {
                                cell.spawn(sign(WAITS.to_owned(), 15.0, 0.7));
                            }
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
                let planners = planning.planner.is_none();
                let waits = matches!(chosen, Some(Row::Act(_, true))) || matches!(planning.answer, Some(Answer::Refused));
                side.spawn(line.clone()).with_children(|keys| {
                    does(keys, &crate::caps!([KEYCODE_BEFORE, KEYCODE_NEXT]), CHOOSE);
                    does(keys, &[crate::cap!(KEYCODE_TAKE_ALL)], if planners && matches!(chosen, Some(Row::Planner(..))) { OPEN } else if waits { BARRED } else { YES });
                    does(keys, &[crate::cap!(KEYCODE_CLOSE)], if planners { NO } else { BACK });
                });
                // The keys that move the cursor, each on the side of a hex it moves toward
                side.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(10.), ..default() }).with_children(|moves| {
                    moves.spawn(Node { width: Val::Px(132.), height: Val::Px(112.), ..default() }).with_children(|compass| {
                        let drawn = Image::new(Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, TextureDimension::D2, hex(64), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
                        compass.spawn((ImageNode::new(images.add(drawn)), Node { position_type: PositionType::Absolute, left: Val::Px(40.), top: Val::Px(30.), width: Val::Px(52.), height: Val::Px(52.), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })).with_children(|hex| drop(hex.spawn(sign(CURSOR.to_owned(), 15.0, 0.95))));
                        let at = |left: f32, top: f32| Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), ..default() };
                        for (cap, place) in [
                            (crate::cap!(KeyCode::Numpad8), at(49., 6.)),
                            (crate::cap!(KeyCode::Numpad9), at(94., 30.)),
                            (crate::cap!(KeyCode::Numpad6), at(94., 64.)),
                            (crate::cap!(KeyCode::Numpad5), at(49., 88.)),
                            (crate::cap!(KeyCode::Numpad4), at(4., 64.)),
                            (crate::cap!(KeyCode::Numpad7), at(4., 30.)),
                        ] {
                            crate::systems::keycap::keycap_at(compass, cap, place);
                        }
                    });
                    moves.spawn(line.clone()).with_children(|rest| does(rest, &[crate::cap!(KEYCODE_HERE)], ME));
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
const YES: &str = "\u{f00c}";
const NO: &str = "\u{f00d}";
const CURSOR: &str = "\u{f05b}";
const WAITS: &str = "\u{f252}";
const BARRED: &str = "\u{f05e}";
const ADDS: &str = "\u{f067}";
const REMOVES: &str = "\u{f068}";
const CHOOSE: &str = "\u{f07d}";
const OPEN: &str = "\u{f061}";
const BACK: &str = "\u{f060}";
const ME: &str = "\u{f007}";

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
        Order::Residents(more) => format!("{PEOPLE} {more:+}"),
        Order::Shrink => "\u{f066}".to_owned(),
        Order::Wall => WALL.to_owned(),
        Order::Raise(_) => ADDS.to_owned(),
        Order::Upgrade(..) => "\u{f062}".to_owned(),
        Order::Downgrade(..) => "\u{f063}".to_owned(),
        Order::Zone(..) => "\u{f041}".to_owned(),
        Order::Hire(_) => "\u{f234}".to_owned(),
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
            items: vec![Laid { what: 1, level: 0, tiles: vec![(0, 0)], stands: true, ..Laid::default() }, Laid { what: HOME, level: 1, tiles: vec![(1, 0)], stands: true, ..Laid::default() }],
            ..Layout::default()
        }
    }

    /// The menu is the hub's planners and the hiring of the next; a
    /// planner chosen, it is that planner's orders, a home under the
    /// cursor upgraded to what it becomes; and one at work offers nothing.
    #[test]
    fn a_planner_is_chosen_and_then_its_action() {
        let top = listed(&hub(), None, (1, 0));
        assert_eq!(top.iter().filter(|row| matches!(row, Row::Planner(..))).count(), 2);
        assert!(matches!(top.last(), Some(Row::Act(row, false)) if matches!(row.order, Order::Hire(_))), "the hiring comes last");
        let orders = |planner, layout: &Layout| -> Vec<Offer> { listed(layout, Some(planner), (1, 0)).into_iter().filter_map(|row| if let Row::Act(row, _) = row { Some(row) } else { None }).collect() };
        let homes = orders(2, &hub());
        assert_eq!(homes.iter().find(|row| row.order == Order::Upgrade(1, 0)).unwrap().takes, Fixture::Pavilion.needs());
        assert!(!homes.iter().any(|row| row.order == Order::Wall) && orders(1, &hub()).iter().any(|row| row.order == Order::Wall), "each planner has its own");
        let busy = Layout { busy: 1 << 2, ..hub() };
        assert!(orders(2, &busy) == homes && listed(&busy, Some(2), (1, 0)).iter().all(|row| matches!(row, Row::Act(_, true))), "one at work shows its actions, and does none");
        assert!(matches!(listed(&busy, None, (1, 0))[1], Row::Planner(2, true)));
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
