//! Takes how far ahead of the player's feet each gather lands its work
//! from what the player's clips declare (`gathering::CHOP_REACH` and the
//! rest), so a clip re-exported moves the reach on the next build.

use std::{env, fs, path::Path};

const CLIPS: &str = "../../assets/actors/player-basic.glb";

fn main() {
    println!("cargo:rerun-if-changed={CLIPS}");
    let bytes = fs::read(CLIPS).unwrap_or_else(|error| panic!("{CLIPS}: {error}; the assets submodule must be checked out"));
    // A GLB is a 12-byte header, then its JSON chunk: length, type, data
    let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + length]).expect("the GLB's JSON chunk");
    let declared = json["nodes"].as_array().expect("nodes").iter()
        .find_map(|node| node["extras"].get("animgen"))
        .expect("the armature node's animgen extras");
    let reach = |clip: &str| declared[clip]["reach"].as_f64().unwrap_or_else(|| panic!("{CLIPS}: {clip} declares no reach")) as f32;
    let source = format!(
        "pub const CHOP_REACH: f32 = {:?};
pub const MINE_REACH: f32 = {:?};
pub const PICKUP_REACH: f32 = {:?};
",
        reach("chop"), reach("mine"), reach("pickup"),
    );
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("reaches.rs"), source).unwrap();
}
