//! Change requests → concrete plans, plan hashing, plan store, staleness (spec §3.3). Owned by hg-zmi.6.

pub mod kinds_extra;
pub mod reminders;

pub mod apply;
pub mod commands;
pub mod core_kinds;
pub mod grammar;
pub mod hash;
pub mod kind;
pub mod store;
pub mod types;

#[cfg(test)]
mod tests;
