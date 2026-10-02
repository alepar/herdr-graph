//! Level-triggered structural differ (spec §4.3): snapshots → observed mutations; session capture; session-ended hook.
pub mod baseline;
pub mod classify;
pub mod diff;
pub mod matcher;
pub mod mutations;
pub mod sessions;
pub mod step;
#[cfg(test)]
mod tests;

pub use mutations::register_mutations;
pub use sessions::{SessionCapture, capture_session};
pub use step::{RuntimeLoop, SessionEnded, StepSummary};
