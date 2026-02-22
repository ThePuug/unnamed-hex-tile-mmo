use std::time::Instant;

use clap::{Parser, ValueEnum};
use image::{Rgb, RgbImage};
use rayon::prelude::*;
use common::elevation_color_rgb;
use terrain::Terrain;

const SQRT_3: f64 = 1.7320508075688772;

fn hex_to_pixel(q: f64, r: f64) -> (f64, f64) {
    (q + r * 0.5, r * SQRT_3 / 2.0)
}

fn pixel_to_hex(x: f64, y: f64) -> (f64, f64) {
    let r = y * 2.0 / SQRT_3;
    let q = x - r * 0.5;
    (q, r)
}

#[derive(Parser)]
#[command(name = "terrain-viewer", about = "Render terrain heightmaps to PNG")]
struct Cli {
    #[arg(long, default_value_t = 0)]
    center_q: i32,

    #[arg(long, default_value_t = 0)]
    center_r: i32,

    #[arg(long, default_value_t = 15000)]
    radius: i32,

    #[arg(long, default_value = "terrain.png")]
    output: String,

    #[arg(long, default_value_t = 2)]
    scale: i32,

    #[arg(long, default_value_t = 0)]
    seed: u64,

    #[arg(long, value_enum, default_value_t = Mode::Elevation)]
    mode: Mode,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    /// Height mapped to topographic color ramp
    Elevation,
    /// Gradient magnitude: white = steep, black = flat
    Slope,
}

fn elevation_color(height: i32) -> Rgb<u8> {
    let (r, g, b) = elevation_color_rgb(height);
    Rgb([r, g, b])
}

fn slope_color(q: i32, r: i32, terrain: &Terrain) -> Rgb<u8> {
    let h = terrain.get_height(q, r) as f64;
    let neighbors = [
        (q + 1, r), (q - 1, r),
        (q, r + 1), (q, r - 1),
        (q + 1, r - 1), (q - 1, r + 1),
    ];
    let max_diff = neighbors.iter()
        .map(|&(nq, nr)| (terrain.get_height(nq, nr) as f64 - h).abs())
        .fold(0.0f64, f64::max);

    let brightness = ((max_diff / 5.0).min(1.0) * 255.0) as u8;
    Rgb([brightness, brightness, brightness])
}

fn main() {
    let cli = Cli::parse();

    let terrain = Terrain::new(cli.seed);
    let scale = cli.scale.max(1);

    let radius_f = cli.radius as f64;
    let scale_f = scale as f64;
    let (center_x, center_y) = hex_to_pixel(cli.center_q as f64, cli.center_r as f64);
    let pixel_diameter = (radius_f * 2.0 / scale_f) as u32;
    let width = pixel_diameter;
    let height = pixel_diameter;

    eprintln!(
        "Generating {} mode: center=({},{}) radius={} scale={} seed={} -> {}x{} image",
        mode_name(cli.mode), cli.center_q, cli.center_r, cli.radius, scale, cli.seed, width, height
    );

    let start = Instant::now();

    let mode = cli.mode;
    let terrain_ref = &terrain;
    let pixels: Vec<(u32, u32, Rgb<u8>)> = (0..height)
        .into_par_iter()
        .flat_map(|py| {
            (0..width).map(move |px| {
                let cart_x = center_x - radius_f + (px as f64) * scale_f;
                let cart_y = center_y - radius_f + (py as f64) * scale_f;
                let (hq, hr) = pixel_to_hex(cart_x, cart_y);
                let q = hq.round() as i32;
                let r = hr.round() as i32;

                let color = match mode {
                    Mode::Elevation => elevation_color(terrain_ref.get_height(q, r)),
                    Mode::Slope => slope_color(q, r, terrain_ref),
                };

                (px, py, color)
            }).collect::<Vec<_>>()
        })
        .collect();

    let elapsed_eval = start.elapsed();
    let tile_count = (width as u64) * (height as u64);
    eprintln!("Evaluated {} tiles in {:.2}s ({:.0} tiles/sec)",
        tile_count,
        elapsed_eval.as_secs_f64(),
        tile_count as f64 / elapsed_eval.as_secs_f64()
    );

    let mut img = RgbImage::new(width, height);
    for (px, py, color) in pixels {
        img.put_pixel(px, py, color);
    }

    img.save(&cli.output).expect("Failed to save PNG");
    let total = start.elapsed();
    eprintln!("Saved {} ({:.2}s total)", cli.output, total.as_secs_f64());
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Elevation => "elevation",
        Mode::Slope => "slope",
    }
}
