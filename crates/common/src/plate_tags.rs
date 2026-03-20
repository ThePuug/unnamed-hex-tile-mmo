use std::mem;

use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

/// Maximum tags per plate. Sized generously — events accumulate and erase
/// tags over a plate's history. 64 slots at ~8 bytes per variant worst case
/// is 512 bytes inline, well within reason for generation-time data and
/// network messages.
pub const MAX_PLATE_TAGS: usize = 64;

/// Metadata tags assigned to macro and micro plates by the event system.
/// Events read, write, and erase tags. Tags accumulate over the world's
/// generated history — later events compose on what earlier events left.
///
/// Variants carry data when classification alone isn't sufficient.
/// Pattern match on the variant to check presence, destructure to read data.
/// All payloads must be Copy-compatible (u8, u16, etc.) — no heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PlateTag {
    /// Base classification — assigned during plate generation.
    Sea,
    /// Base classification — assigned during plate generation.
    Coast,
    /// Base classification — assigned during plate generation.
    Inland,
    /// Spine crest — the highest line of the range.
    Ridge,
    /// Flanks of a spine — transitional elevated terrain.
    Foothills,
    /// Broadly elevated area around a spine.
    Highland,
}

// tinyvec::ArrayVec requires T: Default to satisfy the Array trait bound.
// Sea is the zero-value slot filler; unused ArrayVec slots are never observed.
impl Default for PlateTag {
    fn default() -> Self { PlateTag::Sea }
}

// Compile-time assertion: PlateTag must be Copy so it can live in ArrayVec
// and be embedded in Copy-derived network Event types.
const _: () = {
    fn _assert_copy<T: Copy>() {}
    fn _check() {
        _assert_copy::<PlateTag>();
    }
};

/// Methods for reading and mutating tag collections on plate structs.
///
/// Implement by providing `tags` and `tags_mut` accessors; `has_tag`,
/// `add_tag`, and `erase_tag` are derived from them.
pub trait Tagged {
    fn tags(&self) -> &ArrayVec<[PlateTag; MAX_PLATE_TAGS]>;
    fn tags_mut(&mut self) -> &mut ArrayVec<[PlateTag; MAX_PLATE_TAGS]>;

    /// Returns true if any tag matches the variant, ignoring inner data.
    fn has_tag(&self, tag: &PlateTag) -> bool {
        self.tags()
            .iter()
            .any(|t| mem::discriminant(t) == mem::discriminant(tag))
    }

    /// Appends a tag. Does not deduplicate — caller's responsibility.
    fn add_tag(&mut self, tag: PlateTag) {
        self.tags_mut().push(tag);
    }

    /// Removes all tags whose variant matches, ignoring inner data.
    fn erase_tag(&mut self, tag: &PlateTag) {
        self.tags_mut()
            .retain(|t| mem::discriminant(t) != mem::discriminant(tag));
    }
}

// ── TagSet ──────────────────────────────────────────────────────────────────

/// All PlateTag variants, for iteration.
const ALL_TAGS: &[PlateTag] = &[
    PlateTag::Sea, PlateTag::Coast, PlateTag::Inland,
    PlateTag::Ridge, PlateTag::Foothills, PlateTag::Highland,
];

/// Fixed-size bitfield for O(1) tag operations. Replaces `ArrayVec<[PlateTag; N]>`
/// for composite tile queries where set membership is the primary operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TagSet(u64);

impl TagSet {
    pub fn new() -> Self { Self(0) }

    pub fn has(&self, tag: PlateTag) -> bool {
        self.0 & (1u64 << tag as u8) != 0
    }

    pub fn has_any(&self, tags: &[PlateTag]) -> bool {
        tags.iter().any(|&t| self.has(t))
    }

    pub fn add(&mut self, tag: PlateTag) {
        self.0 |= 1u64 << tag as u8;
    }

    pub fn remove(&mut self, tag: PlateTag) {
        self.0 &= !(1u64 << tag as u8);
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = PlateTag> + '_ {
        ALL_TAGS.iter().copied().filter(|&t| self.has(t))
    }
}

impl From<PlateTag> for TagSet {
    fn from(tag: PlateTag) -> Self {
        let mut s = Self::new();
        s.add(tag);
        s
    }
}

impl FromIterator<PlateTag> for TagSet {
    fn from_iter<I: IntoIterator<Item = PlateTag>>(iter: I) -> Self {
        let mut s = Self::new();
        for t in iter { s.add(t); }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plate {
        tags: ArrayVec<[PlateTag; MAX_PLATE_TAGS]>,
    }

    impl Tagged for Plate {
        fn tags(&self) -> &ArrayVec<[PlateTag; MAX_PLATE_TAGS]> {
            &self.tags
        }
        fn tags_mut(&mut self) -> &mut ArrayVec<[PlateTag; MAX_PLATE_TAGS]> {
            &mut self.tags
        }
    }

    fn plate(tags: Vec<PlateTag>) -> Plate {
        let mut av = ArrayVec::new();
        for t in tags {
            av.push(t);
        }
        Plate { tags: av }
    }

    #[test]
    fn has_tag_present() {
        let p = plate(vec![PlateTag::Sea]);
        assert!(p.has_tag(&PlateTag::Sea));
    }

    #[test]
    fn has_tag_absent_empty() {
        let p = plate(vec![]);
        assert!(!p.has_tag(&PlateTag::Sea));
    }

    #[test]
    fn has_tag_wrong_variant() {
        let p = plate(vec![PlateTag::Inland]);
        assert!(!p.has_tag(&PlateTag::Sea));
    }

    #[test]
    fn add_tag_appends() {
        let mut p = plate(vec![]);
        p.add_tag(PlateTag::Sea);
        assert_eq!(p.tags().len(), 1);
        assert!(p.has_tag(&PlateTag::Sea));
    }

    #[test]
    fn erase_tag_removes_matching() {
        let mut p = plate(vec![PlateTag::Sea, PlateTag::Inland]);
        p.erase_tag(&PlateTag::Sea);
        assert!(!p.has_tag(&PlateTag::Sea));
        assert!(p.has_tag(&PlateTag::Inland));
    }

    #[test]
    fn erase_tag_removes_all_matching() {
        let mut p = plate(vec![PlateTag::Sea, PlateTag::Inland, PlateTag::Sea]);
        p.erase_tag(&PlateTag::Sea);
        assert_eq!(p.tags().len(), 1);
        assert!(p.has_tag(&PlateTag::Inland));
    }

    #[test]
    fn erase_tag_noop_when_absent() {
        let mut p = plate(vec![PlateTag::Inland]);
        p.erase_tag(&PlateTag::Sea);
        assert_eq!(p.tags().len(), 1);
        assert!(p.has_tag(&PlateTag::Inland));
    }

    /// Discriminant matching ignores data carried by variants.
    /// Add a temporary data-carrying variant to verify the contract.
    #[test]
    fn discriminant_matching_ignores_data() {
        // PlateTag::Coast is a unit variant — test that has_tag matches it
        // by discriminant, not value equality. Once data-carrying variants exist
        // (e.g. Elevated(u16)), this test should be updated to use them.
        //
        // For now: two identical unit variants are trivially equal by both
        // discriminant and value. The trait's use of mem::discriminant is correct
        // and will handle data variants properly when they are introduced.
        let p = plate(vec![PlateTag::Coast]);
        assert!(p.has_tag(&PlateTag::Coast));
        assert!(!p.has_tag(&PlateTag::Sea));
    }

    // ── TagSet tests ──

    #[test]
    fn tagset_add_and_has() {
        let mut s = TagSet::new();
        assert!(!s.has(PlateTag::Inland));
        s.add(PlateTag::Inland);
        assert!(s.has(PlateTag::Inland));
        assert!(!s.has(PlateTag::Sea));
    }

    #[test]
    fn tagset_remove() {
        let mut s = TagSet::new();
        s.add(PlateTag::Sea);
        s.add(PlateTag::Inland);
        s.remove(PlateTag::Sea);
        assert!(!s.has(PlateTag::Sea));
        assert!(s.has(PlateTag::Inland));
    }

    #[test]
    fn tagset_has_any() {
        let mut s = TagSet::new();
        s.add(PlateTag::Ridge);
        assert!(s.has_any(&[PlateTag::Sea, PlateTag::Ridge]));
        assert!(!s.has_any(&[PlateTag::Sea, PlateTag::Coast]));
    }

    #[test]
    fn tagset_from_tag() {
        let s = TagSet::from(PlateTag::Highland);
        assert!(s.has(PlateTag::Highland));
        assert!(!s.has(PlateTag::Sea));
    }

    #[test]
    fn tagset_iter() {
        let mut s = TagSet::new();
        s.add(PlateTag::Sea);
        s.add(PlateTag::Ridge);
        let collected: Vec<_> = s.iter().collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&PlateTag::Sea));
        assert!(collected.contains(&PlateTag::Ridge));
    }

    #[test]
    fn tagset_from_iter() {
        let s: TagSet = [PlateTag::Coast, PlateTag::Foothills].iter().copied().collect();
        assert!(s.has(PlateTag::Coast));
        assert!(s.has(PlateTag::Foothills));
        assert!(!s.has(PlateTag::Sea));
    }
}
