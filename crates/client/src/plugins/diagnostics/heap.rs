//! Debug builds: where the Rust heap is held, published as the `heap`
//! topic. Every allocation is counted while it lives (`rust_mb`); each of
//! `LARGE` bytes or more is kept with the stack that made it until it is
//! freed, and the stacks holding the most are published by the functions
//! that made them (`site/<caller < its caller < …>`, megabytes, and `#n`,
//! how many). A leak is a site whose bytes only grow.
//!
//! A thread resolves the stacks to names, never the allocating one, so
//! reading the symbols holds no frame up.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, Once,
    },
    time::Duration,
};

/// The smallest allocation kept with its stack.
const LARGE: usize = 1 << 20;

/// How many return addresses a stack keeps: enough to pass the generic
/// layers (`Vec`, `HashMap`, the asset store) and reach who asked.
const FRAMES: usize = 32;

/// How many of the sites holding the most are published.
const SITES: usize = 12;

/// How many named functions a site's name carries.
const NAMED: usize = 3;

const MB: f64 = 1_048_576.0;

pub struct Tracking;

#[global_allocator]
static ALLOCATOR: Tracking = Tracking;

static LIVE: AtomicUsize = AtomicUsize::new(0);

type Stack = [usize; FRAMES];

#[derive(Default)]
struct Held {
    /// Each large allocation still live: its size and its stack.
    allocations: HashMap<usize, (usize, Stack)>,
    /// Each stack holding a large allocation: its bytes and how many.
    sites: HashMap<Stack, (usize, usize)>,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

/// The sites holding the most, named, as of the resolver's last pass.
static REPORT: Mutex<Vec<(String, f64, usize)>> = Mutex::new(Vec::new());

thread_local! {
    /// Set while this thread is inside the tracker, or is the resolver:
    /// what it allocates there is counted and never kept, or the tracker
    /// would recurse into itself.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` unless this thread is inside the tracker already.
fn outside(f: impl FnOnce()) {
    let _ = INSIDE.try_with(|inside| {
        if !inside.replace(true) {
            f();
            inside.set(false);
        }
    });
}

fn made(ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    LIVE.fetch_add(size, Ordering::Relaxed);
    if size < LARGE {
        return;
    }
    outside(|| {
        let mut stack = [0usize; FRAMES];
        let mut n = 0;
        // Unsynchronized: the synchronized trace waits on the lock the
        // resolver holds while it reads symbols, seconds the first time.
        // Walking the stack needs no symbols.
        unsafe {
            backtrace::trace_unsynchronized(|frame| {
                stack[n] = frame.ip() as usize;
                n += 1;
                n < FRAMES
            });
        }
        let mut held = HELD.lock().unwrap();
        let held = held.get_or_insert_with(Held::default);
        held.allocations.insert(ptr as usize, (size, stack));
        let site = held.sites.entry(stack).or_default();
        site.0 += size;
        site.1 += 1;
    });
}

fn freed(ptr: *mut u8, size: usize) {
    LIVE.fetch_sub(size, Ordering::Relaxed);
    if size < LARGE {
        return;
    }
    outside(|| {
        let mut held = HELD.lock().unwrap();
        let Some(held) = held.as_mut() else { return };
        let Some((size, stack)) = held.allocations.remove(&(ptr as usize)) else { return };
        if let Some(site) = held.sites.get_mut(&stack) {
            site.0 -= size;
            site.1 -= 1;
            if site.1 == 0 {
                held.sites.remove(&stack);
            }
        }
    });
}

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        made(ptr, layout.size());
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        made(ptr, layout.size());
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        freed(ptr, layout.size());
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(ptr, layout, new_size) };
        if !moved.is_null() {
            freed(ptr, layout.size());
            made(moved, new_size);
        }
        moved
    }
}

/// The heap topic's fields: everything live, everything large, and the
/// sites holding the most. Starts the resolver on first call.
pub fn report() -> Vec<(Cow<'static, str>, f32)> {
    static RESOLVER: Once = Once::new();
    RESOLVER.call_once(|| {
        std::thread::Builder::new().name("heap sites".into()).spawn(resolve).expect("a thread to name heap sites on");
    });
    let mut fields = vec![(Cow::Borrowed("rust_mb"), (LIVE.load(Ordering::Relaxed) as f64 / MB) as f32)];
    let large: usize = outside_lock(|held| held.sites.values().map(|&(bytes, _)| bytes).sum());
    fields.push((Cow::Borrowed("large_mb"), (large as f64 / MB) as f32));
    for (name, mb, count) in REPORT.lock().unwrap().iter() {
        fields.push((Cow::Owned(format!("site/{name}")), *mb as f32));
        fields.push((Cow::Owned(format!("site/{name}#n")), *count as f32));
    }
    fields
}

/// Reads what is held, inside the tracker so nothing this allocates is kept.
fn outside_lock<T: Default>(f: impl FnOnce(&Held) -> T) -> T {
    let mut out = T::default();
    outside(|| {
        if let Some(held) = HELD.lock().unwrap().as_ref() {
            out = f(held);
        }
    });
    out
}

/// The resolver: names the sites holding the most, every second.
fn resolve() {
    let _ = INSIDE.try_with(|inside| inside.set(true));
    let mut names: HashMap<Stack, String> = HashMap::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let mut top: Vec<(Stack, usize, usize)> = HELD
            .lock()
            .unwrap()
            .as_ref()
            .map(|held| held.sites.iter().map(|(stack, &(bytes, count))| (*stack, bytes, count)).collect())
            .unwrap_or_default();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.truncate(SITES);
        // Two stacks can name alike; their bytes add.
        let mut named: Vec<(String, f64, usize)> = Vec::new();
        for (stack, bytes, count) in top {
            let name = names.entry(stack).or_insert_with(|| site_name(&stack)).clone();
            match named.iter_mut().find(|(n, _, _)| *n == name) {
                Some(entry) => {
                    entry.1 += bytes as f64 / MB;
                    entry.2 += count;
                }
                None => named.push((name, bytes as f64 / MB, count)),
            }
        }
        *REPORT.lock().unwrap() = named;
    }
}

/// The first `NAMED` functions on the stack past the allocator and the
/// standard library, innermost first, each by its last two path segments
/// without generic arguments.
fn site_name(stack: &Stack) -> String {
    // The generic layers every allocation passes through: the standard
    // library's paths, and the names MSVC gives trait impls and enums.
    const PASSED: [&str; 10] = ["backtrace::", "alloc::", "core::", "std::", "__rust", "client::plugins::diagnostics::heap", "hashbrown::", "impl$", "enum2$", "Global::"];
    let mut functions: Vec<String> = Vec::new();
    for &ip in stack.iter().take_while(|&&ip| ip != 0) {
        if functions.len() == NAMED {
            break;
        }
        backtrace::resolve(ip as *mut std::ffi::c_void, |symbol| {
            let Some(name) = symbol.name() else { return };
            let full = format!("{name:#}");
            let bare = without_generics(full.trim_start_matches('<'));
            let short = shortened(&bare);
            if PASSED.iter().any(|p| bare.starts_with(p) || short.starts_with(p)) || functions.last() == Some(&short) {
                return;
            }
            if functions.len() < NAMED {
                functions.push(short);
            }
        });
    }
    if functions.is_empty() {
        return "unnamed".into();
    }
    functions.join(" < ")
}

fn without_generics(name: &str) -> String {
    let mut depth = 0;
    name.chars()
        .filter(|&c| {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                _ => return depth == 0,
            }
            false
        })
        .collect()
}

fn shortened(path: &str) -> String {
    let segments: Vec<&str> = path.split("::").filter(|s| !s.is_empty() && !s.starts_with("{{closure}}")).collect();
    segments[segments.len().saturating_sub(2)..].join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_loses_its_generics_and_keeps_two_segments() {
        assert_eq!(shortened(&without_generics("bevy_render::render_asset::prepare_assets<bevy_mesh::mesh::Mesh>")), "render_asset::prepare_assets");
        assert_eq!(shortened(&without_generics("music::render::Bank::font::{{closure}}")), "Bank::font");
        assert_eq!(shortened(&without_generics("main")), "main");
    }
}
