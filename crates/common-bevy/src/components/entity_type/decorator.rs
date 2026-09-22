use common::Cover;
use serde::{Deserialize, Serialize};

/// A tile's ground as an entity: what stands in its seven slots, and
/// whether it is solid, which a walker cannot enter where it has no floor.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Decorator {
    pub cover: Cover,
    pub is_solid: bool,
}
