//! Herdr runtime: NDJSON socket client (HerdrApi impl), FakeHerdr, test isolation guard (spec §4.1, §11).
pub mod client;
pub mod fake;
pub mod incarnation;
pub mod isolation;
#[cfg(test)]
mod tests;
pub mod wire;
pub use client::HerdrClient;
pub use fake::FakeHerdr;
