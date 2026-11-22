//! Sound effects: the pools under `sfx/` in the game's assets, and what
//! of a fight is heard through them.
//!
//! A file declares itself in its Vorbis comments: `POOL`, the set it is
//! a variant of, and `GAIN`, the decibels that play the pool at its level
//! in the mix. The client reads every file once as it starts, draws a
//! pool as a bag — every variant once before any repeats, never the same
//! one twice running — and applies that gain and distance, nothing else.
//! A pool with no file is silent.
//!
//! Whatever wants a sound writes a [`Play`]. A sound with an actor is
//! heard from where that actor stands, by ears at the viewed actor
//! facing as the camera does; one with none is the interface's, and is
//! heard the same wherever the view is.
//!
//! A fight's sounds are read from the events its HUD reads ([`hear`]): a
//! skill's activation starting, and how each one resolved.

use std::collections::HashMap;
use std::path::Path;

use bevy::{
    audio::{SpatialScale, Volume},
    prelude::*,
};
use common_bevy::message::{AbilityType, Do, Event as GameEvent, Outcome};
use rand::{seq::SliceRandom, Rng};

use crate::{
    components::Viewed,
    systems::{closeup::CloseupCamera, combat::Heard},
};

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Play>();
        app.init_resource::<Pools>();
        app.add_systems(Startup, (load, spawn_ears));
        app.add_systems(Update, ((hear, play).chain(), listen));
    }
}

/// A sound to play now: one variant of `pool`, heard from where `at`
/// stands, or with no actor heard as the interface is.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Play {
    pub pool: &'static str,
    pub at: Option<Entity>,
}

impl Play {
    /// A key press the gate refuses, for the one who pressed it.
    pub fn refused() -> Self {
        Play { pool: REFUSE, at: None }
    }
}

const WINDUP: &str = "windup";
const INTERRUPT: &str = "interrupt";
const REFUSE: &str = "ui-refuse";

/// What struck: a blow is named by it and by the surface it met.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strike {
    Edge,
    Blunt,
}

impl Strike {
    /// A boot is blunt; every other strike is the weapon's edge.
    fn of(ability: AbilityType) -> Self {
        match ability {
            AbilityType::Kick => Strike::Blunt,
            _ => Strike::Edge,
        }
    }

    /// The blow on a body.
    fn lands(self) -> &'static str {
        match self {
            Strike::Edge => "hit-edge-flesh",
            Strike::Blunt => "hit-blunt-flesh",
        }
    }

    /// The blow on what was raised against it: a block is no sound of its
    /// own, it is the blow on the blocker's steel.
    fn blocked(self) -> &'static str {
        match self {
            Strike::Edge => "hit-edge-metal",
            Strike::Blunt => "hit-blunt-metal",
        }
    }

    /// The stroke through air with nothing at its end: a miss.
    fn whiff(self) -> &'static str {
        match self {
            Strike::Edge => "swing-edge",
            Strike::Blunt => "swing-blunt",
        }
    }
}

/// What is heard as `ent`'s `ability` resolves, and from whom: a blow or
/// a block from the one struck, a miss from the one who swung, an
/// interrupt from the one whose skill was ended. A wound's tick, a blow
/// with no damage, a rune letting a foe go and a skill its user
/// cancelled are silent.
fn resolved(ent: Entity, ability: AbilityType, target: Option<Entity>, damage: f32, outcome: Outcome, dot: bool) -> Option<Play> {
    let strike = Strike::of(ability);
    let (pool, at) = match outcome {
        Outcome::Landed if damage > 0.0 && !dot => (strike.lands(), target.unwrap_or(ent)),
        Outcome::Blocked => (strike.blocked(), target.unwrap_or(ent)),
        Outcome::Missed if ability.rune().is_none() => (strike.whiff(), ent),
        Outcome::Interrupted => (INTERRUPT, ent),
        Outcome::Landed | Outcome::Missed | Outcome::Cancelled => return None,
    };
    Some(Play { pool, at: Some(at) })
}

/// Asks for the sound of each skill begun and each one resolved. An
/// activation told again begins nothing ([`Heard`]), and one with no
/// length has no wind-up to hear.
pub fn hear(mut reader: MessageReader<Do>, mut heard: Local<Heard>, mut plays: MessageWriter<Play>) {
    for message in reader.read() {
        let play = match message.event {
            GameEvent::Activate { ent, activation } if !activation.length.is_zero() && heard.fresh(ent, &activation) => Some(Play { pool: WINDUP, at: Some(ent) }),
            GameEvent::Resolved { ent, ability, target, damage, outcome, dot } => resolved(ent, ability, target, damage, outcome, dot),
            _ => None,
        };
        if let Some(play) = play {
            plays.write(play);
        }
    }
}

/// The order a pool's variants are drawn in: every one once, then again
/// in a new order that does not open on the one just played.
struct Bag {
    order: Vec<usize>,
    next: usize,
}

impl Bag {
    fn new(variants: usize) -> Self {
        Bag { order: (0..variants).collect(), next: variants }
    }

    fn draw(&mut self, rng: &mut impl Rng) -> usize {
        if self.next >= self.order.len() {
            let last = self.order.last().copied();
            self.order.shuffle(rng);
            if self.order.len() > 1 && self.order.first().copied() == last {
                let other = rng.random_range(1..self.order.len());
                self.order.swap(0, other);
            }
            self.next = 0;
        }
        self.next += 1;
        self.order[self.next - 1]
    }
}

struct Pool {
    /// Decibels over the file that play the pool at its declared level.
    gain: f32,
    variants: Vec<Handle<AudioSource>>,
    bag: Bag,
}

#[derive(Resource, Default)]
struct Pools(HashMap<String, Pool>);

/// The comments of an Ogg Vorbis file, keys in capitals; none where the
/// bytes hold no comment header.
fn tags(bytes: &[u8]) -> HashMap<String, String> {
    const HEADER: &[u8] = b"\x03vorbis";
    let mut out = HashMap::new();
    let Some(start) = bytes.windows(HEADER.len()).position(|w| w == HEADER) else { return out };
    let mut at = start + HEADER.len();
    let word = |at: &mut usize| -> Option<usize> {
        let four: [u8; 4] = bytes.get(*at..*at + 4)?.try_into().ok()?;
        *at += 4;
        Some(u32::from_le_bytes(four) as usize)
    };
    let Some(vendor) = word(&mut at) else { return out };
    at += vendor;
    let Some(count) = word(&mut at) else { return out };
    for _ in 0..count {
        let Some(len) = word(&mut at) else { break };
        let Some(text) = bytes.get(at..at + len) else { break };
        at += len;
        if let Some((key, value)) = std::str::from_utf8(text).ok().and_then(|t| t.split_once('=')) {
            out.insert(key.to_ascii_uppercase(), value.to_string());
        }
    }
    out
}

/// Reads every sound under `sfx/` into the pool it declares. A file that
/// declares no pool or no gain is left out, and said.
fn load(mut pools: ResMut<Pools>, mut sources: ResMut<Assets<AudioSource>>) {
    let folder = Path::new(crate::ASSETS).join("sfx");
    let Ok(entries) = std::fs::read_dir(&folder) else {
        warn!("sfx: no sounds at {}; effects are silent", folder.display());
        return;
    };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "ogg")).collect();
    files.sort();
    for file in files {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let tags = tags(&bytes);
        let (Some(pool), Some(gain)) = (tags.get("POOL"), tags.get("GAIN").and_then(|g| g.parse::<f32>().ok())) else {
            warn!("sfx: {} declares no pool and gain", file.display());
            continue;
        };
        let pool = pools.0.entry(pool.clone()).or_insert_with(|| Pool { gain, variants: Vec::new(), bag: Bag::new(0) });
        pool.variants.push(sources.add(AudioSource { bytes: bytes.into() }));
        pool.bag = Bag::new(pool.variants.len());
    }
    info!("sfx: {} pools", pools.0.len());
}

/// World units within which a sound is heard at its full level; past it
/// the level falls by the square of the distance.
const FULL_WITHIN: f32 = 6.0;

/// World units between the two ears.
const EAR_GAP: f32 = 2.0;

/// Plays one variant of each pool asked for. A pool that is not there,
/// or an actor that is gone, plays nothing.
fn play(mut commands: Commands, mut plays: MessageReader<Play>, mut pools: ResMut<Pools>, bodies: Query<&Transform>) {
    let mut rng = rand::rng();
    for play in plays.read() {
        let Some(pool) = pools.0.get_mut(play.pool) else { continue };
        let source = pool.variants[pool.bag.draw(&mut rng)].clone();
        let settings = PlaybackSettings::DESPAWN.with_volume(Volume::Decibels(pool.gain));
        match play.at {
            None => {
                commands.spawn((AudioPlayer(source), settings));
            }
            Some(actor) => {
                let Ok(body) = bodies.get(actor) else { continue };
                // A root of its own, where the actor stood: the sound is
                // over before the actor has gone anywhere, and it outlasts
                // an actor that dies as it starts.
                commands.spawn((
                    AudioPlayer(source),
                    settings.with_spatial(true).with_spatial_scale(SpatialScale::new(1.0 / FULL_WITHIN)),
                    Transform::from_translation(body.translation),
                ));
            }
        }
    }
}

/// The ears every positioned sound is heard by.
#[derive(Component)]
struct Ears;

fn spawn_ears(mut commands: Commands) {
    commands.spawn((Ears, SpatialListener::new(EAR_GAP), Transform::default()));
}

/// Keeps the ears where the viewed actor stands, turned as the camera
/// is: the camera stands far off the fight, and ears there would hear
/// the player's own blows as distant; left and right are the screen's.
fn listen(
    mut ears: Query<&mut Transform, With<Ears>>,
    viewed: Query<&Transform, (With<Viewed>, Without<Ears>)>,
    camera: Query<&Transform, (With<Camera3d>, Without<CloseupCamera>, Without<Ears>)>,
) {
    let (Ok(mut ears), Ok(viewed), Ok(camera)) = (ears.single_mut(), viewed.single(), camera.single()) else { return };
    let placed = Transform { translation: viewed.translation, rotation: camera.rotation, scale: Vec3::ONE };
    if *ears != placed {
        *ears = placed;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::*;
    use common_bevy::{components::activation::Activation, moment::Moment};

    const OUTCOMES: [Outcome; 5] = [Outcome::Landed, Outcome::Blocked, Outcome::Missed, Outcome::Interrupted, Outcome::Cancelled];

    fn pool_of(outcome: Outcome, ability: AbilityType) -> Option<&'static str> {
        let (user, target) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        resolved(user, ability, Some(target), 10.0, outcome, false).map(|play| play.pool)
    }

    #[test]
    fn every_outcome_but_a_cancel_has_a_pool_of_its_own() {
        for strike in [AbilityType::Frenzy, AbilityType::Kick] {
            let pools: Vec<_> = OUTCOMES.iter().filter_map(|&outcome| pool_of(outcome, strike)).collect();
            assert_eq!(pools.len(), 4, "all but the cancel sound");
            assert_eq!(pools.iter().collect::<HashSet<_>>().len(), 4, "and no two alike: {pools:?}");
            assert!(!pools.contains(&WINDUP) && !pools.contains(&REFUSE), "nor like a wind-up or a refused press");
        }
        assert_eq!(pool_of(Outcome::Cancelled, AbilityType::Frenzy), None);
    }

    #[test]
    fn each_sound_comes_from_the_one_it_happened_to() {
        let (user, target) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let from = |outcome| resolved(user, AbilityType::Frenzy, Some(target), 10.0, outcome, false).and_then(|play| play.at);
        assert_eq!(from(Outcome::Landed), Some(target));
        assert_eq!(from(Outcome::Blocked), Some(target));
        assert_eq!(from(Outcome::Missed), Some(user));
        assert_eq!(from(Outcome::Interrupted), Some(user));
        assert_eq!(Play::refused().at, None, "a refused press is the interface's");
    }

    #[test]
    fn what_is_no_blow_is_silent() {
        let (user, target) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        assert_eq!(resolved(user, AbilityType::Frenzy, Some(target), 3.0, Outcome::Landed, true), None, "a wound's tick");
        assert_eq!(resolved(user, AbilityType::Yank, Some(target), 0.0, Outcome::Landed, false), None, "a pull");
        assert_eq!(resolved(user, AbilityType::Drag, Some(target), 0.0, Outcome::Missed, false), None, "a rune letting go");
    }

    #[derive(Resource, Default)]
    struct Asked(usize);

    fn count(mut plays: MessageReader<Play>, mut asked: ResMut<Asked>) {
        asked.0 += plays.read().count();
    }

    #[test]
    fn an_activation_told_again_sounds_once() {
        let mut app = App::new();
        app.add_message::<Do>();
        app.add_message::<Play>();
        app.init_resource::<Asked>();
        app.add_systems(Update, (hear, count).chain());
        let actor = app.world_mut().spawn_empty().id();
        let activation = Activation { ability: AbilityType::Overpower, target: None, began: Moment::from_millis(5_000), length: Duration::from_secs(2) };
        let told = |app: &mut App, activation| {
            app.world_mut().write_message(Do { event: GameEvent::Activate { ent: actor, activation } });
            app.update();
            app.world().resource::<Asked>().0
        };

        assert_eq!(told(&mut app, activation), 1);
        assert_eq!(told(&mut app, activation), 1, "told again, it begins nothing");
        assert_eq!(told(&mut app, Activation { length: Duration::ZERO, began: Moment::from_millis(9_000), ..activation }), 1, "one that takes hold at once has no wind-up");
        assert_eq!(told(&mut app, Activation { began: Moment::from_millis(12_000), ..activation }), 2, "the next one sounds");
    }

    #[test]
    fn a_bag_plays_every_variant_before_any_again_and_none_twice_running() {
        let mut rng = rand::rng();
        let mut bag = Bag::new(6);
        let drawn: Vec<usize> = (0..600).map(|_| bag.draw(&mut rng)).collect();
        for pass in drawn.chunks(6) {
            assert_eq!(pass.iter().collect::<HashSet<_>>().len(), 6);
        }
        assert!(drawn.windows(2).all(|w| w[0] != w[1]));
        assert_eq!(Bag::new(1).draw(&mut rng), 0, "a pool of one is that one");
    }

    #[test]
    fn a_file_declares_its_pool_and_gain() {
        let mut bytes = b"OggS....\x01vorbis....OggS..".to_vec();
        bytes.extend(b"\x03vorbis");
        bytes.extend(6u32.to_le_bytes());
        bytes.extend(b"ffmpeg");
        bytes.extend(2u32.to_le_bytes());
        for comment in ["POOL=windup", "gain=-16.2"] {
            bytes.extend((comment.len() as u32).to_le_bytes());
            bytes.extend(comment.as_bytes());
        }
        let read = tags(&bytes);
        assert_eq!(read.get("POOL").map(String::as_str), Some("windup"));
        assert_eq!(read.get("GAIN").map(String::as_str), Some("-16.2"));
        assert!(tags(b"OggS and nothing after").is_empty());
        assert!(tags(&bytes[..bytes.len() - 4]).contains_key("POOL"), "a header cut short gives what it held");
    }

    /// Every pool the client asks for is one the assets declare.
    #[test]
    fn every_pool_asked_for_is_in_the_assets() {
        let folder = Path::new(crate::ASSETS).join("sfx");
        let declared: HashSet<String> = std::fs::read_dir(&folder)
            .expect("the assets' sfx folder")
            .flatten()
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|bytes| tags(&bytes).remove("POOL"))
            .collect();
        let mut asked = vec![WINDUP, INTERRUPT, REFUSE];
        for strike in [Strike::Edge, Strike::Blunt] {
            asked.extend([strike.lands(), strike.blocked(), strike.whiff()]);
        }
        for pool in asked {
            assert!(declared.contains(pool), "no file under sfx/ declares the pool {pool}");
        }
    }
}
