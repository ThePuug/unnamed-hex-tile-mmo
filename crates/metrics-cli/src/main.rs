//! The metrics the game's processes publish, kept and read from the
//! command line.
//!
//! `metrics serve` runs the recorder: it joins every source's multicast
//! group and keeps the last `KEEP_SECS` of every field it hears, named
//! `source/topic/field`. Every other command asks the running recorder
//! over `CONTROL` and prints its answer. The subscriptions say which fields
//! `snapshot` and `stats` report; with none, or with `--all`, they report
//! every field. The recorder holds them for as long as it runs.
//!
//! Only the client publishes to a group so far; the server still sends to
//! the console alone.

use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::{SocketAddrV4, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use common::metrics::{self, MetricsPacket};

/// Every source the recorder joins, by the name its fields go under.
const SOURCES: [(&str, SocketAddrV4); 1] = [("client", metrics::CLIENT_GROUP)];

/// Where the recorder takes commands.
const CONTROL: &str = "127.0.0.1:5110";

/// How long the recorder keeps a value: ten minutes.
const KEEP_SECS: f64 = 600.0;

/// Arguments travel to the recorder joined by this, one command a line.
const SEPARATOR: char = '\x1f';

#[derive(Parser)]
#[command(name = "metrics", about = "Record the game's metrics and read them back")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the recorder in this process until `metrics stop`.
    Serve,
    /// Stop the running recorder.
    Stop,
    /// Report the fields matching each pattern (`*` matches anything).
    Subscribe { patterns: Vec<String> },
    /// Stop reporting the fields matching each pattern exactly as it was
    /// subscribed, or every pattern with `--all`.
    Unsubscribe {
        patterns: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Every field heard, with its newest value; `*` marks the subscribed.
    List { pattern: Option<String> },
    /// A page's fields over the last `window` seconds, each drawn as a
    /// line; with no page named, the pages there are.
    Page {
        name: Option<String>,
        #[arg(long, default_value_t = 60.0)]
        window: f64,
    },
    /// The last `count` publications side by side, `every` publications
    /// apart (a publication is half a second for the client).
    Snapshot {
        #[arg(default_value_t = 1)]
        count: usize,
        #[arg(long, default_value_t = 1)]
        every: usize,
        /// Every field, whatever is subscribed.
        #[arg(long)]
        all: bool,
    },
    /// Count, newest, least, mean, p95 and peak of each field over the last
    /// `window` seconds.
    Stats {
        #[arg(long, default_value_t = 60.0)]
        window: f64,
        /// Every field, whatever is subscribed.
        #[arg(long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Command::Serve = cli.command {
        serve();
        return;
    }
    let mut stream = match TcpStream::connect(CONTROL) {
        Ok(stream) => stream,
        Err(_) => {
            eprintln!("no recorder at {CONTROL}: start one with `metrics serve`");
            std::process::exit(1);
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let line = args.join(&SEPARATOR.to_string());
    if writeln!(stream, "{line}").is_err() {
        eprintln!("the recorder at {CONTROL} closed before it was asked");
        std::process::exit(1);
    }
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    print!("{answer}");
}

/// One value as the recorder heard it.
struct Sample {
    /// Seconds since the recorder started.
    at: f64,
    value: f32,
}

#[derive(Default)]
struct Recorder {
    subscriptions: Vec<String>,
    fields: BTreeMap<String, VecDeque<Sample>>,
    /// When each publication of each source arrived, by the publisher's
    /// timestamp, oldest first.
    publications: BTreeMap<&'static str, VecDeque<(f64, f64)>>,
}

impl Recorder {
    fn hear(&mut self, source: &'static str, packet: MetricsPacket, at: f64) {
        let publications = self.publications.entry(source).or_default();
        if publications.back().is_none_or(|&(ts, _)| ts != packet.timestamp_secs) {
            publications.push_back((packet.timestamp_secs, at));
        }
        while publications.front().is_some_and(|&(_, heard)| heard < at - KEEP_SECS) {
            publications.pop_front();
        }
        for (name, value) in packet.fields {
            let samples = self.fields.entry(format!("{source}/{}/{name}", packet.group)).or_default();
            samples.push_back(Sample { at, value });
            while samples.front().is_some_and(|s| s.at < at - KEEP_SECS) {
                samples.pop_front();
            }
        }
    }

    fn reported(&self, all: bool) -> impl Iterator<Item = (&String, &VecDeque<Sample>)> {
        let every = all || self.subscriptions.is_empty();
        self.fields.iter().filter(move |(name, _)| every || self.subscriptions.iter().any(|p| glob(p, name)))
    }

    fn answer(&mut self, command: Command, now: f64) -> String {
        match command {
            Command::Serve | Command::Stop => String::new(),
            Command::Subscribe { patterns } => {
                for pattern in patterns {
                    if !self.subscriptions.contains(&pattern) {
                        self.subscriptions.push(pattern);
                    }
                }
                self.describe_subscriptions()
            }
            Command::Unsubscribe { patterns, all } => {
                self.subscriptions.retain(|p| !all && !patterns.contains(p));
                self.describe_subscriptions()
            }
            Command::List { pattern } => {
                let rows: Vec<Vec<String>> = self
                    .fields
                    .iter()
                    .filter(|(name, _)| pattern.as_ref().is_none_or(|p| glob(p, name)))
                    .map(|(name, samples)| {
                        let subscribed = self.subscriptions.iter().any(|p| glob(p, name));
                        let newest = samples.back().map_or("-".into(), |s| value(s.value as f64));
                        vec![name.clone(), newest, if subscribed { "*".into() } else { String::new() }]
                    })
                    .collect();
                if rows.is_empty() {
                    return "nothing heard yet\n".into();
                }
                table(&[], &rows)
            }
            Command::Snapshot { count, every, all } => self.snapshot(count.max(1), every.max(1), all, now),
            Command::Stats { window, all } => self.stats(window, all, now),
            Command::Page { name: None, .. } => PAGES.iter().map(|(name, patterns)| format!("{name:<8} {}
", patterns.join("  "))).collect(),
            Command::Page { name: Some(name), window } => match PAGES.iter().find(|(page, _)| *page == name) {
                Some((_, patterns)) => self.page(patterns, window, now),
                None => format!("no page {name}: `metrics page` lists them
"),
            },
        }
    }

    fn describe_subscriptions(&self) -> String {
        if self.subscriptions.is_empty() {
            return "no subscriptions: every field is reported\n".into();
        }
        self.subscriptions.iter().map(|p| format!("{p}\n")).collect()
    }

    /// A column for each chosen publication of the first source heard,
    /// newest last; each field shows its value from that publication.
    fn snapshot(&self, count: usize, every: usize, all: bool, now: f64) -> String {
        let Some(publications) = self.publications.values().find(|p| !p.is_empty()) else {
            return "nothing heard yet\n".into();
        };
        let mut columns: Vec<f64> = publications.iter().rev().step_by(every).take(count).map(|&(_, at)| at).collect();
        columns.reverse();
        // A publication's packets arrive together; this takes in all of one
        // and none of the next.
        const SAME: f64 = 0.2;
        let header: Vec<String> = std::iter::once("field".to_string()).chain(columns.iter().map(|at| format!("-{:.1}s", now - at))).collect();
        let rows: Vec<Vec<String>> = self
            .reported(all)
            .map(|(name, samples)| {
                let values = columns.iter().map(|&col| {
                    samples.iter().rev().find(|s| s.at <= col + SAME && s.at > col - SAME).map_or("-".into(), |s| value(s.value as f64))
                });
                std::iter::once(name.clone()).chain(values).collect()
            })
            .collect();
        table(&header, &rows)
    }

    fn stats(&self, window: f64, all: bool, now: f64) -> String {
        let header: Vec<String> = ["field", "n", "last", "min", "mean", "p95", "peak"].map(String::from).to_vec();
        let rows: Vec<Vec<String>> = self
            .reported(all)
            .filter_map(|(name, samples)| {
                let mut values: Vec<f64> = samples.iter().filter(|s| s.at >= now - window).map(|s| s.value as f64).collect();
                let last = *values.last()?;
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                values.sort_by(f64::total_cmp);
                let p95 = values[((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
                Some(vec![name.clone(), values.len().to_string(), value(last), value(values[0]), value(mean), value(p95), value(*values.last()?)])
            })
            .collect();
        if rows.is_empty() {
            return format!("nothing heard in the last {window}s\n");
        }
        table(&header, &rows)
    }
}

/// The pages: each a name and the patterns of the fields it draws, in the
/// order it draws them.
const PAGES: &[(&str, &[&str])] = &[
    ("frame", &["client/frame/*", "client/diag/frame_time", "client/process/memory_mb"]),
    ("memory", &["client/process/*", "client/heap/rust_mb", "client/heap/large_mb", "client/world/tiles", "client/diag/entity_count", "client/cover/models", "client/cover/cards", "client/census/total/triangles"]),
    ("heap", &["client/process/committed_mb", "client/heap/rust_mb", "client/heap/large_mb", "client/heap/site/*"]),
    ("terrain", &["client/world/*", "client/terrain/*"]),
    ("render", &["client/cover/*", "client/census/*", "client/diag/entity_count"]),
    ("passes", &["client/diag/render/*/elapsed_cpu", "client/diag/render/*/elapsed_gpu"]),
    ("network", &["client/network/*"]),
    ("timings", &["client/timings/*.p95"]),
];

/// How many characters a page's line takes.
const LINE: usize = 40;

/// The steps a line is drawn in, lowest first.
const STEPS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl Recorder {
    /// Each field a page's patterns match, in the patterns' order: its
    /// newest value, p95 and least and most over the window, and a line of
    /// the window, each character the most heard in its share of it, from
    /// the least at the bottom to the most at the top.
    fn page(&self, patterns: &[&str], window: f64, now: f64) -> String {
        let header: Vec<String> = ["field", "last", "p95", "min", "max", "line"].map(String::from).to_vec();
        let mut rows = Vec::new();
        for pattern in patterns {
            for (name, samples) in self.fields.iter().filter(|(name, _)| glob(pattern, name)) {
                if rows.iter().any(|row: &Vec<String>| &row[0] == name) {
                    continue;
                }
                let heard: Vec<&Sample> = samples.iter().filter(|s| s.at >= now - window).collect();
                let Some(last) = heard.last() else { continue };
                let mut values: Vec<f64> = heard.iter().map(|s| s.value as f64).collect();
                values.sort_by(f64::total_cmp);
                let (min, max) = (values[0], values[values.len() - 1]);
                let p95 = values[((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
                let mut bins = [None::<f64>; LINE];
                for s in &heard {
                    let bin = (((s.at - (now - window)) / window * LINE as f64) as usize).min(LINE - 1);
                    bins[bin] = Some(bins[bin].map_or(s.value as f64, |b: f64| b.max(s.value as f64)));
                }
                let line: String = bins
                    .iter()
                    .map(|bin| match bin {
                        None => ' ',
                        Some(_) if max == min => STEPS[STEPS.len() / 2],
                        Some(v) => STEPS[(((v - min) / (max - min)) * (STEPS.len() - 1) as f64).round() as usize],
                    })
                    .collect();
                rows.push(vec![name.clone(), value(last.value as f64), value(p95), value(min), value(max), line]);
            }
        }
        if rows.is_empty() {
            return format!("nothing on this page heard in the last {window}s
");
        }
        table(&header, &rows)
    }
}

/// Runs the recorder: a thread joined to each source, and this one taking
/// commands until told to stop.
fn serve() {
    let listener = match TcpListener::bind(CONTROL) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("cannot take commands at {CONTROL} ({e}): is a recorder already running?");
            std::process::exit(1);
        }
    };
    let started = Instant::now();
    let recorder = Arc::new(Mutex::new(Recorder::default()));
    for (source, group) in SOURCES {
        let socket = metrics::subscriber(group).unwrap_or_else(|e| panic!("cannot join {source} at {group}: {e}"));
        let recorder = recorder.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            loop {
                let n = match socket.recv(&mut buf) {
                    Ok(n) => n,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    // Windows reports some send failures on the next
                    // receive; the socket is still joined.
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                };
                if let Some(packet) = MetricsPacket::decode(&buf[..n]) {
                    recorder.lock().unwrap().hear(source, packet, started.elapsed().as_secs_f64());
                }
            }
        });
        println!("recording {source} from {group}");
    }
    println!("taking commands at {CONTROL}");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        let args = std::iter::once("metrics").chain(line.trim_end_matches(['\r', '\n']).split(SEPARATOR).filter(|a| !a.is_empty()));
        let answer = match Cli::try_parse_from(args) {
            Ok(Cli { command: Command::Stop }) => {
                let _ = stream.write_all(b"recorder stopped\n");
                return;
            }
            Ok(cli) => recorder.lock().unwrap().answer(cli.command, started.elapsed().as_secs_f64()),
            Err(e) => e.to_string(),
        };
        let _ = stream.write_all(answer.as_bytes());
    }
}

/// Whether `name` matches `pattern` whole, `*` matching any run of
/// characters.
fn glob(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = name.strip_prefix(first) else { return false };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else { return rest.is_empty() };
    for part in middle {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// A value as short as it reads: whole numbers bare, others to three
/// significant places past the point at most.
fn value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else if v.abs() >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.3}")
    }
}

/// Rows in columns, the first left-aligned and the rest right-aligned.
fn table(header: &[String], rows: &[Vec<String>]) -> String {
    let all = std::iter::once(header).filter(|h| !h.is_empty()).chain(rows.iter().map(Vec::as_slice));
    let mut widths: Vec<usize> = Vec::new();
    for row in all.clone() {
        for (i, cell) in row.iter().enumerate() {
            if widths.len() <= i {
                widths.push(0);
            }
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for row in all {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| if i == 0 { format!("{cell:<w$}", w = widths[i]) } else { format!("{cell:>w$}", w = widths[i]) })
            .collect();
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_matches_the_whole_name() {
        assert!(glob("client/frame/*", "client/frame/p95_ms"));
        assert!(glob("*memory*", "client/process/memory_mb"));
        assert!(glob("client/world/tiles", "client/world/tiles"));
        assert!(!glob("client/world/tile", "client/world/tiles"));
        assert!(!glob("*/fps", "client/frame/fps_extra"));
        assert!(glob("a*b*c", "a__b__c"));
        assert!(!glob("a*b*c", "a__c__b"));
    }

    /// A field's values from one publication sit in one column, and a
    /// publication is counted once however many packets carry it.
    #[test]
    fn a_snapshot_lines_up_each_publication() {
        let packet = |group: &'static str, ts: f64, v: f32| MetricsPacket {
            group: group.into(),
            cadence: metrics::Cadence::Snapshot,
            timestamp_secs: ts,
            fields: vec![("x".into(), v)],
        };
        let mut recorder = Recorder::default();
        for (i, ts) in [1.0, 1.5, 2.0].into_iter().enumerate() {
            let at = i as f64 * 0.5;
            recorder.hear("client", packet("a", ts, i as f32), at);
            recorder.hear("client", packet("b", ts, 10.0 + i as f32), at + 0.001);
        }
        assert_eq!(recorder.publications["client"].len(), 3);
        let shown = recorder.snapshot(2, 1, false, 1.0);
        let lines: Vec<Vec<&str>> = shown.lines().map(|l| l.split_whitespace().collect()).collect();
        assert_eq!(lines[1], vec!["client/a/x", "1", "2"]);
        assert_eq!(lines[2], vec!["client/b/x", "11", "12"]);
    }
}
