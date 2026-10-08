//! The server page: its frame and fixed tick, memory, network and input
//! guard; the async pipelines and system timings; the world and the
//! event stack's caches. The server publishes a snapshot topic, `server`,
//! and its timings, `timings`, every two seconds.

use eframe::egui;

use common::{glyphs::*, numfmt::{NumFmt, Overflow, Precision}};

use crate::{feed::Feed, widgets::*};

/// Timing values, ms.
const TIME5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
/// Counts.
const COUNT5: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
/// Byte rates and sizes.
const RATE5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Suffix };
const OVERRUN: NumFmt = NumFmt { width: 1, precision: Precision::Integer, overflow: Overflow::Clamp };

/// A tick or system against the fixed tick's budget.
const ALARM_TIMING: Alarm = Alarm { bands: &[(100.0, COLOR_NORMAL), (125.0, COLOR_WARN), (f64::INFINITY, COLOR_CRITICAL)] };

/// INV-007: the credit a client's own timing is spent against. It falling
/// is a client ahead of real time; clamped milliseconds are the movement
/// refused for it.
const ALARM_CREDIT: Alarm = Alarm { bands: &[(10.0, COLOR_CRITICAL), (50.0, COLOR_WARN), (f64::INFINITY, COLOR_DIM)] };

pub fn draw(ui: &mut egui::Ui, feed: &Feed, cw: f32, rh: f32) {
    let field = |name: &str| feed.latest(&format!("server/{name}"));
    let history = |name: &str| feed.history(&format!("server/{name}"));
    let bars = bar_count(cw);
    let peak = |name: &str| feed.peak(&format!("server/{name}"), bars);
    let sum = |name: &str| feed.sum(&format!("server/{name}"), bars);
    // `tick_count` is summed and reset each flush, so a publication
    // carries the ticks of its interval, not a running total.
    let (_, interval) = feed.clock("server");
    let ticks_per_sec = if interval > 0.0 { field("tick_count") / interval } else { 0.0 };

    ui.columns(3, |cols| {
        draw_section(&mut cols[0], "SYSTEM", |ui| {
            for (label, value, overruns, rate) in [("FRAME", "frame_peak_ms", "frame_overruns", None), ("TICK", "tick_peak_ms", "tick_overruns", Some(ticks_per_sec))] {
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("{label:>7}"), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", TIME5.fmt(field(value)), "ms"), COLOR_DIM);
                    s.spark(&history(value), SparkScale::Fixed(125.0), &ALARM_TIMING);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(peak(value))), COLOR_DIM);
                    let over = sum(overruns);
                    s.quarter(&format!("{}{:<2}", OVERRUN.fmt(over), GLYPH_OVERRUN), ANY.color(over));
                    // The rate the fixed schedule is actually holding
                    if let Some(rate) = rate {
                        s.half(&format!("{:>5}{:<2}", TIME5.fmt(rate), "/s"), COLOR_DIM);
                    }
                });
            }
            // The process, and the Map's share of it
            let mem_ceiling = power_of_two_ceil(history("memory_mb").iter().copied().fold(0.0_f32, f32::max) as f64) as f32;
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "MEM"), COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", RATE5.fmt(field("memory_mb")), "MB"), COLOR_DIM);
                s.spark(&history("memory_mb"), SparkScale::Fixed(mem_ceiling), &DIM);
                s.half("    map", COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", RATE5.fmt(field("memory_map_mb")), "MB"), COLOR_DIM);
            });
            for (glyph, value) in [(GLYPH_NET_UP, "net_sent_bps"), (GLYPH_NET_DOWN, "net_recv_bps")] {
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("{glyph:<2}NET  "), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", RATE5.fmt(field(value)), "Bs"), COLOR_DIM);
                    s.spark(&history(value), SparkScale::Auto, &DIM);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, RATE5.fmt(peak(value))), COLOR_DIM);
                });
            }
            // Each reliable channel: the queue behind it, and how full its
            // buffer is
            const CHAN_QUEUE: NumFmt = NumFmt { width: 5, precision: Precision::Integer, overflow: Overflow::Suffix };
            const CHAN_BUF: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Clamp };
            for (label, queue, buf) in [("ORD", "net_ord_queue", "net_ord_buf_pct"), ("UNORD", "net_unord_queue", "net_unord_buf_pct")] {
                seg_row(ui, cw, rh, |s| {
                    s.half(&format!("{label:>7}"), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", CHAN_QUEUE.fmt(field(queue)), "B"), COLOR_DIM);
                    s.spark(&history(queue), SparkScale::Auto, &DIM);
                    s.half(&format!("{:>7}", "buf%"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", CHAN_BUF.fmt(field(buf))), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[0], "INPUT", |ui| {
            const PCT5: NumFmt = NumFmt { width: 5, precision: Precision::Collapsing, overflow: Overflow::Clamp };
            let credit = field("input.credit_pct");
            seg_row(ui, cw, rh, |s| {
                s.half(" CREDIT", COLOR_DIM);
                s.half(&format!("{:>5}{:<2}", PCT5.fmt(credit), " %"), ALARM_CREDIT.color(credit));
                s.spark(&history("input.credit_pct"), SparkScale::Fixed(100.0), &ALARM_CREDIT);
                s.half("  clamp", COLOR_DIM);
                let clamped = sum("input.clamped_ms");
                s.half(&format!("{:>5}{:<2}", TIME5.fmt(clamped), "ms"), ANY.color(clamped));
            });
            let violations = sum("input.violations");
            let disconnects = field("input.disconnects");
            seg_row(ui, cw, rh, |s| {
                s.half("   VIOL", COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(violations)), ANY.color(violations));
                s.spark(&history("input.violations"), SparkScale::Auto, &ANY);
                s.half("   drop", COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(field("input.drops"))), COLOR_DIM);
                s.half("     dc", COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(disconnects)), ANY.color(disconnects));
            });
        });

        draw_section(&mut cols[1], "ASYNC", |ui| {
            // Each pipeline: what one task costs, then how many slots of its
            // budget are spent and how much work is behind them. A full
            // queue is not a fault; a full queue with a growing wait is.
            for (label, pipeline) in [("CHUNK", "chunk"), ("SUMMRY", "summary")] {
                let dur = format!("{pipeline}.dur_ms");
                let queue = format!("{pipeline}.in_flight");
                let budget = field(&format!("{pipeline}.budget"));
                seg_row(ui, cw, rh, |s| {
                    s.half(&fit_half(label), COLOR_DIM);
                    s.half(&format!("{:>5}{:<2}", TIME5.fmt(field(&dur)), "ms"), COLOR_DIM);
                    s.spark(&history(&dur), SparkScale::Auto, &DIM);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(peak(&dur))), COLOR_DIM);
                });
                let spent = budget > 0.0 && field(&queue) >= budget;
                seg_row(ui, cw, rh, |s| {
                    s.half("  queue", COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(&queue))), if spent { COLOR_WARN } else { COLOR_DIM });
                    s.spark(&history(&queue), SparkScale::Fixed(budget.max(1.0) as f32), &DIM);
                    s.half("   wait", COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(&format!("{pipeline}.pending")))), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[1], "TIMINGS", |ui| {
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
                    // p95 per invocation, over each interval
                    s.half(&format!("{:>5}{:<2}", TIME5.fmt(feed.latest(&p95)), "ms"), COLOR_DIM);
                    s.spark(&feed.history(&p95), SparkScale::Fixed(125.0), &ALARM_TIMING);
                    s.half(&format!("{:<2}{:<5}", GLYPH_PEAK, TIME5.fmt(feed.peak(&p95, bars))), COLOR_DIM);
                    // Intervals whose p95 overran the tick
                    let over = feed.over(&p95, bars, 125.0) as f64;
                    s.quarter(&format!("{}{:<2}", OVERRUN.fmt(over), GLYPH_OVERRUN), ANY.color(over));
                    s.half(&format!("n={:<5}", COUNT5.fmt(feed.sum(&format!("timings/{system}.n"), bars))), COLOR_DIM);
                });
            }
        });

        draw_section(&mut cols[2], "WORLD", |ui| {
            seg_row(ui, cw, rh, |s| {
                for (label, value) in [("#PLR", "connected_players"), ("#NPC", "npc_count"), ("#HEX", "loaded_hexes")] {
                    s.half(&format!("{label:>7}"), COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(value))), COLOR_DIM);
                }
            });
            // The transport's clients beside the lobby's players: the two
            // disagreeing is a connection half torn down.
            let players = field("connected_players");
            let clients = field("net_clients");
            seg_row(ui, cw, rh, |s| {
                s.half(&format!("{:>7}", "#CONN"), COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(clients)), if clients == players { COLOR_DIM } else { COLOR_WARN });
                s.half(&format!("{:>7}", "#SPWN"), COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(field("spawners.active"))), COLOR_DIM);
            });
        });

        draw_section(&mut cols[2], "EVENTS", |ui| {
            let percent = |hits: f64, misses: f64| if hits + misses > 0.0 { ((hits / (hits + misses) * 100.0) as u32).min(99) } else { 0 };
            // The composite: tiles cached, and how often a tile was
            seg_row(ui, cw, rh, |s| {
                s.half("       ", COLOR_DIM);
                s.half(" cached", COLOR_DIM);
                s.half(&format!("{:>5}  ", COUNT5.fmt(field("evt.visible"))), COLOR_DIM);
                s.half(&format!("{:<2}{:>2}%  ", GLYPH_CACHE, percent(field("evt.tile_hits"), field("evt.tile_misses"))), COLOR_DIM);
            });
            // Each layer, in the order the stack evaluates them: the server
            // names a layer's fields when the stack first reports it and
            // publishes in that order.
            for layer in feed.under("server/evt.").filter_map(|(_, rest)| rest.strip_suffix(".index")) {
                let cells = percent(field(&format!("evt.{layer}.cell_hits")), field(&format!("evt.{layer}.cell_misses")));
                seg_row(ui, cw, rh, |s| {
                    s.half(&fit_half(layer), COLOR_DIM);
                    s.half("indexed", COLOR_DIM);
                    s.half(&format!("{:>5}  ", COUNT5.fmt(field(&format!("evt.{layer}.index")))), COLOR_DIM);
                    s.half(&format!("{:<2}{:>2}%  ", GLYPH_CACHE, cells), COLOR_DIM);
                });
            }
        });
    });
}
