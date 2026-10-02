//! Committed-revision reads (git2), instance path layout, name→path slugging (spec §1, §2.1–2.3).
//! Implements crate::ports::store::Store. Readers only ever see complete commits; nothing here reads the working tree.
pub mod git;
pub mod init;
pub mod layout;
pub mod record;
pub mod slug;
pub mod tree;
#[cfg(test)]
mod tests;

pub use git::GitStore;
pub use record::Record;
pub use tree::{CommitView, Edit, EditSet, Overlay, TreeRead};
