//! Templates, applications, effective desired structure (spec §5). Owned by hg-zmi.9.
pub mod document;
pub mod kinds;
pub mod structure;
pub mod withdrawal;

#[cfg(test)]
mod tests;

pub use kinds::register_kinds;
