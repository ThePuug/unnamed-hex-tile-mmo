//! Ropes: a pull drawn from what pulls to whom it pulls. One rope stands
//! for each foe a rune's pulse winds up on (the rune's `Event::Activate`,
//! naming the foe), from the rune's glyph, in its glyph's light. Unbuilt:
//! a rope from an actor.
//!
//! A rope is a child of its source, so it goes when its source does. Its
//! shader lays it from one shared mesh (`shaders/rope.wesl`): a cord from
//! the source to a loop round its target's `socket.tether`, the loop the
//! size the rig declares there ([`Girth`]). It hangs slack while the
//! pulse winds up, tightens as the pulse lands (`Event::Resolved`), stays
//! taut while its target slides in (`Displacing`) and lets go as the
//! slide ends. A pulse that lets its foe go drops it, and so does one
//! that lands and moves nothing.

use std::{collections::HashSet, time::Duration};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
};
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::{NotShadowCaster, NotShadowReceiver};

use common_bevy::{
    components::entity_type::EntityType,
    message::{Do, Event as GameEvent, Outcome},
    moment::Moment,
};

use crate::{
    components::Displacing,
    plugins::cel::CelMaterial,
    systems::{
        actor::actor_name,
        equipment::{joints_of, rig, Worn},
    },
};

const SHADER: &str = "shaders/rope.wesl";

/// The socket a rope leaves a rune from, the glyph's own
const FROM: &str = "socket.glyph";
/// The socket its loop closes round
const TETHER: &str = "socket.tether";

/// Rings along the cord and round the loop, and faces round the tube
const CORD_RINGS: usize = 24;
const LOOP_RINGS: usize = 20;
const SIDES: usize = 6;

/// The cord's radius and the loop's, in world units
const CORD_RADIUS: f32 = 0.025;
const LOOP_RADIUS: f32 = 0.02;
/// How far a slack cord's middle hangs, as a share of its length
const SLACK_SAG: f32 = 0.18;
/// The loop's size over the girth the rig declares, hanging loose and
/// closed tight, and how far a loose loop droops below its middle, as a
/// share of its width
const LOOSE: f32 = 1.6;
const CINCH: f32 = 1.1;
const DROOP: f32 = 0.5;
/// How long a rope takes to tighten, how long a taut one waits on its
/// target's slide before it lets go, and how long a dropped one takes to
/// fall away
const SNAP: Duration = Duration::from_millis(80);
const TAUT_WAIT: Duration = Duration::from_millis(400);
const DROP: Duration = Duration::from_millis(350);
/// How long past its pulse's end a slack rope waits to hear how the pulse
/// ended before it falls away
const UNHEARD: Duration = Duration::from_millis(500);

pub type RopeMaterial = ExtendedMaterial<StandardMaterial, RopeExtension>;

/// The rope's shape, in its own frame: the uniform its shader lays it by.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct RopeShape {
    /// Where it leaves its source; w the cord's radius
    pub start: Vec4,
    /// Where the cord meets the loop; w how far its middle sags
    pub end: Vec4,
    /// The loop's middle; w the loop's own radius
    pub middle: Vec4,
    /// The loop's two half-widths
    pub wide: Vec4,
    pub deep: Vec4,
    /// The way the world's down lies; w the loop's angle where the cord
    /// meets it
    pub down: Vec4,
}

/// A rope's shape over the standard material it is lit as.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct RopeExtension {
    #[uniform(100)]
    pub shape: RopeShape,
}

impl MaterialExtension for RopeExtension {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/cel_material.wesl".into()
    }
    /// Its one attribute, in every pass, and both faces drawn: a vertex
    /// carries where on the rope it lies, never a position.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?];
        Ok(())
    }
}

/// The loop round a body at its tether socket, as the rig declares it in
/// the socket's frame: its middle and its two half-widths.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Girth {
    pub middle: Vec3,
    pub wide: Vec3,
    pub deep: Vec3,
}

impl Girth {
    /// From the nine numbers a socket's `loop` extras hold
    pub fn from_extras(n: [f32; 9]) -> Self {
        Self { middle: Vec3::new(n[0], n[1], n[2]), wide: Vec3::new(n[3], n[4], n[5]), deep: Vec3::new(n[6], n[7], n[8]) }
    }
}

/// The mesh every rope shares
#[derive(Resource)]
pub struct RopeKit {
    mesh: Handle<Mesh>,
}

impl FromWorld for RopeKit {
    fn from_world(world: &mut World) -> Self {
        Self { mesh: world.resource_mut::<Assets<Mesh>>().add(rope_mesh()) }
    }
}

/// The rope's mesh: the cord's rings, then the loop's, each a ring of
/// faces round the tube. Each vertex's position is where on the rope it
/// lies: the part, 0 the cord and 1 the loop; how far along, 0 to 1; and
/// where round the tube, 0 to 1. The loop's last ring is its first, so it
/// closes.
pub fn rope_mesh() -> Mesh {
    let mut places: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (part, steps) in [(0.0, CORD_RINGS - 1), (1.0, LOOP_RINGS)] {
        let base = places.len() as u32;
        for i in 0..=steps {
            for k in 0..=SIDES {
                places.push([part, i as f32 / steps as f32, k as f32 / SIDES as f32]);
            }
        }
        let row = (SIDES + 1) as u32;
        for i in 0..steps as u32 {
            for k in 0..SIDES as u32 {
                let a = base + i * row + k;
                let b = a + row;
                indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, places)
        .with_inserted_indices(Indices::U32(indices))
}

/// Where a rope is in its pull
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// Its pulse winds up
    Slack,
    /// Its pulse landed: tightening, then waiting on its target's slide
    Taut { since: Moment },
    /// Its target slides toward its source
    Reeling,
    /// Its pulse let the target go, or landed and moved nothing: falling
    /// away
    Dropping { since: Moment },
}

impl Stage {
    /// The stage after this one at `now`: its pulse still `winding`, or
    /// ended having `landed` (None where nothing is known of its end), its
    /// target `sliding`. None when the rope is done.
    pub fn next(self, winding: bool, landed: Option<bool>, sliding: bool, now: Moment) -> Option<Stage> {
        match self {
            Stage::Slack if winding => Some(Stage::Slack),
            Stage::Slack => match landed {
                Some(true) if sliding => Some(Stage::Reeling),
                Some(true) => Some(Stage::Taut { since: now }),
                _ => Some(Stage::Dropping { since: now }),
            },
            Stage::Taut { .. } if sliding => Some(Stage::Reeling),
            Stage::Taut { since } => (now.since(since) < TAUT_WAIT).then_some(self),
            Stage::Reeling => sliding.then_some(Stage::Reeling),
            Stage::Dropping { since } => (now.since(since) < DROP).then_some(self),
        }
    }

    /// How tight it is at `now`, 0 slack to 1 taut
    fn tight(self, now: Moment) -> f32 {
        match self {
            Stage::Slack | Stage::Dropping { .. } => 0.0,
            Stage::Taut { since } => (now.since(since).as_secs_f32() / SNAP.as_secs_f32()).min(1.0),
            Stage::Reeling => 1.0,
        }
    }

    /// How far it has fallen away at `now`, 0 to 1
    fn fallen(self, now: Moment) -> f32 {
        match self {
            Stage::Dropping { since } => (now.since(since).as_secs_f32() / DROP.as_secs_f32()).min(1.0),
            _ => 0.0,
        }
    }
}

/// The shape of a rope from `start` to a loop of `girth` (its middle and
/// half-widths), every point in one frame where `down` is the world's down
/// and `scale` world units are one: slack it sags and its loop hangs loose
/// and low, taut it runs straight to a loop closed round the body, and
/// fallen away it thins to nothing.
pub fn shape(start: Vec3, girth: Girth, down: Vec3, scale: f32, tight: f32, fallen: f32) -> RopeShape {
    let size = LOOSE + (CINCH - LOOSE) * tight;
    let middle = girth.middle + down * DROOP * girth.wide.length() * (1.0 - tight);
    let (wide, deep) = (girth.wide * size, girth.deep * size);
    // Where the cord meets the loop: the loop's point facing the source
    let toward = start - middle;
    let angle = (toward.dot(deep) / deep.length_squared().max(1e-12)).atan2(toward.dot(wide) / wide.length_squared().max(1e-12));
    let end = middle + wide * angle.cos() + deep * angle.sin();
    let sag = SLACK_SAG * start.distance(end) * (1.0 - tight) * (1.0 + fallen);
    let thick = (1.0 - fallen) / scale;
    RopeShape {
        start: start.extend(CORD_RADIUS * thick),
        end: end.extend(sag),
        middle: middle.extend(LOOP_RADIUS * thick),
        wide: wide.extend(0.0),
        deep: deep.extend(0.0),
        down: down.extend(angle),
    }
}

/// One rune's rope to one foe: what pulls and whom, the moment its pulse
/// began on the server's clock, the moment on the client's it stops
/// waiting on that pulse, whether the pulse landed once that is heard, its
/// stage, and its two ends once found: the node it leaves from and its
/// target's tether.
#[derive(Component)]
pub struct Rope {
    source: Entity,
    target: Entity,
    began: Moment,
    until: Moment,
    landed: Option<bool>,
    stage: Stage,
    from: Option<Entity>,
    tether: Option<Entity>,
}

/// The colour of the first material under `node`, as the glTF authored it
fn tint_under(
    node: Entity,
    children: &Query<&Children>,
    cel: &Query<&MeshMaterial3d<CelMaterial>>,
    cels: &Assets<CelMaterial>,
    standard: &Query<&MeshMaterial3d<StandardMaterial>>,
    standards: &Assets<StandardMaterial>,
) -> Option<Color> {
    children.iter_descendants(node).find_map(|e| {
        cel.get(e).ok().and_then(|m| cels.get(&m.0)).map(|m| m.base.base_color)
            .or_else(|| standard.get(e).ok().and_then(|m| standards.get(&m.0)).map(|m| m.base_color))
    })
}

/// Gives each foe a rune's pulse winds up on a rope from that rune, slack
/// until the pulse ends, and takes how each pulse ended; a foe caught
/// again keeps its rope, slack anew. A rope is lit in its glyph's colour,
/// as light, once the rune's glyph is drawn.
#[allow(clippy::too_many_arguments)]
pub fn throw(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    kit: Res<RopeKit>,
    mut materials: ResMut<Assets<RopeMaterial>>,
    mut ropes: Query<(Entity, &mut Rope)>,
    types: Query<&EntityType>,
    children: Query<&Children>,
    names: Query<&Name>,
    (cel, cels, standard, standards): (Query<&MeshMaterial3d<CelMaterial>>, Res<Assets<CelMaterial>>, Query<&MeshMaterial3d<StandardMaterial>>, Res<Assets<StandardMaterial>>),
    (server, time): (Res<crate::resources::Server>, Res<Time>),
) {
    let now = Moment::ZERO + time.elapsed();
    let game_now = server.now(time.elapsed().as_millis());
    for message in reader.read() {
        match message.event {
            GameEvent::Activate { ent: source, activation } => {
                let Some(target) = activation.target else { continue };
                if !matches!(types.get(source), Ok(EntityType::Rune(_))) {
                    continue;
                }
                let until = now + activation.left(game_now) + UNHEARD;
                match ropes.iter_mut().find(|(_, rope)| (rope.source, rope.target) == (source, target)) {
                    // A pulse told again, to a client that met its rune late
                    Some((_, rope)) if rope.began == activation.began => {}
                    Some((_, mut rope)) => {
                        rope.began = activation.began;
                        rope.until = until;
                        rope.landed = None;
                        rope.stage = Stage::Slack;
                    }
                    None => {
                        commands.spawn((
                            Rope { source, target, began: activation.began, until, landed: None, stage: Stage::Slack, from: None, tether: None },
                            Transform::default(),
                            Visibility::Hidden,
                            ChildOf(source),
                        ));
                    }
                }
            }
            GameEvent::Resolved { ent: source, target: Some(target), outcome, .. } => {
                if let Some((_, mut rope)) = ropes.iter_mut().find(|(_, rope)| (rope.source, rope.target) == (source, target)) {
                    rope.landed = Some(outcome == Outcome::Landed);
                }
            }
            _ => {}
        }
    }

    for (ent, mut rope) in &mut ropes {
        if rope.from.is_some() {
            continue;
        }
        let Some(&from) = joints_of(rope.source, &children, &names).get(FROM) else { continue };
        let Some(tint) = tint_under(from, &children, &cel, &cels, &standard, &standards) else { continue };
        let material = materials.add(RopeMaterial {
            base: StandardMaterial { base_color: tint, unlit: true, ..default() },
            extension: RopeExtension::default(),
        });
        rope.from = Some(from);
        commands.entity(ent).insert((
            Mesh3d(kit.mesh.clone()),
            MeshMaterial3d(material),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Moves each rope on through its pull and lays it from where its ends
/// stand this frame. Runs after transforms propagate, so it reads the
/// joints as this frame's animation left them and the loop keeps to the
/// body as it moves.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    time: Res<Time>,
    mut ropes: Query<(Entity, &mut Rope, &GlobalTransform, Option<&MeshMaterial3d<RopeMaterial>>, &mut Visibility)>,
    sliding: Query<(), With<Displacing>>,
    nodes: Query<&GlobalTransform>,
    tethers: Query<(&GlobalTransform, &Girth)>,
    (types, children, parents, names, worn): (Query<&EntityType>, Query<&Children>, Query<&ChildOf>, Query<&Name>, Query<Entity, With<Worn>>),
    mut materials: ResMut<Assets<RopeMaterial>>,
) {
    let now = Moment::ZERO + time.elapsed();
    let worn: HashSet<Entity> = worn.iter().collect();
    for (ent, mut rope, frame, material, mut visibility) in &mut ropes {
        if !nodes.contains(rope.target) {
            commands.entity(ent).despawn();
            continue;
        }
        let winding = rope.landed.is_none() && now < rope.until;
        let Some(stage) = rope.stage.next(winding, rope.landed, sliding.contains(rope.target), now) else {
            commands.entity(ent).despawn();
            continue;
        };
        rope.stage = stage;

        if rope.tether.is_none() {
            let body = types.get(rope.target).map_or("", |&typ| actor_name(typ));
            rope.tether = rig(rope.target, body, &children, &parents, &names, &worn)
                .and_then(|own| joints_of(own, &children, &names).get(TETHER).copied())
                .filter(|&node| tethers.contains(node));
        }
        let ends = (
            rope.tether.and_then(|node| tethers.get(node).ok()),
            rope.from.and_then(|node| nodes.get(node).ok()),
            material.and_then(|material| materials.get_mut(&material.0)),
        );
        let (Some((socket, girth)), Some(from), Some(mut material)) = ends else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let local = frame.affine().inverse();
        let scale = frame.scale().x.max(1e-6);
        let girth = Girth {
            middle: local.transform_point3(socket.transform_point(girth.middle)),
            wide: local.transform_vector3(socket.affine().transform_vector3(girth.wide)),
            deep: local.transform_vector3(socket.affine().transform_vector3(girth.deep)),
        };
        let down = local.transform_vector3(Vec3::NEG_Y).normalize_or(Vec3::NEG_Y);
        let start = local.transform_point3(from.translation());
        material.extension.shape = shape(start, girth, down, scale, stage.tight(now), stage.fallen(now));
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Moment = Moment::ZERO;

    fn later(ms: u64) -> Moment {
        AT + Duration::from_millis(ms)
    }

    #[test]
    fn a_rope_hangs_slack_while_its_pulse_winds_up_and_tightens_as_it_lands() {
        assert_eq!(Stage::Slack.next(true, None, false, AT), Some(Stage::Slack));
        assert_eq!(Stage::Slack.next(false, Some(true), false, AT), Some(Stage::Taut { since: AT }));
        assert_eq!(Stage::Slack.next(false, Some(true), true, AT), Some(Stage::Reeling), "its slide come with it");
    }

    #[test]
    fn a_taut_rope_reels_its_target_in_and_lets_go_as_the_slide_ends() {
        let taut = Stage::Taut { since: AT };
        assert_eq!(taut.next(false, Some(true), true, later(100)), Some(Stage::Reeling));
        assert_eq!(Stage::Reeling.next(false, Some(true), true, later(900)), Some(Stage::Reeling));
        assert_eq!(Stage::Reeling.next(false, Some(true), false, later(1200)), None);
        assert_eq!(taut.next(false, Some(true), false, later(100)), Some(taut), "waiting on the slide");
        assert_eq!(taut.next(false, Some(true), false, AT + TAUT_WAIT), None, "and letting go where none comes");
    }

    #[test]
    fn a_rope_whose_pulse_let_go_falls_away() {
        let dropping = Stage::Slack.next(false, Some(false), false, AT);
        assert_eq!(dropping, Some(Stage::Dropping { since: AT }));
        assert_eq!(Stage::Slack.next(false, None, false, AT), dropping, "and one whose pulse ended unheard");
        assert_eq!(dropping.unwrap().next(false, Some(false), false, AT + DROP), None);
    }

    fn girth() -> Girth {
        Girth { middle: Vec3::new(4.0, 1.0, 0.0), wide: Vec3::X * 0.3, deep: Vec3::Z * 0.2 }
    }

    #[test]
    fn slack_it_sags_and_hangs_loose_and_taut_it_runs_straight_to_a_closed_loop() {
        let start = Vec3::new(0.0, 0.2, 0.0);
        let slack = shape(start, girth(), Vec3::NEG_Y, 1.0, 0.0, 0.0);
        let taut = shape(start, girth(), Vec3::NEG_Y, 1.0, 1.0, 0.0);
        assert!(slack.end.w > 0.0, "slack, it sags");
        assert_eq!(taut.end.w, 0.0, "taut, it runs straight");
        assert!(slack.wide.length() > taut.wide.length(), "its loop closes as it tightens");
        assert!(slack.middle.y < taut.middle.y, "and rises from where it hung");
        let fallen = shape(start, girth(), Vec3::NEG_Y, 1.0, 0.0, 1.0);
        assert_eq!(fallen.start.w, 0.0, "fallen away, it is gone");
    }

    #[test]
    fn the_cord_meets_the_loop_on_its_side_toward_the_source() {
        let start = Vec3::new(0.0, 1.0, 0.0);
        let taut = shape(start, girth(), Vec3::NEG_Y, 1.0, 1.0, 0.0);
        let (end, middle) = (taut.end.truncate(), taut.middle.truncate());
        let a = taut.down.w;
        assert!((middle + taut.wide.truncate() * a.cos() + taut.deep.truncate() * a.sin() - end).length() < 1e-5, "on the loop, where it starts");
        assert!(end.distance(start) < middle.distance(start), "nearer the source than the loop's middle");
    }

    #[test]
    fn the_mesh_closes_every_ring_and_names_only_its_own_vertices() {
        let mesh = rope_mesh();
        let count = mesh.count_vertices() as u32;
        let Some(Indices::U32(indices)) = mesh.indices() else { panic!("u32 indices") };
        assert_eq!(indices.len(), 6 * SIDES * (CORD_RINGS - 1 + LOOP_RINGS));
        assert!(indices.iter().all(|&i| i < count));
    }
}
