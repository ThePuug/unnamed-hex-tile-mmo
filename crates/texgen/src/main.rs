//! Texture generator: each texture is a module that draws one tileable image
//! from a seed. The module is the source; the PNG under `assets/textures` is
//! its output, committed so the client runs without a generation step. An
//! asset holds several seeds stacked vertically, which the client loads as
//! a texture array and blends by world position so the repeat never lines
//! up. Which textures exist is the README; how one is made and reviewed is
//! AGENTS.md.

mod canvas;
mod color;
mod noise;
mod textures;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use image::{imageops, RgbaImage};

use textures::{Params, TEXTURES};

/// Seam contrast over the image's own strongest interior edge, past which
/// it does not tile. A tileable image sits at or under about 1.0; a hard seam
/// is many times that.
pub const SEAM_LIMIT: f32 = 1.5;

/// Seeds stacked into one asset. The client's loader splits the stack into
/// this many layers, so the two must agree.
pub const VARIANTS: u64 = 3;

#[derive(Parser)]
#[command(about = "Draws one tileable texture, or all of them, from a seed.")]
struct Args {
    /// Texture name from `--list`, or `all`.
    name: Option<String>,
    /// Print every texture with its brief and exit.
    #[arg(long)]
    list: bool,
    /// Edge length in pixels.
    #[arg(long, default_value_t = 256)]
    size: u32,
    /// First seed. The shipped asset starts at 0.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Seeds to draw, stacked vertically into one PNG. The shipped asset
    /// holds `VARIANTS`; 1 writes a single tile.
    #[arg(long, default_value_t = VARIANTS)]
    variants: u64,
    /// Where the texture PNG goes. Default: the workspace `assets/textures`.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Where each variant's tile and 2x2 proof sheet go. Default:
    /// `target/texgen`.
    #[arg(long)]
    proof: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.list {
        for t in TEXTURES {
            println!("{:<16} {}", t.name, t.brief);
        }
        return ExitCode::SUCCESS;
    }
    let Some(name) = args.name else {
        eprintln!("texgen: give a texture name or `all`; `--list` names them");
        return ExitCode::FAILURE;
    };
    let chosen: Vec<_> = if name == "all" {
        TEXTURES.iter().collect()
    } else {
        match TEXTURES.iter().find(|t| t.name == name) {
            Some(t) => vec![t],
            None => {
                eprintln!("texgen: no texture `{name}`; `--list` names them");
                return ExitCode::FAILURE;
            }
        }
    };
    if args.variants == 0 {
        eprintln!("texgen: --variants must be at least 1");
        return ExitCode::FAILURE;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let out = args.out.unwrap_or_else(|| root.join("assets/textures"));
    let proof = args.proof.unwrap_or_else(|| root.join("target/texgen"));
    for dir in [&out, &proof] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("texgen: cannot create {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    }

    let mut all_tile = true;
    for t in chosen {
        let mut stack = RgbaImage::new(args.size, args.size * args.variants as u32);
        for k in 0..args.variants {
            let params = Params { size: args.size, seed: args.seed + k };
            let canvas = (t.build)(&params);
            let tile = canvas.to_image();
            imageops::replace(&mut stack, &tile, 0, (k as u32 * args.size) as i64);
            let tile_path = proof.join(format!("{}-{k}.png", t.name));
            let sheet_path = proof.join(format!("{}-{k}.tiled.png", t.name));
            if let Err(e) = tile.save(&tile_path).and_then(|_| canvas.tiled(2).save(&sheet_path)) {
                eprintln!("texgen: {}: {e}", t.name);
                return ExitCode::FAILURE;
            }
            let seam = canvas.seam_ratio();
            let verdict = if seam <= SEAM_LIMIT {
                "tiles"
            } else {
                all_tile = false;
                "SEAM"
            };
            println!("{:<16} seed {:<3} seam {seam:.2} {verdict:<5} {}", t.name, params.seed, sheet_path.display());
        }
        let asset = out.join(format!("{}.png", t.name));
        if let Err(e) = stack.save(&asset) {
            eprintln!("texgen: {}: {e}", t.name);
            return ExitCode::FAILURE;
        }
        println!("{:<16} {} seeds stacked -> {}", t.name, args.variants, asset.display());
    }
    if all_tile {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
