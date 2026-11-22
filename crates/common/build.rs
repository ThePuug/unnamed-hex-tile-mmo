//! Takes what the assets declare and the game reads as code: how far ahead
//! of the player's feet each gather lands its work, from what the player's
//! clips declare (`gathering::CHOP_REACH` and the rest), and every den
//! model's pieces, from their nodes (`den::MODELS`), so a model
//! re-exported moves them on the next build; and the kit, every set and
//! piece worn or wielded and what each seed makes of it, from the table
//! beside the models (`kit::SETS`, `kit::PIECES`).

use std::{env, fs, path::Path};

const CLIPS: &str = "../../assets/actors/player-basic.glb";
const MODELS: &str = "../../assets/models";

/// A GLB's JSON chunk: it follows a 12-byte header and its own length and
/// type.
fn gltf(path: &str) -> serde_json::Value {
    println!("cargo:rerun-if-changed={path}");
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("{path}: {error}; the assets submodule must be checked out"));
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + length]).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn reaches() -> String {
    let json = gltf(CLIPS);
    let declared = json["nodes"].as_array().expect("nodes").iter()
        .find_map(|node| node["extras"].get("animgen"))
        .expect("the armature node's animgen extras");
    let reach = |clip: &str| declared[clip]["reach"].as_f64().unwrap_or_else(|| panic!("{CLIPS}: {clip} declares no reach")) as f32;
    format!(
        "pub const CHOP_REACH: f32 = {:?};
pub const MINE_REACH: f32 = {:?};
pub const PICKUP_REACH: f32 = {:?};
",
        reach("chop"), reach("mine"), reach("pickup"),
    )
}

/// The pieces of each scene of the den model at `path`, as Rust.
fn scenes(path: &str) -> String {
    let json = gltf(path);
    let nodes = json["nodes"].as_array().expect("nodes");
    let mut out = String::from("&[");
    for scene in json["scenes"].as_array().expect("scenes") {
        out.push_str("&[");
        for index in scene["nodes"].as_array().expect("a scene's nodes") {
            let index = index.as_u64().unwrap() as usize;
            let node = &nodes[index];
            let name = node["name"].as_str().unwrap_or("?");
            let mesh = node["mesh"].as_u64().unwrap_or_else(|| panic!("{path}: `{name}` has no mesh"));
            let number = |value: &serde_json::Value| value.as_f64().unwrap_or(0.0) as f32;
            let at = &node["translation"];
            let (x, z) = (number(&at[0]), number(&at[2]));
            // Turned about the vertical only: its quaternion's y and w
            let turn = &node["rotation"];
            let yaw = if turn.is_array() { 2.0 * number(&turn[1]).atan2(number(&turn[3])) } else { 0.0 };
            let extras = &node["extras"];
            let lies = match extras["ground"].as_str() {
                Some("lie") => true,
                Some("stand") => false,
                _ => panic!("{path}: `{name}` says neither that it lies nor that it stands"),
            };
            let slope = extras["slope"].as_f64().unwrap_or_else(|| panic!("{path}: `{name}` names no slope")) as f32;
            let solid: Vec<f32> = extras["solid"].as_array().map_or(Vec::new(), |run| run.iter().map(number).collect());
            assert!(solid.len() % 4 == 0, "{path}: `{name}`'s solid is not a run of fours");
            let circles: Vec<String> = solid.chunks(4).map(|c| format!("[{:?}, {:?}, {:?}, {:?}]", c[0], c[1], c[2], c[3])).collect();
            out.push_str(&format!(
                "Piece {{ mesh: {mesh}, at: [{x:?}, {z:?}], yaw: {yaw:?}, lies: {lies}, slope: {slope:?}, solid: &[{}] }},",
                circles.join(", "),
            ));
        }
        out.push_str("],");
    }
    out.push(']');
    out
}

fn dens() -> String {
    println!("cargo:rerun-if-changed={MODELS}");
    let mut stems: Vec<String> = fs::read_dir(MODELS)
        .unwrap_or_else(|error| panic!("{MODELS}: {error}; the assets submodule must be checked out"))
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            Some(name.strip_prefix("den-")?.strip_suffix("-active.glb")?.to_string())
        })
        .collect();
    stems.sort();
    let mut out = String::from("pub static MODELS: &[Model] = &[\n");
    for stem in stems {
        out.push_str(&format!(
            "    Model {{ stem: {stem:?}, active: {}, cleared: {} }},\n",
            scenes(&format!("{MODELS}/den-{stem}-active.glb")),
            scenes(&format!("{MODELS}/den-{stem}-cleared.glb")),
        ));
    }
    out.push_str("];\n");
    out
}

/// `kebab-case` as the variant it names: `main-hand` is `MainHand`.
fn variant(name: &str) -> String {
    name.split('-').map(|word| word[..1].to_uppercase() + &word[1..]).collect()
}

/// The kit as Rust: every set with each seed's colours, and every piece
/// with its set, its slots and its shapes.
fn kit() -> String {
    let path = format!("{MODELS}/kit.json");
    println!("cargo:rerun-if-changed={path}");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}; the assets submodule must be checked out"));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
    let sets = json["sets"].as_array().expect("sets");
    let mut out = String::from("pub static SETS: &[Set] = &[\n");
    for set in sets {
        let seeds: Vec<String> = set["seeds"].as_array().expect("a set's seeds").iter()
            .map(|seed| {
                let materials: Vec<String> = seed.as_object().expect("a seed's materials").iter()
                    .map(|(name, rgb)| {
                        let v = |k: usize| rgb[k].as_f64().unwrap_or_else(|| panic!("{path}: {name} is no colour")) as f32;
                        format!("({name:?}, [{:?}, {:?}, {:?}])", v(0), v(1), v(2))
                    })
                    .collect();
                format!("&[{}]", materials.join(", "))
            })
            .collect();
        out.push_str(&format!(
            "    Set {{ name: {:?}, kind: Kind::{}, seeds: &[{}] }},\n",
            set["name"].as_str().expect("a set's name"),
            variant(set["kind"].as_str().expect("a set's kind")),
            seeds.join(", "),
        ));
    }
    out.push_str("];\npub static PIECES: &[Piece] = &[\n");
    for piece in json["pieces"].as_array().expect("pieces") {
        let name = piece["name"].as_str().expect("a piece's name");
        let of = piece["set"].as_str().expect("a piece's set");
        let set = sets.iter().position(|s| s["name"] == of).unwrap_or_else(|| panic!("{path}: {name} is of no set, `{of}`"));
        let fills: Vec<String> = piece["fills"].as_array().expect("a piece's slots").iter()
            .map(|slot| format!("Slot::{}", variant(slot.as_str().expect("a slot's name"))))
            .collect();
        out.push_str(&format!(
            "    Piece {{ name: {name:?}, set: {set}, fills: &[{}], shapes: {} }},\n",
            fills.join(", "),
            piece["shapes"].as_u64().expect("a piece's shapes"),
        ));
    }
    out.push_str("];\n");
    out
}

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    fs::write(Path::new(&out).join("reaches.rs"), reaches()).unwrap();
    fs::write(Path::new(&out).join("dens.rs"), dens()).unwrap();
    fs::write(Path::new(&out).join("kit.rs"), kit()).unwrap();
}
