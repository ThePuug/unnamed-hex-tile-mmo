//! Whose a construct is, as the store keeps it: its builder by its
//! provider's name for it, or the camp it is of by its hearth's tile.

use common::network::Identity;

use super::store::BuiltRow;
use crate::systems::building::{Built, Owner};

/// A construct's record as a row.
pub fn built(built: Built) -> BuiltRow {
    match built.by {
        Owner::Player(by) => BuiltRow { provider: Some(by.provider().to_owned()), name: Some(by.name()), camp: None },
        Owner::Camp(q, r) => BuiltRow { provider: None, name: None, camp: Some((q, r)) },
    }
}

/// The record `row` keeps, or none where it names neither a camp nor a
/// player of a provider this build knows.
pub fn rebuilt(row: BuiltRow) -> Option<Built> {
    let by = match row.camp {
        Some((q, r)) => Owner::Camp(q, r),
        None => Owner::Player(Identity::named(&row.provider?, &row.name?)?),
    };
    Some(Built { by })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owner_is_recalled_as_it_was_kept() {
        for by in [Owner::Player(Identity::Key([9; 16])), Owner::Camp(40, -7)] {
            assert_eq!(rebuilt(built(Built { by })), Some(Built { by }));
        }
        assert_eq!(rebuilt(BuiltRow { provider: Some("steam".into()), name: Some("ab".into()), camp: None }), None);
    }
}
