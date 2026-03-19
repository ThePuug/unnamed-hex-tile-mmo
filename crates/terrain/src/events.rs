//! # World Event System
//!
//! Events are lazy evaluation layers that read prior layers' caches and produce
//! per-chunk data. Each event type owns its own cache with LRU eviction.
//! The "substrate" is just the read path into prior caches — no separate data structure.
//!
//! ## Chunk Grid
//!
//! Each event type defines a hex chunk grid scale. Chunks use offset hex layout
//! (odd rows shifted right by half). The 1-ring neighborhood (center + 6 neighbors)
//! is the complete input to every evaluation — no unbounded lookups.

use std::collections::HashMap;

/// Hex row height factor: sqrt(3)/2
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

// ── Hex chunk grid utilities ──

/// Convert world coordinates to chunk coordinates at a given scale.
pub fn chunk_coord(wx: f64, wy: f64, scale: f64) -> (i32, i32) {
    let row_height = scale * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { scale * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / scale).round() as i32;
    (cq, cr)
}

/// Center of a chunk in world coordinates at a given scale.
pub fn chunk_center(cq: i32, cr: i32, scale: f64) -> (f64, f64) {
    let odd_shift = if cr & 1 != 0 { scale * 0.5 } else { 0.0 };
    (
        cq as f64 * scale + odd_shift,
        cr as f64 * scale * HEX_ROW_HEIGHT,
    )
}

/// The 1-ring neighborhood (center + 6 neighbors) for offset hex grids.
/// Returns 7 (dq, dr) offsets. Odd/even row parity affects neighbor positions.
pub fn chunk_1ring(cr: i32) -> [(i32, i32); 7] {
    if cr & 1 == 0 {
        [(0, 0), (-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    } else {
        [(0, 0), (-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    }
}

// ── Event Cache Metrics ──

/// Auto-instrumented metrics for an EventCache. All counters are cumulative
/// lifetime totals (monotonically increasing).
#[derive(Default, Clone)]
pub struct EventCacheMetrics {
    pub chunks_evaluated: u64,
    pub chunks_with_output: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

// ── Event Cache ──

/// Generic LRU cache for event chunk data.
/// Each event type wraps this with its own `ChunkData` type.
/// Auto-instrumented via `EventCacheMetrics`.
pub struct EventCache<T> {
    chunks: HashMap<(i32, i32), CacheEntry<T>>,
    access_counter: u64,
    max_chunks: usize,
    pub metrics: EventCacheMetrics,
}

struct CacheEntry<T> {
    data: T,
    last_accessed: u64,
}

impl<T> EventCache<T> {
    pub fn new(max_chunks: usize) -> Self {
        Self {
            chunks: HashMap::new(),
            access_counter: 0,
            max_chunks,
            metrics: EventCacheMetrics::default(),
        }
    }

    /// Get or evaluate a chunk. Calls `evaluate_fn` on cache miss.
    pub fn get_or_evaluate(
        &mut self,
        cq: i32, cr: i32,
        evaluate_fn: &mut impl FnMut(i32, i32) -> T,
    ) -> &T {
        self.access_counter += 1;
        let stamp = self.access_counter;

        if !self.chunks.contains_key(&(cq, cr)) {
            self.metrics.cache_misses += 1;
            let data = evaluate_fn(cq, cr);
            self.metrics.chunks_evaluated += 1;
            self.chunks.insert((cq, cr), CacheEntry { data, last_accessed: stamp });
            self.evict_if_over_budget();
        } else {
            self.metrics.cache_hits += 1;
        }

        let entry = self.chunks.get_mut(&(cq, cr)).unwrap();
        entry.last_accessed = stamp;
        &entry.data
    }

    /// Ensure a chunk is populated, calling evaluate_fn on cache miss.
    /// `has_output` tests whether the result counts as non-empty output.
    pub fn ensure(
        &mut self, cq: i32, cr: i32,
        evaluate_fn: &mut impl FnMut(i32, i32) -> T,
        has_output: impl FnOnce(&T) -> bool,
    ) {
        if self.chunks.contains_key(&(cq, cr)) {
            self.metrics.cache_hits += 1;
            return;
        }
        self.metrics.cache_misses += 1;
        self.access_counter += 1;
        let data = evaluate_fn(cq, cr);
        self.metrics.chunks_evaluated += 1;
        if has_output(&data) {
            self.metrics.chunks_with_output += 1;
        }
        self.chunks.insert((cq, cr), CacheEntry { data, last_accessed: self.access_counter });
        self.evict_if_over_budget();
    }

    /// Ensure the 1-ring neighborhood is populated.
    pub fn ensure_1ring(
        &mut self, cq: i32, cr: i32,
        evaluate_fn: &mut impl FnMut(i32, i32) -> T,
        has_output: impl Fn(&T) -> bool,
    ) {
        for (dq, dr) in chunk_1ring(cr) {
            self.ensure(cq + dq, cr + dr, evaluate_fn, &has_output);
        }
    }

    /// Access a cached chunk's data (must exist).
    pub fn get(&self, cq: i32, cr: i32) -> Option<&T> {
        self.chunks.get(&(cq, cr)).map(|e| &e.data)
    }

    /// Touch a chunk to update its LRU timestamp.
    pub fn touch(&mut self, cq: i32, cr: i32) {
        self.access_counter += 1;
        if let Some(entry) = self.chunks.get_mut(&(cq, cr)) {
            entry.last_accessed = self.access_counter;
        }
    }

    fn evict_if_over_budget(&mut self) {
        if self.chunks.len() <= self.max_chunks { return; }
        let lru_key = self.chunks.iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(&key, _)| key);
        if let Some(key) = lru_key {
            self.chunks.remove(&key);
        }
    }
}
