//! The client page: the frame and what the process holds, the network, the
//! terrain about the player and what each kind of thing in the scene costs
//! to draw, the render passes, and the client's timers. The client
//! publishes a packet a topic twice a second (`client::plugins::diagnostics::publish`).

use eframe::egui;

use common::{glyphs::*, numfmt::{NumFmt, Overflow, Precision}};

use crate::{feed::Feed, widgets::*};

const TIME5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
const COUNT5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
const OVERRUN: NumFmt = NumFmt { width: 1, precision: Precision::Integer, overflow: Overflow::Clamp };

/// A frame against 144 and 60 frames a second.
const ALARM_FRAME: Alarm = Alarm { bands: &[(6.944, COLOR_NORMAL), (16.667, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };
const ALARM_FPS: Alarm = Alarm { bands: &[(60.0, COLOR_CRITICAL), (144.0, COLOR_WARN), (f64::INFINITY, COLOR_NORMAL)] };
/// A system's p95 against the frame.
const ALARM_TIMING: Alarm = Alarm { bands: &[(16.667, COLOR_NORMAL), (33.333, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };
/// A draw call costs the thread that encodes it, every frame, and a wood
/// that stands in thousands of them pays for every one.
const ALARM_DRAWS: Alarm = Alarm { bands: &[(500.0, COLOR_NORMAL), (2000.0, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };
const ALARM_BW: Alarm = Alarm { bands: &[(15360.0, COLOR_NORMAL), (20480.0, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };
const ALARM_MSG: Alarm = Alarm { bands: &[(15.0, COLOR_NORMAL), (20.0, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };

pub fn draw(ui: &mut egui::Ui, feed: &Feed, cw: f32, rh: f32) {
    let field = |name: &str| feed.latest(name);
    let bars = bar_count(cw);

    ui.columns(3, |cols| {
        draw_section(&mut cols[0], "FRAME", |ui| {
            const FPS: NumFmt = NumFmt { width: 3, precision: Precision::Integer, overflow: Overflow::Clamp };
            let fps = field("frame/fps");
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "FRAME"), COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", TIME5.fmt(field("frame/p95_ms")), "ms"), COLOR_DIM);
                s.spark(&feed.history("frame/p95_ms"), SparkScale::Fixed(33.0), &ALARM_FRAME);
                s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(feed.peak("frame/p95_ms", bars))), COLOR_DIM);
                s.half(&format!("ƒ{:<6}", FPS.fmt(fps)), ALARM_FPS.color(fps));
            });
        });

        // What the process holds: in RAM, promised by the OS (what runs out
        // when an allocation fails), and, in a debug build, the Rust heap
        // and its allocations of a megabyte or more.
        draw_section(&mut cols[0], "MEMORY", |ui| {
            const MB5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
            for (label, name) in [("WORKING", "process/memory_mb"), ("COMMIT", "process/committed_mb"), ("HEAP", "heap/rust_mb"), ("LARGE", "heap/large_mb")] {
                let history = feed.history(name);
                if history.is_empty() {
                    continue;
                }
                let ceiling = power_of_two_ceil(history.iter().copied().fold(0.0_f32, f32::max) as f64) as f32;
                seg_row(ui, cw, rh, |s| {
                    s.half(&fit_half(label), COLOR_DIM);
                    s.half(&format!("{:>5}MB", MB5.fmt(field(name))), COLOR_DIM);
                    s.spark(&history, SparkScale::Fixed(ceiling), &DIM);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, MB5.fmt(feed.peak(name, bars))), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[0], "NETWORK", |ui| {
            const NET_BPS: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
            const NET_MPS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:<2}NET  ", GLYPH_NET_DOWN), COLOR_DIM);
                s.half(&format!("{:>4}{:<3}", NET_BPS.fmt(field("network/bytes_per_sec")), "B/s"), COLOR_DIM);
                s.spark(&feed.history("network/bytes_per_sec"), SparkScale::Fixed(40960.0), &ALARM_BW);
                s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, NET_BPS.fmt(feed.peak("network/bytes_per_sec", bars))), COLOR_DIM);
            });
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:<2}MSG  ", GLYPH_NET_DOWN), COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", NET_MPS.fmt(field("network/messages_per_sec")), "/s"), COLOR_DIM);
                s.spark(&feed.history("network/messages_per_sec"), SparkScale::Fixed(40.0), &ALARM_MSG);
                s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, NET_MPS.fmt(feed.peak("network/messages_per_sec", bars))), COLOR_DIM);
            });
        });

        draw_section(&mut cols[0], "TERRAIN", |ui| {
            const COORD: NumFmt = NumFmt { width: 7, precision: Precision::Integer, overflow: Overflow::Clamp };
            let [q, r, z, wx, wy] = ["q", "r", "z", "wx", "wy"].map(|f| field(&format!("world/{f}")));
            seg_row(ui, cw, rh, |s| {
                s.quarter(&format!(" {:<2}", GLYPH_HEX), COLOR_DIM);
                for v in [q, r, z] {
                    s.half(&format!("{:<7}", COORD.fmt(v)), COLOR_DIM);
                }
                s.quarter(&format!(" {:<2}", GLYPH_GRID), COLOR_DIM);
                for v in [wx, wy] {
                    s.half(&format!("{:<7}", COORD.fmt(v)), COLOR_DIM);
                }
            });
            const LOD_TRIS: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
            const LOD_CT: NumFmt = NumFmt { width: 4, precision: Precision::Integer, overflow: Overflow::Suffix };
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "TILES"), COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(field("world/tiles"))), COLOR_DIM);
                s.half(&format!("{:>7}", "async"), COLOR_DIM);
                s.half(&format!("{:>4}msh", LOD_CT.fmt(field("terrain/async_mesh"))), COLOR_DIM);
            });
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "LoD"), COLOR_DIM);
                s.half(&format!("{:<2}{:<5}", GLYPH_TRIANGLES, LOD_TRIS.fmt(field("terrain/tris"))), COLOR_DIM);
                s.half(&format!("{:>4}chk", LOD_CT.fmt(field("terrain/chunks"))), COLOR_DIM);
            });
            let mut bands: Vec<u32> = feed.under("terrain/band/").filter_map(|(_, rest)| rest.strip_suffix("/tris")?.parse().ok()).collect();
            bands.sort();
            for r in bands {
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("  r={r:<3}"), COLOR_DIM);
                    s.half(&format!("{:<2}{:<5}", GLYPH_TRIANGLES, LOD_TRIS.fmt(field(&format!("terrain/band/{r}/tris")))), COLOR_DIM);
                    s.half(&format!("{:>4}chk", LOD_CT.fmt(field(&format!("terrain/band/{r}/chunks")))), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[1], "RENDER", |ui| {
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "ENTS"), COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(field("diag/entity_count"))), COLOR_DIM);
            });
            // The cover: what it draws, and what it draws in one go. A draw
            // costs the thread that encodes it; an instance in it costs
            // nothing more.
            for (what, draws, instances) in [("MODELS", "cover/model_draws", "cover/models"), ("CARDS", "cover/card_draws", "cover/cards")] {
                let draws = field(draws);
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("{what:>7}"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(instances))), COLOR_DIM);
                    s.half(&format!("{:>7}", "DRAWS"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(draws)), ALARM_DRAWS.color(draws));
                });
            }
            // What each group hands the rasteriser. The triangles are what a
            // pass is paid in; the draws beside them say whether they came
            // in one go.
            for (what, group) in [("GROUND", "terrain"), ("COVER", "cover"), ("ACTORS", "actors"), ("OTHER", "other")] {
                let draws = field(&format!("census/{group}/draws"));
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("{what:>7}"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(&format!("census/{group}/triangles")))), COLOR_DIM);
                    s.half(&format!("{:>7}", "DRAWS"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(draws)), ALARM_DRAWS.color(draws));
                });
            }
        });

        // Every pass Bevy times, heaviest first: what the GPU spent on it,
        // and what the thread encoding it spent. The frame is GPU-bound when
        // the gpu sum nears the frame time; when it falls well short, the
        // cpu column says whether the encoding took the rest.
        draw_section(&mut cols[1], "PASSES", |ui| {
            const PASS_MS: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
            let mut passes: Vec<(&str, f64, f64)> = Vec::new();
            for (name, rest) in feed.under("diag/render/") {
                let (pass, gpu) = match (rest.strip_suffix("/elapsed_cpu"), rest.strip_suffix("/elapsed_gpu")) {
                    (Some(pass), _) => (pass, false),
                    (_, Some(pass)) => (pass, true),
                    _ => continue,
                };
                let entry = match passes.iter_mut().find(|p| p.0 == pass) {
                    Some(entry) => entry,
                    None => {
                        passes.push((pass, 0.0, 0.0));
                        passes.last_mut().unwrap()
                    }
                };
                if gpu { entry.2 = field(name) } else { entry.1 = field(name) }
            }
            passes.sort_by(|a, b| (b.1 + b.2).total_cmp(&(a.1 + a.2)));
            let cpu: f64 = passes.iter().map(|p| p.1).sum();
            let gpu: f64 = passes.iter().map(|p| p.2).sum();
            seg_row(ui, cw, rh, |s| {
                s.wide(&format!("{:<31}", "all passes"), COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(cpu), "c"), ALARM_FRAME.color(cpu));
                s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(gpu), "g"), ALARM_FRAME.color(gpu));
            });
            for (pass, cpu, gpu) in passes.iter().take(10) {
                // The last path component names the pass; its tail is the
                // telling part.
                let leaf = pass.rsplit('/').next().unwrap_or(pass);
                let shown: String = leaf.chars().rev().take(31).collect::<Vec<_>>().into_iter().rev().collect();
                seg_row(ui, cw, rh, |s| {
                    s.wide(&format!("{shown:<31}"), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(*cpu), "c"), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", PASS_MS.fmt(*gpu), "g"), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[2], "TIMINGS", |ui| {
            let mut systems: Vec<&str> = feed.under("timings/").filter_map(|(_, rest)| rest.strip_suffix(".p95")).collect();
            systems.sort();
            if systems.is_empty() {
                seg_row(ui, cw, rh, |s| s.half("     --", COLOR_DIM));
            }
            for system in systems {
                let p95 = format!("timings/{system}.p95");
                seg_row(ui, cw, rh, |s| {
                    let shown: String = if system.chars().count() > 15 { system.chars().take(15).collect() } else { format!("{system:>15}") };
                    s.full(&shown, COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", TIME5.fmt(feed.latest(&p95)), "ms"), COLOR_DIM);
                    s.spark(&feed.history(&p95), SparkScale::Fixed(33.0), &ALARM_TIMING);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(feed.peak(&p95, bars))), COLOR_DIM);
                    // Publications whose p95 took a 60 fps frame's budget
                    let over = feed.over(&p95, bars, 16.667) as f64;
                    s.quarter(&format!("{}{:<2}", OVERRUN.fmt(over), GLYPH_OVERRUN), ANY.color(over));
                });
            }
        });
    });
}
