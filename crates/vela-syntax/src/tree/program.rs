//! The parsed file.

use crate::tree::Item;

/// A parsed file: its top-level items, in source order.
#[derive(Debug, Default)]
pub struct Program {
    /// The items.
    pub items: Vec<Item>,
}
