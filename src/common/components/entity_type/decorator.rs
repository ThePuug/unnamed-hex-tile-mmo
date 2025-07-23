use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecoratorImpl {
    pub index: usize,
    pub is_solid: bool,
}
